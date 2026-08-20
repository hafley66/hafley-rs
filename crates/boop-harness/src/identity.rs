//! Who is calling. Env inference answers "who am I", never "who is the child I
//! am spawning" (agent-bus, two incidents, 2026-08-07).

use std::collections::BTreeMap;

use anyhow::Result;

use boop_store::bus::Route;

/// The rung of the ladder that produced an identity. Ordered most trustworthy
/// first; `Env` is self-reported, so a consumer needing certainty checks this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rung {
    Env,
    ClaudeProcess,
    CodexProcess,
    KimiProcess,
    RouteCwd,
    Pane,
    None,
}

impl Rung {
    pub fn as_str(self) -> &'static str {
        match self {
            Rung::Env => "env",
            Rung::ClaudeProcess => "claude-process",
            Rung::CodexProcess => "codex-process",
            Rung::KimiProcess => "kimi-process",
            Rung::RouteCwd => "route-cwd",
            Rung::Pane => "pane",
            Rung::None => "none",
        }
    }

    /// Whether the rung observed the identity or read it back from a stamp.
    /// `pane` is the route's `session_id` field, written once at adoption; a
    /// session id moves on /clear, on compaction and on resume, so that field
    /// is the weakest answer that is still an answer.
    pub fn confidence(self) -> &'static str {
        match self {
            Rung::Env | Rung::ClaudeProcess | Rung::CodexProcess | Rung::KimiProcess => "exact",
            Rung::RouteCwd => "live-transcript",
            Rung::Pane => "stamped",
            Rung::None => "unresolved",
        }
    }
}

/// The caller's own identity. Every field is optional because an unresolved
/// caller is a real answer.
#[derive(Clone, Debug, Default)]
pub struct Identity {
    pub session: Option<String>,
    pub lane: Option<String>,
    pub parent: Option<String>,
    pub harness: Option<String>,
    pub pane: Option<String>,
    pub rung: Option<Rung>,
}

impl Identity {
    pub fn to_json(&self) -> serde_json::Value {
        let rung = self.rung.unwrap_or(Rung::None);
        serde_json::json!({
            "session": self.session,
            "lane": self.lane,
            "parent": self.parent,
            "harness": self.harness,
            "pane": self.pane,
            "rung": rung.as_str(),
            "confidence": rung.confidence(),
        })
    }
}

/// Resolve the caller. Rungs are tried in order and the first hit wins:
/// stamped env, registered pane, harness process tell, then unresolved.
/// A miss falls through and nothing is guessed.
pub fn resolve(routes: &BTreeMap<String, Route>) -> Result<Identity> {
    let registry = crate::registry::Registry::discover();
    resolve_with(&registry, routes)
}

/// Resolve through the registered harness adapters. The ladder order is
/// global; each rung's implementation belongs to the adapter that owns it.
pub fn resolve_with(
    registry: &crate::registry::Registry,
    routes: &BTreeMap<String, Route>,
) -> Result<Identity> {
    if let Some(harness) = registry.all().first() {
        if let Some(identity) = harness.identity_env() {
            return Ok(identity);
        }
    }
    if let Some(pane) = caller_pane() {
        reject_two_routes_on_one_pane(boop_store::tmux::mux(), routes, &pane)?;
    }
    let pane_identity = registry
        .all()
        .iter()
        .find_map(|harness| harness.identity_pane(routes));
    for harness in registry.all() {
        if let Some(identity) = harness.identity_process() {
            return Ok(named_by_route(identity, pane_identity.as_ref()));
        }
    }
    let Some(mut identity) = pane_identity else {
        return Ok(Identity {
            rung: Some(Rung::None),
            ..Default::default()
        });
    };
    if let Some(route) = identity.lane.as_deref().and_then(|lane| routes.get(lane)) {
        if let Some(session) = live_session_for_route(registry, route)? {
            identity.session = Some(session);
            identity.rung = Some(Rung::RouteCwd);
        }
    }
    Ok(identity)
}

/// The pane the caller stands in: its own `$TMUX_PANE`, else the pane the
/// calling tmux client has selected.
pub(crate) fn caller_pane() -> Option<String> {
    std::env::var("TMUX_PANE")
        .ok()
        .filter(|pane| !pane.is_empty())
        .or_else(|| boop_store::tmux::mux().current_pane(None))
}

