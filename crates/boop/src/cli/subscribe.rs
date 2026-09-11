//! `boop beep agent subscribe` / `unsubscribe`: which subscribers want a
//! lane's commits pushed, and by which mode. A subscription row is keyed
//! `(subscriber, lane)`; `lane` may be `'*'` for every lane the subscriber's
//! parent edge owns.

use std::path::Path;

use anyhow::{Context, Result};

use boop::{bus, ident, identity, lane};

use crate::cli::{line, mail_dir};

/// The `children` target: one row per current child plus a wildcard row.
const CHILDREN_ALIAS: &str = "children";
/// The wildcard lane that covers every lane the subscriber parents.
const WILDCARD: &str = "*";
/// The mode `subscribe` writes when `--mode` is absent.
const DEFAULT_MODE: &str = "door";

/// Resolve `door` / `mailbox`, refusing any other word at the verb boundary.
pub(crate) fn parse_commit_push_mode(text: &str) -> Result<String> {
    match text {
        "door" | "mailbox" => Ok(text.to_owned()),
        other => anyhow::bail!("commit-push mode must be door or mailbox, not `{other}`"),
    }
}

/// Who is subscribing: `--as`, else the caller's own identity. The wildcard
/// row is scoped to the subscriber's parent edges by the ladder.
fn subscriber(as_name: Option<&str>) -> Result<String> {
    let identity = identity::require(as_name);
    identity
        .lane
        .or(identity.session)
        .context("the caller identity carries no lane name")
}

/// The lanes one verb target names: `children` expands to one name per child
/// plus the wildcard, `'*'` is itself, and anything else is one lane.
fn subscription_targets(
    target: &str,
    subscriber: &str,
    routes: &std::collections::BTreeMap<String, bus::Route>,
) -> Vec<String> {
    match target {
        CHILDREN_ALIAS => {
            let mut targets: Vec<String> = lane::children_of(subscriber, routes)
                .into_iter()
                .map(|(name, _)| name.to_owned())
                .collect();
            targets.push(WILDCARD.to_owned());
            targets
        }
        WILDCARD => vec![WILDCARD.to_owned()],
        named => vec![named.to_owned()],
    }
}

/// Write one subscription row per target and print one line per row.
pub(crate) fn run_subscribe(
    target: &str,
    mode: Option<&str>,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let store = bus::open_store(&dir)?;
    let subscriber = subscriber(as_name)?;
    let mode = parse_commit_push_mode(mode.unwrap_or(DEFAULT_MODE))?;
    let routes = bus::read_routes(&dir)?;
    let created_at = bus::now_iso();
    let targets = subscription_targets(target, &subscriber, &routes);
    let mut written = 0usize;
    for lane_name in targets {
        store.set_commit_subscription(&ident::CommitSubscriptionRow {
            subscriber: subscriber.clone(),
            lane: lane_name.clone(),
            mode: mode.clone(),
            created_at: created_at.clone(),
        })?;
        written += 1;
        line(&format!("subscribe {subscriber} {lane_name} {mode}"));
    }
    line(&format!("subscribed {written} row(s)"));
    Ok(())
}

/// Drop one subscription row per target and print one line per row removed.
pub(crate) fn run_unsubscribe(
    target: &str,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let store = bus::open_store(&dir)?;
    let subscriber = subscriber(as_name)?;
    let routes = bus::read_routes(&dir)?;
    let targets = subscription_targets(target, &subscriber, &routes);
    let mut dropped = 0usize;
    for lane_name in targets {
        dropped += store.drop_commit_subscription(&subscriber, &lane_name)?;
        line(&format!("unsubscribe {subscriber} {lane_name}"));
    }
    line(&format!("dropped {dropped} row(s)"));
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use boop::bus::Route;
    use boop::harness::HarnessId;

    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "boop_subscribe_{}_{}_{}",
            std::process::id(),
            name,
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn child(parent: &str) -> Route {
        Route {
            kind: "lane".into(),
            harness: Some(HarnessId::Opencode),
            tmux: Some("lane-x".into()),
            cwd: None,
            model: None,
            mode: None,
            session_id: None,
            source_path: None,
            parent: Some(parent.into()),
            goal: None,
            registered_at: None,
            base_sha: None,
            worktree_dir: None,
            app_server_socket: None,
        }
    }

    /// RECEIPT. A subscribe then unsubscribe against one store leaves no row.
    #[test]
    fn subscribe_then_unsubscribe_leaves_zero_rows() {
        let dir = temp_dir("roundtrip");
        run_subscribe("feature-x", None, Some("obs"), Some(&dir)).unwrap();
        let store = bus::open_store(&dir).unwrap();
        assert_eq!(
            store
                .commit_subscriptions_for_lane("feature-x")
                .unwrap()
                .len(),
            1
        );
        drop(store);
        run_unsubscribe("feature-x", Some("obs"), Some(&dir)).unwrap();
        let store = bus::open_store(&dir).unwrap();
        assert_eq!(
            store
                .commit_subscriptions_for_lane("feature-x")
                .unwrap()
                .len(),
            0
        );
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// RECEIPT. `children` writes one row per current child plus the wildcard.
    #[test]
    fn children_writes_one_row_per_child_plus_a_wildcard() {
        let dir = temp_dir("children");
        bus::write_route(&dir, "c1", &child("obs")).unwrap();
        bus::write_route(&dir, "c2", &child("obs")).unwrap();
        bus::write_route(&dir, "other", &child("someone-else")).unwrap();
        run_subscribe("children", Some("mailbox"), Some("obs"), Some(&dir)).unwrap();
        let store = bus::open_store(&dir).unwrap();
        // One row per child plus one wildcard: c1 and c2 each carry a row, a
        // route that is not a child carries only the wildcard.
        for child in ["c1", "c2"] {
            let rows = store.commit_subscriptions_for_lane(child).unwrap();
            assert_eq!(rows.iter().filter(|row| row.lane == child).count(), 1);
            assert_eq!(rows.iter().filter(|row| row.lane == "*").count(), 1);
        }
        let stranger = store.commit_subscriptions_for_lane("other").unwrap();
        assert_eq!(stranger.iter().filter(|row| row.lane == "other").count(), 0);
        assert_eq!(stranger.iter().filter(|row| row.lane == "*").count(), 1);
        assert_eq!(
            store.commit_subscription("obs", "*").unwrap().as_deref(),
            Some("mailbox")
        );
        drop(store);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
