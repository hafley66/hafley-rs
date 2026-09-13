//! Persistent recipient selection and focus observations for registered tmux
//! routes.
//!
//! A route's checkbox and focus stamp live in `agent_route_selection`, keyed by
//! route name. The table's `ON DELETE CASCADE` is the ownership rule: a route
//! rewrite updates the same `agent_route` row and keeps the child, while
//! deleting the route removes the child so a re-created route starts clean.
use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result};
use boop::{bus, ident::Store};
use clap::Subcommand;
use rusqlite::OptionalExtension;
use serde::Serialize;

/// The field separator the pane snapshot uses. tmux rewrites control bytes in
/// its format output under a `C` locale (tab and unit separator both become
/// `_`), so the separator must be printable. Colon is the one printable byte a
/// tmux session name cannot hold, and indices, pane ids and the 0/1 flags
/// never do; the title is placed last and split off as a remainder, so a colon
/// inside a title changes nothing.
const FIELD_SEP: char = ':';

#[derive(Subcommand)]
pub enum SelectionCmd {
    /// List live Boop-controlled panes, most recently focused first.
    List,
    /// Change one route's checkbox without replacing other selections.
    Set {
        route: String,
        #[arg(long)]
        checked: bool,
    },
    /// Record a human focus event for a tmux session or pane.
    Focus {
        target: String,
        #[arg(long)]
        at: Option<i64>,
    },
    /// Clear the selected recipient set.
    Clear,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelectionRow {
    route: String,
    session: String,
    pane: String,
    title: String,
    last_focused_at: Option<i64>,
    selected: bool,
}

/// One live tmux pane, as the snapshot reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PaneObs {
    id: String,
    session: String,
    /// `session:window.pane`, the spelling a route may store.
    target: String,
    title: String,
    window_active: bool,
    pane_active: bool,
}

/// One registered Boop route resolved onto a live pane.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ResolvedRoute {
    route: String,
    session: String,
    pane: String,
    target: String,
    title: String,
}

impl ResolvedRoute {
    fn row(&self, last_focused_at: Option<i64>, selected: bool) -> SelectionRow {
        SelectionRow {
            route: self.route.clone(),
            session: self.session.clone(),
            pane: self.pane.clone(),
            title: self.title.clone(),
            last_focused_at,
            selected,
        }
    }
}

fn open(dir: &Path) -> Result<Store> {
    let store = bus::open_store(dir)?;
    // The FK is relational, not decorative: `bus::upsert_route` updates on
    // conflict so a route rewrite keeps the child row, and deleting the route
    // cascades the selection away. A re-created route therefore starts clean
    // instead of inheriting a dead route's checkbox.
    store
        .connection()
        .pragma_update(None, "foreign_keys", "ON")?;
    store.connection().execute_batch(
        "CREATE TABLE IF NOT EXISTS agent_route_selection (
            route TEXT PRIMARY KEY REFERENCES agent_route(route) ON DELETE CASCADE,
            last_focused_at INTEGER,
            selected INTEGER NOT NULL DEFAULT 0 CHECK(selected IN (0,1))
        ) WITHOUT ROWID;
        CREATE TRIGGER IF NOT EXISTS agent_route_selection_delete
        AFTER DELETE ON agent_route BEGIN
            DELETE FROM agent_route_selection WHERE route=OLD.route;
        END;",
    )?;
    Ok(store)
}

/// Parse one `tmux list-panes` snapshot. Rows the fields do not fully cover are
/// dropped rather than guessed at, and a dead pane is not a recipient.
fn parse_panes(text: &str) -> Vec<PaneObs> {
    text.lines()
        .filter_map(|line| {
            let mut fields = line.splitn(8, FIELD_SEP);
            let id = fields.next()?.to_owned();
            let session = fields.next()?.to_owned();
            let window = fields.next()?;
            let pane = fields.next()?;
            let dead = fields.next()?;
            let window_active = fields.next()?;
            let pane_active = fields.next()?;
            // Title is the remainder so a separator inside it is preserved.
            let title = fields.next().unwrap_or_default().to_owned();
            if id.is_empty() || dead != "0" {
                return None;
            }
            Some(PaneObs {
                id,
                target: format!("{session}:{window}.{pane}"),
                session,
                title,
                window_active: window_active == "1",
                pane_active: pane_active == "1",
            })
        })
        .collect()
}