/// A harness process names its own session; the registry route standing on the
/// same pane names the lane that session answers to. Neither one is guessed
/// from the other.
fn named_by_route(mut identity: Identity, pane_identity: Option<&Identity>) -> Identity {
    let Some(matched) = pane_identity else {
        return identity;
    };
    identity.lane = matched.lane.clone().or(identity.lane);
    identity.pane = identity.pane.take().or_else(|| matched.pane.clone());
    identity.harness = identity.harness.take().or_else(|| matched.harness.clone());
    identity
}

/// One pane carries one caller. Two routes standing on it name two senders and
/// picking either would put the wrong name on the mail.
fn reject_two_routes_on_one_pane(
    multiplexer: &dyn boop_store::tmux::Multiplexer,
    routes: &BTreeMap<String, Route>,
    pane: &str,
) -> Result<()> {
    let tmux_session = multiplexer.session_of_pane(None, pane);
    let mut hits = routes
        .iter()
        .filter(|(_, route)| route_owns_pane(multiplexer, route, pane, tmux_session.as_deref()))
        .map(|(name, _)| name.as_str());
    if let (Some(first), Some(second)) = (hits.next(), hits.next()) {
        anyhow::bail!(
            "ambiguous caller: pane {pane} is registered as both `{first}` and `{second}`; prune one route"
        );
    }
    Ok(())
}

/// Whether a route's tmux target names this pane. The target is written in
/// whatever form the adopter used: a pane id, a session name, or the
/// `session:window.pane` form `boop adopt` records, which tmux resolves.
pub(crate) fn route_owns_pane(
    multiplexer: &dyn boop_store::tmux::Multiplexer,
    route: &Route,
    pane: &str,
    tmux_session: Option<&str>,
) -> bool {
    let Some(target) = route.tmux.as_deref().filter(|target| !target.is_empty()) else {
        return false;
    };
    if target == pane || Some(target) == tmux_session {
        return true;
    }
    target.contains(':') && multiplexer.pane_id(None, target).as_deref() == Some(pane)
}

/// Rung 4. The route's `session_id` is a stamp; the session it names moves on
/// /clear, on compaction and on resume, and a route written before its session
/// existed carries none at all. The harness's own transcripts for the route's
/// cwd say which session is live, and only one written since the route
/// registered can be this pane's.
fn live_session_for_route(
    registry: &crate::registry::Registry,
    route: &Route,
) -> Result<Option<String>> {
    let (Some(harness_id), Some(cwd)) = (route.harness.as_deref(), route.cwd.as_deref()) else {
        return Ok(None);
    };
    let Some(harness) = registry.by_id(harness_id) else {
        return Ok(None);
    };
    let since = route
        .registered_at
        .as_deref()
        .and_then(boop_store::session::parse_iso_ms);
    let mut live: Vec<crate::harness::SessionRef> = harness
        .root_sessions_for_cwd(cwd)?
        .into_iter()
        .filter(|session| since.is_none_or(|since| session.modified_ms >= since))
        .collect();
    live.sort_by_key(|session| session.modified_ms);
    match live.len() {
        0 => Ok(None),
        1 => Ok(Some(live.remove(0).session_id)),
        _ => {
            let names: Vec<&str> = live
                .iter()
                .map(|session| session.session_id.as_str())
                .collect();
            anyhow::bail!(
                "ambiguous caller: {} {harness_id} sessions wrote to {cwd} since the route registered ({}); name one with `boop adopt --session-id`",
                names.len(),
                names.join(", ")
            )
        }
    }
}

/// Rung 1. The stamp a boop spawn wrote into the child's own environment.
pub(crate) fn from_env_for(_harness: &str) -> Option<Identity> {
    let session = std::env::var("BOOP_SESSION")
        .ok()
        .filter(|s| !s.is_empty())?;
    Some(Identity {
        session: Some(session),
        lane: std::env::var("BOOP_LANE").ok().filter(|s| !s.is_empty()),
        parent: std::env::var("BOOP_PARENT").ok().filter(|s| !s.is_empty()),
        harness: std::env::var("BOOP_HARNESS").ok().filter(|s| !s.is_empty()),
        pane: std::env::var("TMUX_PANE").ok(),
        rung: Some(Rung::Env),
    })
}

