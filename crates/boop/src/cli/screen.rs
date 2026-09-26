//! `boop lane squares`: a lane's pane frame (boop_harness::pane), the same frame instant pushes.

use std::path::Path;

use anyhow::Result;

use boop::bus;
use boop::tmux;
use boop_harness::pane::{self, Options};

use crate::cli::db::open_store;
use crate::cli::{line, mail_dir};
use crate::QueryFormat;

pub(crate) fn run_lane_squares(
    mail_dir_arg: Option<&Path>,
    lane: &str,
    format: QueryFormat,
    socket: Option<&str>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let routes = bus::read_routes(&dir)?;
    let Some(route) = routes.get(lane) else {
        anyhow::bail!("no registry route for lane `{lane}`")
    };
    let Some(target) = route.tmux.as_deref().filter(|target| !target.is_empty()) else {
        anyhow::bail!("lane `{lane}` has no tmux session to read")
    };
    let Some(session) = route.session_id.as_deref().filter(|session| !session.is_empty()) else {
        anyhow::bail!("lane `{lane}` route carries no session_id")
    };
    let frame = pane::frame(tmux::mux(), socket, target, &open_store()?, session, &Options::default())?;
    match format {
        QueryFormat::Ndjson => line(&serde_json::to_string(&frame)?),
        QueryFormat::Text => emit_text(lane, &frame),
    }
    Ok(())
}

/// One value per line; `said` flattened to one line and cut to 120 chars.
fn emit_text(lane: &str, frame: &pane::PaneFrame) {
    line(&format!("lane\t{lane}"));
    line(&format!("session\t{}", frame.session));
    line(&format!("pane\t{}", frame.pane));
    line(&format!("rows\t{}", frame.rows));
    if let Some(window) = frame.window {
        line(&format!("window\t{}\t{}", window.top, window.bottom));
    }
    line("role\tturn\tbuffer_start\tbuffer_end\tconfidence\tsaid");
    for turn in frame.pinned.iter().chain(&frame.turns) {
        let said: String = turn.said.replace(['\n', '\r', '\t'], " ").chars().take(120).collect();
        line(&format!(
            "{}\t{}\t{}\t{}\t{:?}\t{}",
            turn.role, turn.turn, turn.buffer_start, turn.buffer_end, turn.confidence, said
        ));
    }
}