/// Take one pane snapshot from tmux. `Ok((panes, None))` is a live server,
/// `Ok((empty, Some(why)))` a positively identified absent server, and `Err` a
/// hard failure (tmux missing or not executable, permission denied, any other
/// tmux error) that the caller must surface rather than read as "no panes".
fn pane_snapshot() -> Result<(Vec<PaneObs>, Option<String>)> {
    let format = [
        "#{pane_id}",
        "#{session_name}",
        "#{window_index}",
        "#{pane_index}",
        "#{pane_dead}",
        "#{window_active}",
        "#{pane_active}",
        "#{pane_title}",
    ]
    .join(&FIELD_SEP.to_string());
    let output = Command::new("tmux")
        .args(["list-panes", "-a", "-F", &format])
        .output()
        .context("run tmux list-panes")?;
    if output.status.success() {
        return Ok((parse_panes(&String::from_utf8_lossy(&output.stdout)), None));
    }
    let why = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if tmux_server_absent(&why) {
        return Ok((Vec::new(), Some(why)));
    }
    anyhow::bail!("tmux list-panes failed: {why}");
}

/// Whether a failing tmux command names the one case an empty list is right
/// for: no server on the selected socket. A permission error or any other
/// diagnostic is not an absent server.
fn tmux_server_absent(stderr: &str) -> bool {
    let text = stderr.to_lowercase();
    text.contains("no server running")
        || (text.contains("error connecting to") && text.contains("no such file or directory"))
}

/// The live pane a route's tmux target names, without process-name heuristics:
/// a `%pane` is itself, an exact `session:window.pane` is that pane, a
/// `session:window` is that window's active pane, and a bare session name is
/// the session's active window's active pane.
fn pane_for_target<'a>(panes: &'a [PaneObs], target: &str) -> Option<&'a PaneObs> {
    if target.starts_with('%') {
        return panes.iter().find(|pane| pane.id == target);
    }
    if target.contains(':') {
        return panes.iter().find(|pane| pane.target == target).or_else(|| {
            panes
                .iter()
                .find(|pane| pane.target.starts_with(&format!("{target}.")) && pane.pane_active)
        });
    }
    panes
        .iter()
        .find(|pane| pane.session == target && pane.window_active && pane.pane_active)
}

/// Every registered harness route with a live pane, one row per route.
fn resolve_route_panes(
    routes: &BTreeMap<String, bus::Route>,
    panes: &[PaneObs],
) -> Vec<ResolvedRoute> {
    let mut out = Vec::new();
    for (name, route) in routes {
        if route.harness.is_none() {
            continue;
        }
        let Some(target) = route.tmux.as_deref().filter(|target| !target.is_empty()) else {
            continue;
        };
        let Some(pane) = pane_for_target(panes, target) else {
            continue;
        };
        out.push(ResolvedRoute {
            route: name.clone(),
            session: pane.session.clone(),
            pane: pane.id.clone(),
            target: pane.target.clone(),
            title: pane.title.clone(),
        });
    }
    out
}

fn live_rows(store: &Store, routes: &BTreeMap<String, bus::Route>) -> Result<Vec<SelectionRow>> {
    let (panes, absent) = pane_snapshot()?;
    if let Some(why) = absent {
        eprintln!("selection list: no tmux server, no live panes ({why})");
    }
    let mut statement = store.connection().prepare_cached(
        "SELECT last_focused_at, selected FROM agent_route_selection WHERE route = ?1",
    )?;
    let mut rows = Vec::new();
    for resolved in resolve_route_panes(routes, &panes) {
        let (last_focused_at, selected) = statement
            .query_row([resolved.route.as_str()], |row| {
                Ok((row.get::<_, Option<i64>>(0)?, row.get::<_, bool>(1)?))
            })
            .optional()?
            .unwrap_or((None, false));
        rows.push(resolved.row(last_focused_at, selected));
    }
    rows.sort_by(|a, b| {
        b.last_focused_at
            .cmp(&a.last_focused_at)
            .then(a.route.cmp(&b.route))
    });
    Ok(rows)
}