/// Rung 2. `$TMUX_PANE` names interactive shells directly. Codex tool
/// subprocesses omit it while retaining `TMUX`, so tmux resolves the calling
/// client's selected pane. The registry may own that pane or its whole session.
pub(crate) fn from_pane_for(harness: &str, routes: &BTreeMap<String, Route>) -> Option<Identity> {
    let pane = caller_pane()?;
    let multiplexer = boop_store::tmux::mux();
    let tmux_session = multiplexer.session_of_pane(None, &pane);
    let (lane, route) = routes.iter().find(|(_, route)| {
        route
            .harness
            .as_deref()
            .is_none_or(|route_harness| route_harness == harness)
            && route_owns_pane(multiplexer, route, &pane, tmux_session.as_deref())
    })?;
    Some(Identity {
        session: route.session_id.clone().or_else(|| Some(lane.clone())),
        lane: Some(lane.clone()),
        parent: None,
        harness: route.harness.clone(),
        pane: Some(pane),
        rung: Some(Rung::Pane),
    })
}

/// The env stamp a spawn writes into its CHILD. Every value describes the
/// child; the spawner appears only as `BOOP_PARENT`.
pub fn child_stamp(session: &str, lane: &str, harness: &str, parent: Option<&str>) -> String {
    let mut stamp = format!(
        "BOOP_SESSION={} BOOP_LANE={} BOOP_HARNESS={}",
        shell_word(session),
        shell_word(lane),
        shell_word(harness)
    );
    if let Some(parent) = parent {
        stamp.push_str(&format!(" BOOP_PARENT={}", shell_word(parent)));
    }
    stamp
}

