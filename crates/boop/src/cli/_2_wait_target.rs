//! Which wait a `boop wait <id>` runs: a lane's result wait or a message wait.

use std::collections::BTreeMap;
use std::path::Path;

use boop::bus::{self, Route};
use boop::trail::{self, Spawn};

use crate::cli::mail_dir;

/// `id` names a lane when the registry holds its lane route, or when the lane
/// trail holds its spawn record. A lane that ended (harness died mid-turn,
/// supervisor exited) has had its route dropped by the pane epilogue; its
/// spawn record and result row stay, so the name is still a lane.
pub(crate) fn names_a_lane(
    routes: &BTreeMap<String, Route>,
    spawn: Option<&Spawn>,
    id: &str,
) -> bool {
    match routes.get(id) {
        Some(route) => route.kind == "lane",
        None => spawn.is_some_and(|spawn| bus::route_from_value(&spawn.route).kind == "lane"),
    }
}

/// Whether `boop wait <id>` dispatches to `run_lane_wait`. An unreadable
/// registry falls through to the mail wait.
pub(crate) fn wait_target_is_a_lane(mail_dir_arg: Option<&Path>, id: &str) -> bool {
    let Ok(dir) = mail_dir(mail_dir_arg) else {
        return false;
    };
    let Ok(routes) = bus::read_routes(&dir) else {
        return false;
    };
    names_a_lane(&routes, trail::read_spawn(id).as_ref(), id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spawn(route: serde_json::Value) -> Spawn {
        Spawn {
            tmux: "feature-w".into(),
            socket: None,
            cwd: "/tmp".into(),
            command: "boop beep lane run".into(),
            route,
            spawn_id: Some(1),
            post_pr: false,
            pr_base: None,
        }
    }

    /// RECEIPT (one-wait-verb): a registered lane route is a lane wait; a
    /// message id with neither route nor spawn record is a mail wait.
    #[test]
    fn wait_dispatches_to_lane_wait_only_for_a_registered_lane_route() {
        let dir = crate::cli::testkit::temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        crate::cli::write_route(&dir, "worker", crate::cli::testkit::route_with(None)).unwrap();
        assert!(wait_target_is_a_lane(Some(&dir), "worker"));
        assert!(!wait_target_is_a_lane(Some(&dir), "m-does-not-exist"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// FAIL-PRE-FIX (contract 4_lane_wait "wait on a finished lane mirrors
    /// stub exit 1"): the epilogue dropped the failed lane's route, so the
    /// name read as a message id and the wait exited 124 instead of rc=1.
    #[test]
    fn a_lane_whose_route_was_dropped_is_still_a_lane_by_its_spawn_record() {
        let routes = BTreeMap::new();
        let record = spawn(serde_json::json!({"kind": "lane", "tmux": "feature-w"}));
        assert!(names_a_lane(&routes, Some(&record), "feature-w"));
        assert!(!names_a_lane(&routes, None, "feature-w"));
        let coordinator = spawn(serde_json::json!({"kind": "coordinator"}));
        assert!(!names_a_lane(&routes, Some(&coordinator), "feature-w"));
    }
}
