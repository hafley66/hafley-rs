//! `boop lane squares`: the user/agent squares on a lane's pane, read through
//! the multiplexer rather than handed in.
//!
//! The caller is a renderer that draws them in the terminal's right margin. It
//! gets the pane's own geometry with them, so it never has to ask the terminal
//! for its shape separately and never disagrees with the snapshot it is
//! overlaying.

use std::path::Path;

use anyhow::{Context, Result};

use boop::bus;
use boop::{screen, tmux};

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
    let Some(session) = route
        .session_id
        .as_deref()
        .filter(|session| !session.is_empty())
    else {
        anyhow::bail!("lane `{lane}` route carries no session_id")
    };
    // The pane answers first: a dead pane is a missing screen, not an empty
    // one, and a renderer must not draw an overlay over nothing.
    let snapshot = tmux::mux()
        .pane_snapshot(socket, target)
        .with_context(|| format!("lane `{lane}` pane {target} answered no snapshot"))?;
    let rows = open_store()?.turn_rows(&boop::ident::TurnQuery {
        session: Some(session.to_owned()),
        ..Default::default()
    })?;
    let state = screen::screen_state(lane, session, snapshot, &rows);
    match format {
        QueryFormat::Ndjson => line(&serde_json::to_string(&state)?),
        QueryFormat::Text => emit_text(&state),
    }
    Ok(())
}

/// Text output stays one value per line, so a preview's own newlines and tabs
/// become spaces. The ndjson form is the one that carries the exact text.
fn emit_text(state: &screen::ScreenState) {
    line(&format!("lane\t{}", state.lane));
    line(&format!("session\t{}", state.session));
    line(&format!("pane\t{}", state.snapshot.target.terminal));
    line(&format!("columns\t{}", state.snapshot.size.columns));
    line(&format!("rows\t{}", state.snapshot.size.rows));
    line(&format!("screen\t{:?}", state.snapshot.screen));
    line("role\tturn\tviewport_start\tviewport_end\tpreview");
    for square in &state.squares {
        line(&format!(
            "{}\t{}\t{}\t{}\t{}",
            square.role,
            square.turn,
            square.viewport_start,
            square.viewport_end,
            square.preview.replace(['\n', '\r', '\t'], " ")
        ));
    }
}