fn shell_word(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use boop_store::bus::Route;
    use boop_store::testing::FakeMux;

    use super::{child_stamp, reject_two_routes_on_one_pane, resolve, route_owns_pane, Rung};

    fn route_on(target: &str) -> Route {
        Route {
            kind: "coordinator".into(),
            harness: None,
            tmux: Some(target.to_owned()),
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
        }
    }

    /// FAIL-PRE-FIX. `boop adopt` records `session:window.pane`, which equals
    /// neither the pane id nor the session name, so the string compare this
    /// replaced matched nothing and every adopted coordinator read `rung none`.
    #[test]
    fn a_route_adopted_as_session_window_pane_names_its_own_caller_pane() {
        let mux = FakeMux::available(&["turn-visibility"]).with_pane("%2810", "turn-visibility");
        assert!(route_owns_pane(
            &mux,
            &route_on("turn-visibility:0.0"),
            "%2810",
            Some("turn-visibility")
        ));
        assert!(route_owns_pane(&mux, &route_on("%2810"), "%2810", None));
        assert!(route_owns_pane(
            &mux,
            &route_on("turn-visibility"),
            "%2810",
            Some("turn-visibility")
        ));
        assert!(!route_owns_pane(
            &mux,
            &route_on("other:0.0"),
            "%2810",
            Some("turn-visibility")
        ));
    }

    /// One pane carries one caller; picking either of two would put the wrong
    /// name on the mail.
    #[test]
    fn two_routes_standing_on_one_pane_are_a_named_error() {
        let mux = FakeMux::available(&["turn-visibility"]).with_pane("%2810", "turn-visibility");
        let mut routes = BTreeMap::new();
        routes.insert("coord-a".to_owned(), route_on("turn-visibility:0.0"));
        routes.insert("coord-b".to_owned(), route_on("%2810"));
        let error = reject_two_routes_on_one_pane(&mux, &routes, "%2810")
            .expect_err("two routes on one pane must not resolve");
        let text = error.to_string();
        assert!(text.contains("ambiguous caller"), "{text}");
        assert!(
            text.contains("coord-a") && text.contains("coord-b"),
            "{text}"
        );
    }

    /// The claude rung reads the id claude stamps into every process it runs.
    #[test]
    fn the_claude_process_rung_names_the_session_claude_stamped() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", None::<&str>),
                ("TMUX_PANE", None::<&str>),
                ("CLAUDE_CODE_SESSION_ID", Some("555ec3f8")),
                ("CODEX_THREAD_ID", None::<&str>),
                ("KIMI_SESSION_ID", None::<&str>),
            ],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.session.as_deref(), Some("555ec3f8"));
                assert_eq!(identity.harness.as_deref(), Some("claude"));
                assert_eq!(identity.rung, Some(Rung::ClaudeProcess));
                assert_eq!(identity.to_json()["confidence"], "exact");
            },
        );
    }

    /// The bus incident, in one assertion: the stamp describes the CHILD, and
    /// the spawner appears only as the parent.
    #[test]
    fn the_child_stamp_never_carries_the_spawners_session_as_its_own() {
        let stamp = child_stamp("child-1", "lane-a", "opencode", Some("coordinator-9"));
        assert!(stamp.contains("BOOP_SESSION='child-1'"));
        assert!(stamp.contains("BOOP_LANE='lane-a'"));
        assert!(stamp.contains("BOOP_PARENT='coordinator-9'"));
        assert!(
            !stamp.contains("BOOP_SESSION='coordinator-9'"),
            "the spawner's id must never become the child's own: {stamp}"
        );
    }

    #[test]
    fn a_stamp_with_no_parent_omits_the_variable() {
        let stamp = child_stamp("child-1", "lane-a", "claude", None);
        assert!(!stamp.contains("BOOP_PARENT"), "{stamp}");
    }

    /// An unresolved caller is reported, never defaulted to something plausible.
    #[test]
    fn an_unresolved_caller_says_so() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", None::<&str>),
                ("TMUX_PANE", None::<&str>),
                ("CLAUDE_CODE_SESSION_ID", None::<&str>),
                ("CODEX_THREAD_ID", None::<&str>),
                ("KIMI_SESSION_ID", None::<&str>),
            ],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.rung, Some(Rung::None));
                assert!(identity.session.is_none());
                assert_eq!(identity.to_json()["confidence"], "unresolved");
            },
        );
    }

    #[test]
    fn a_fresh_codex_process_identifies_its_spawning_pane_without_a_route() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", None::<&str>),
                ("TMUX_PANE", Some("%1206")),
                ("CLAUDE_CODE_SESSION_ID", None::<&str>),
                ("CODEX_THREAD_ID", Some("thread-7")),
                ("KIMI_SESSION_ID", None::<&str>),
            ],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.session.as_deref(), Some("thread-7"));
                assert_eq!(identity.lane.as_deref(), Some("codex-1206"));
                assert_eq!(identity.harness.as_deref(), Some("codex"));
                assert_eq!(identity.pane.as_deref(), Some("%1206"));
                assert_eq!(identity.rung, Some(Rung::CodexProcess));
            },
        );
    }

    #[test]
    fn kimi_process_rung_uses_the_follow_up_contract() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", None::<&str>),
                ("TMUX_PANE", None::<&str>),
                ("CLAUDE_CODE_SESSION_ID", None::<&str>),
                ("CODEX_THREAD_ID", None::<&str>),
                ("KIMI_SESSION_ID", Some("session-8")),
            ],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.session.as_deref(), Some("session-8"));
                assert_eq!(identity.harness.as_deref(), Some("kimi"));
                assert_eq!(identity.rung, Some(Rung::KimiProcess));
            },
        );
    }
    /// The env rung is self-reported, so it must say so rather than pass as
    /// verified: the bus incident was a self-reported id trusted blindly.
    #[test]
    fn the_env_rung_is_labelled() {
        temp_env::with_vars(
            [
                ("BOOP_SESSION", Some("s-1")),
                ("BOOP_LANE", Some("lane-a")),
                ("BOOP_PARENT", Some("coord")),
            ],
            || {
                let identity = resolve(&BTreeMap::new()).unwrap();
                assert_eq!(identity.rung, Some(Rung::Env));
                assert_eq!(identity.session.as_deref(), Some("s-1"));
                assert_eq!(identity.parent.as_deref(), Some("coord"));
                assert_eq!(identity.to_json()["rung"], "env");
            },
        );
    }

    mod temp_env {
        /// Env is process-global; these tests set and restore it around one
        /// closure and are marked serial by running inside a single mutex.
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

        pub fn with_vars<const N: usize>(vars: [(&str, Option<&str>); N], body: impl FnOnce()) {
            let _guard = LOCK.lock().unwrap_or_else(|error| error.into_inner());
            let saved: Vec<(String, Option<String>)> = vars
                .iter()
                .map(|(key, _)| ((*key).to_owned(), std::env::var(key).ok()))
                .collect();
            for (key, value) in &vars {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
            body();
            for (key, value) in saved {
                match value {
                    Some(value) => std::env::set_var(&key, value),
                    None => std::env::remove_var(&key),
                }
            }
        }
    }
}