/// The selected set `shout --selected` sends to. Errors when the set is empty
/// so an empty selection can never fall back to a broadcast.
pub fn selected_routes(dir: &Path) -> Result<Vec<String>> {
    let store = open(dir)?;
    let mut query = store.connection().prepare(
        "SELECT s.route FROM agent_route_selection s
          JOIN agent_route r ON r.route = s.route
         WHERE s.selected = 1
         ORDER BY s.route",
    )?;
    let routes = query
        .query_map([], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<String>>>()?;
    anyhow::ensure!(!routes.is_empty(), "no Boop recipients selected");
    Ok(routes)
}

pub fn run(cmd: SelectionCmd, mail_dir: Option<std::path::PathBuf>) -> Result<()> {
    let dir = super::mail_dir(mail_dir.as_deref())?;
    let store = open(&dir)?;
    let routes = bus::read_routes(&dir)?;
    match cmd {
        SelectionCmd::List => {
            println!("{}", serde_json::to_string(&live_rows(&store, &routes)?)?);
        }
        SelectionCmd::Set { route, checked } => {
            anyhow::ensure!(routes.contains_key(&route), "unknown Boop route: {route}");
            store.connection().execute(
                "INSERT INTO agent_route_selection(route, selected) VALUES (?1, ?2)
                 ON CONFLICT(route) DO UPDATE SET selected = excluded.selected",
                rusqlite::params![route, checked],
            )?;
        }
        SelectionCmd::Focus { target, at } => {
            let at = at.unwrap_or_else(|| boop::live::now_ms() as i64);
            let (panes, absent) = pane_snapshot()?;
            if let Some(why) = absent {
                anyhow::bail!("selection focus: no tmux server ({why})");
            }
            // A session target names one pane: its active window's active pane.
            // A pane target names itself. Only that pane's route takes the
            // stamp, so a session with several Boop pane routes moves one row.
            let Some(pane) = pane_for_target(&panes, &target) else {
                anyhow::bail!("selection focus: no live tmux pane for {target}");
            };
            let matched = resolve_route_panes(&routes, &panes);
            let mut focused = 0usize;
            for resolved in matched.iter().filter(|resolved| resolved.pane == pane.id) {
                store.connection().execute(
                    "INSERT INTO agent_route_selection(route, last_focused_at)
                     VALUES (?1, ?2)
                     ON CONFLICT(route) DO UPDATE SET
                       last_focused_at = MAX(COALESCE(last_focused_at, 0), excluded.last_focused_at)",
                    rusqlite::params![resolved.route, at],
                )?;
                focused += 1;
            }
            if focused == 0 {
                anyhow::bail!(
                    "selection focus: pane {} ({}) holds no registered Boop route",
                    pane.id,
                    pane.target
                );
            }
        }
        SelectionCmd::Clear => {
            store.connection().execute(
                "UPDATE agent_route_selection SET selected = 0 WHERE selected = 1",
                [],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use boop::harness::HarnessId;

    fn line(
        id: &str,
        session: &str,
        window: u32,
        pane: u32,
        dead: &str,
        wa: &str,
        pa: &str,
        title: &str,
    ) -> String {
        [
            id,
            session,
            &window.to_string(),
            &pane.to_string(),
            dead,
            wa,
            pa,
            title,
        ]
        .join(&FIELD_SEP.to_string())
    }

    fn route(kind: &str, tmux: Option<&str>) -> bus::Route {
        bus::Route {
            kind: kind.into(),
            harness: Some(HarnessId::Claude),
            tmux: tmux.map(str::to_owned),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: None,
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }

    fn routes(rows: &[(&str, &str, Option<&str>)]) -> BTreeMap<String, bus::Route> {
        rows.iter()
            .map(|(name, kind, tmux)| ((*name).to_owned(), route(kind, *tmux)))
            .collect()
    }

    /// RECEIPT. A pane title holding a tab does not shift the dead/active
    /// fields, and a separator inside the title stays in the title.
    #[test]
    fn a_title_with_tabs_and_separators_keeps_its_fields() {
        let sep = FIELD_SEP.to_string();
        let title = format!("task\twith{sep}separator");
        let text = line("%1", "s", 0, 0, "0", "1", "1", &title);
        let panes = parse_panes(&text);
        assert_eq!(panes.len(), 1);
        assert_eq!(panes[0].title, title);
        assert_eq!(panes[0].id, "%1");
        assert!(panes[0].window_active && panes[0].pane_active);
    }

    /// A dead pane is not a recipient.
    #[test]
    fn dead_panes_are_dropped() {
        let text = line("%1", "s", 0, 0, "1", "1", "1", "gone");
        assert!(parse_panes(&text).is_empty());
    }

    /// RECEIPT. A bare session target picks the active window's active pane,
    /// not whichever pane the snapshot lists first. Sabotage: matching the
    /// first pane leaves route-a focused.
    #[test]
    fn a_session_target_picks_the_active_window_pane() {
        let panes = parse_panes(&format!(
            "{}\n{}",
            line("%1", "s", 0, 0, "0", "0", "1", "old window"),
            line("%2", "s", 1, 0, "0", "1", "1", "active window"),
        ));
        let found = pane_for_target(&panes, "s").unwrap();
        assert_eq!(found.id, "%2");
    }

    /// RECEIPT. A `session:window` target picks that window's active pane, not
    /// the first pane the snapshot lists. Sabotage: a bare prefix match returns
    /// the inactive pane.
    #[test]
    fn a_window_target_picks_the_active_pane() {
        let panes = parse_panes(&format!(
            "{}\n{}",
            line("%1", "s", 1, 0, "0", "1", "0", "inactive"),
            line("%2", "s", 1, 1, "0", "1", "1", "active"),
        ));
        assert_eq!(pane_for_target(&panes, "s:1").unwrap().id, "%2");
    }

    /// RECEIPT. A bare session target touches only the route on the active
    /// pane. Sabotage: focusing every pane in the session moves both.
    #[test]
    fn a_session_focus_matches_one_active_pane_route() {
        let panes = parse_panes(&format!(
            "{}\n{}\n{}",
            line("%1", "s", 0, 0, "0", "0", "1", "old"),
            line("%2", "s", 1, 0, "0", "1", "1", "active"),
            line("%3", "other", 0, 0, "0", "1", "1", "elsewhere"),
        ));
        let routes = routes(&[
            ("route-a", "lane", Some("s:0.0")),
            ("route-b", "lane", Some("s:1.0")),
            ("route-c", "lane", Some("%3")),
        ]);
        let resolved = resolve_route_panes(&routes, &panes);
        let active = pane_for_target(&panes, "s").unwrap();
        let focused: Vec<&str> = resolved
            .iter()
            .filter(|resolved| resolved.pane == active.id)
            .map(|resolved| resolved.route.as_str())
            .collect();
        assert_eq!(focused, ["route-b"]);
    }

    /// RECEIPT. A pane target names itself even when another window is active,
    /// and a `session:window.pane` target matches the composed target.
    #[test]
    fn pane_targets_resolve_without_process_heuristics() {
        let panes = parse_panes(&format!(
            "{}\n{}",
            line("%1", "s", 0, 0, "0", "0", "1", "old"),
            line("%2", "s", 1, 0, "0", "1", "1", "active"),
        ));
        assert_eq!(pane_for_target(&panes, "%1").unwrap().id, "%1");
        assert_eq!(pane_for_target(&panes, "s:0.0").unwrap().id, "%1");
        assert_eq!(pane_for_target(&panes, "s:1").unwrap().id, "%2");
    }

    /// RECEIPT. A registered route whose pane is gone is not listed, and a
    /// pane-less route never is; only harness routes on live panes appear.
    #[test]
    fn only_registered_harness_routes_on_live_panes_resolve() {
        let panes = parse_panes(&line("%1", "s", 0, 0, "0", "1", "1", "t"));
        let routes = routes(&[
            ("live", "lane", Some("%1")),
            ("dead", "lane", Some("%9")),
            ("paneless", "coordinator", None),
        ]);
        let resolved = resolve_route_panes(&routes, &panes);
        let names: Vec<&str> = resolved.iter().map(|r| r.route.as_str()).collect();
        assert_eq!(names, ["live"]);
    }
}
