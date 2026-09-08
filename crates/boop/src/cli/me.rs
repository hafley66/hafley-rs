use std::path::Path;

use anyhow::{Context, Result};

use boop::bus::Route;
use boop::harness::HarnessId;
use boop::registry::Registry;
use boop::{bus, identity, tmux};
#[cfg(feature = "agent-read")]
use boop::ident;

#[cfg(feature = "agent-read")]
use crate::cli::db::open_store;
use crate::cli::job::waiting_as;
use crate::cli::mail_dir;
#[cfg(feature = "agent-read")]
use crate::cli::{line, now_ms};

// ---------------------------------------------------------------------------
// Registration, including the legacy lane patch spelling.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub(crate) fn register_route(
    name: &str,
    kind: Option<&str>,
    tmux_target: Option<&str>,
    harness: Option<&str>,
    session_id: Option<&str>,
    cwd: Option<&str>,
    model: Option<&str>,
    mode: Option<&str>,
    parent: Option<&str>,
    goal: Option<&str>,
    mail_dir_arg: Option<&Path>,
    worktree: Option<&Path>,
    registry: &Registry,
    multiplexer: &dyn tmux::Multiplexer,
) -> Result<()> {
    let pane = tmux_target
        .map(|target| {
            anyhow::ensure!(
                multiplexer.target_alive(None, target),
                "no live tmux target {target}"
            );
            multiplexer
                .pane_id(None, target)
                .with_context(|| format!("no live tmux target {target}"))
        })
        .transpose()?;
    let dir = mail_dir(mail_dir_arg)?;
    let harness = harness.map(str::parse::<HarnessId>).transpose()?;
    let patch = bus::route_to_value(&Route {
        kind: kind.unwrap_or(if pane.is_some() { "coordinator" } else { "native" }).into(),
        harness,
        tmux: pane.clone(),
        cwd: cwd.map(str::to_owned),
        model: model.map(str::to_owned),
        mode: mode.map(str::to_owned),
        session_id: session_id.map(str::to_owned),
        source_path: None,
        parent: parent.map(str::to_owned),
        goal: goal.map(str::to_owned),
        registered_at: Some(bus::now_iso()),
        base_sha: None,
        worktree_dir: worktree.map(|path| path.display().to_string()),
        app_server_socket: None,
    });
    // Merge only supplied fields under the store's transaction. Registration
    // and lane patch share this update path, so omitted metadata and a lane's
    // supervisor ownership survive a rebind.
    bus::cas_update_json(&dir.join("registry.json"), |current| {
        let existing = current.get(name);
        let mut fields = existing
            .map(bus::route_from_value)
            .map(|route| bus::route_to_value(&route))
            .unwrap_or_else(|| serde_json::json!({}));
        let fields = fields
            .as_object_mut()
            .expect("route serializes as an object");
        for (key, value) in patch.as_object().expect("route patch is an object") {
            if key != "kind" || kind.is_some() || existing.is_none() {
                fields.insert(key.clone(), value.clone());
            }
        }
        let merged = bus::route_from_value(&serde_json::Value::Object(fields.clone()));
        if session_id.is_none() {
            if let (Some(harness), Some(pane)) = (merged.harness, pane.as_deref()) {
                if let Some(live) = registry.get(harness).live().live_session_in_pane(pane)? {
                    fields.insert("sessionId".into(), serde_json::json!(live.session_id));
                }
            }
        }
        current.insert(name.to_owned(), serde_json::Value::Object(fields.clone()));
        Ok(())
    })?;
    println!("registered {name}");
    Ok(())
}

// ---------------------------------------------------------------------------
// whoami
// ---------------------------------------------------------------------------

#[cfg(feature = "agent-read")]
pub(crate) fn run_me_favorite(index: i64, note: Option<&str>) -> Result<()> {
    anyhow::ensure!(
        index < 0,
        "favorite index must be negative; -1 is the newest assistant message"
    );

    let dir = mail_dir(None)?;
    let routes = bus::read_routes(&dir).unwrap_or_default();
    let identity = identity::resolve(&routes)?;
    let session = identity
        .session
        .context("no caller session resolved: no BOOP_SESSION stamp in this process")?;

    let store = open_store()?;
    let rows = store.turn_rows(&ident::TurnQuery {
        session: Some(session.clone()),
        role: Some("assistant".to_owned()),
        ..Default::default()
    })?;
    let offset = index
        .checked_neg()
        .and_then(|value| value.checked_sub(1))
        .context("favorite index is outside the supported range")? as usize;
    let row = rows.iter().rev().nth(offset).with_context(|| {
        format!(
            "session {session} has {} assistant messages; cannot select {index}",
            rows.len()
        )
    })?;
    anyhow::ensure!(
        !row.said.trim().is_empty(),
        "selected assistant message is empty"
    );
    let source = format!("{}:{}:assistant:{}", row.harness, session, row.turn);
    let id = store.favorite_add(&row.said, note, &source, now_ms())?;
    // The note stays free text on the row; its tags also land in agent_tag,
    // so the CLI path and the instant path feed one table.
    if let Some(note) = note {
        store.tags_apply_note(note, &format!("favorite:{id}"), now_ms() as i64)?;
    }
    line(&format!("favorite {id}"));
    Ok(())
}

/// Read or write the caller's mood. Writing validates the name against the
/// stored moods, so a typo never reaches a delivery path.
pub(crate) fn run_me_mood(
    mood: Option<&str>,
    clear: bool,
    as_name: Option<&str>,
    mail_dir_arg: Option<&Path>,
) -> Result<()> {
    let dir = mail_dir(mail_dir_arg)?;
    let session = waiting_as(&dir, as_name)?;
    let store = boop::Store::open(boop::Store::default_path()?)?;
    match (mood, clear) {
        (Some(mood), _) => {
            store.set_session_mood(&session, mood, boop::channel::now_ms())?;
            println!("mood: {mood} (set on {session})");
        }
        (None, true) => {
            let had = store.clear_session_attr(&session, boop::ident::MOOD_ATTR_KEY)?;
            match had {
                true => println!("mood cleared on {session}"),
                false => println!("{session} had no mood of its own"),
            }
            println!("{}", store.effective_mood(&session)?.line());
        }
        (None, false) => println!("{}", store.effective_mood(&session)?.line()),
    }
    Ok(())
}

/// The caller's own identity, from the two rungs and nothing else. A caller
/// neither rung names exits 2 on one line naming `--as`.
pub(crate) fn run_whoami(
    json: bool,
    as_name: Option<&str>,
    _mail_dir: Option<&Path>,
) -> Result<()> {
    let identity = identity::require(as_name);
    if json {
        println!("{}", identity.to_json());
        return Ok(());
    }
    let rung = identity.rung.unwrap_or(identity::Rung::None);
    println!("session  {}", identity.session.as_deref().unwrap_or("-"));
    println!("lane     {}", identity.lane.as_deref().unwrap_or("-"));
    println!("parent   {}", identity.parent.as_deref().unwrap_or("-"));
    println!("harness  {}", identity.harness.as_deref().unwrap_or("-"));
    println!("pane     {}", identity.pane.as_deref().unwrap_or("-"));
    println!("rung     {} ({})", rung.as_str(), rung.confidence());
    println!(
        "rungs    --as ({}), env BOOP_SESSION ({})",
        mark(as_name.is_some()),
        mark(identity.rung == Some(identity::Rung::Env))
    );
    Ok(())
}

/// Which rung answered, for the two-rung line `whoami` prints.
fn mark(hit: bool) -> &'static str {
    if hit {
        "hit"
    } else {
        "miss"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    use crate::cli::testkit::temp_mail_dir;
    use crate::{Cli, MeCmd, SubCmd};
    use boop::bus::read_routes;
    use boop::harness::{Capabilities, Harness, LanePolicy, MailPolicy, VariantSupport};
    use boop::tmux::{LiveSessions, Multiplexer};

    struct AdoptMux;

    impl Multiplexer for AdoptMux {
        fn current_pane(&self, _: Option<&str>) -> Option<String> {
            None
        }
        fn session_of_pane(&self, _: Option<&str>, _: &str) -> Option<String> {
            None
        }
        fn pane_id(&self, _: Option<&str>, target: &str) -> Option<String> {
            (target == "sprefa-5:0.0").then(|| "%77".to_owned())
        }
        fn pane_pid(&self, _: Option<&str>, _: &str) -> Option<u32> {
            Some(10)
        }
        fn live_sessions(&self, _: Option<&str>) -> Option<LiveSessions> {
            Some(LiveSessions {
                names: ["sprefa-5".into()].into_iter().collect(),
            })
        }
        fn has_session(&self, _: Option<&str>, target: &str) -> anyhow::Result<bool> {
            Ok(target.split(':').next() == Some("sprefa-5"))
        }
        fn kill_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn target_alive(&self, _: Option<&str>, _: &str) -> bool {
            true
        }
        fn capture_pane(&self, _: Option<&str>, _: &str, _: Option<u32>) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn new_detached_session(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: &str,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        fn new_bare_session(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn new_window(
            &self,
            _: Option<&str>,
            _: &str,
            _: &str,
            _: &str,
            _: &str,
        ) -> anyhow::Result<String> {
            Ok(String::new())
        }
        fn swap_windows(&self, _: Option<&str>, _: &str, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
        fn kill_window(&self, _: Option<&str>, _: &str) -> anyhow::Result<()> {
            Ok(())
        }
    }

    /// A claude harness whose live registry holds one session, in one pane.
    struct LiveClaude;

    static CLAUDE_CAPABILITIES: Capabilities = Capabilities {
        bans_plan_family_models: false,
        lanes: LanePolicy::CoordinatorSubagentsOnly,
        variant: VariantSupport::None,
        mail: MailPolicy::Door,
        image_paste_keys: Some("C-v"),
        native_tui_projector: false,
        wrapper_owns_alternate_screen: false,
    };

    struct OnePane;

    impl boop::live::LiveSessions for OnePane {
        fn live_sessions(&self) -> anyhow::Result<Vec<boop::live::LiveSession>> {
            Ok(vec![boop::live::LiveSession {
                harness: HarnessId::Claude,
                session_id: "da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b".into(),
                pid: Some(11),
                cwd: None,
                tmux_pane: Some("%77".into()),
                status: boop::live::LiveStatus::Idle,
                door: boop::live::DoorAddress::None,
                observed_ms: 1,
                started_ms: None,
                scope: boop::live::LiveSessionScope::Unknown,
                parent_session: None,
            }])
        }
    }

    impl Harness for LiveClaude {
        fn id(&self) -> HarnessId {
            HarnessId::Claude
        }

        fn capabilities(&self) -> &'static Capabilities {
            &CLAUDE_CAPABILITIES
        }

        fn live(&self) -> &dyn boop::live::LiveSessions {
            &OnePane
        }

        fn sessions(&self) -> anyhow::Result<Vec<boop::harness::SessionRef>> {
            Ok(Vec::new())
        }

        fn read_from(
            &self,
            _session: &boop::harness::SessionRef,
            offset: u64,
        ) -> anyhow::Result<boop::harness::ReadChunk> {
            Ok(boop::harness::ReadChunk {
                events: Vec::new(),
                next_offset: offset,
                reset: false,
                skipped: 0,
            })
        }
    }

    #[test]
    /// RECEIPT. Adopt names the session the harness's own live registry
    /// reports for the adopted pane, and an explicit `--session-id` still wins.
    fn adopt_reads_the_session_from_the_live_registry_and_explicit_id_wins() {
        let dir = temp_mail_dir();
        std::fs::create_dir_all(&dir).unwrap();
        let mux = AdoptMux;
        let registry = Registry::with(vec![Box::new(LiveClaude)]);
        register_route(
            "sprefa-coordinator",
            Some("coordinator"),
            Some("sprefa-5:0.0"),
            Some("claude"),
            None,
            Some("/repo"),
            None,
            None,
            None,
            None,
            Some(&dir),
            None,
            &registry,
            &mux,
        )
        .unwrap();
        let discovered = read_routes(&dir).unwrap();
        assert_eq!(
            discovered["sprefa-coordinator"].session_id.as_deref(),
            Some("da6da0ca-5ad6-4f2f-88f7-de82e79f1e6b")
        );

        register_route(
            "sprefa-coordinator",
            Some("coordinator"),
            Some("sprefa-5:0.0"),
            Some("claude"),
            Some("explicit-session"),
            Some("/repo"),
            None,
            None,
            None,
            None,
            Some(&dir),
            None,
            &registry,
            &mux,
        )
        .unwrap();
        let explicit = read_routes(&dir).unwrap();
        assert_eq!(
            explicit["sprefa-coordinator"].session_id.as_deref(),
            Some("explicit-session")
        );
        assert_eq!(explicit.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn me_favorite_defaults_to_the_newest_assistant_message() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite"])
            .expect("caller-relative favorite command parses");
        assert!(matches!(
            cli.command,
            Some(SubCmd::Me {
                cmd: MeCmd::Favorite { index: -1, .. },
                ..
            })
        ));
    }

    #[test]
    fn me_favorite_accepts_an_older_negative_position() {
        let cli = Cli::try_parse_from(["boop", "me", "favorite", "-2", "--note", "keep"])
            .expect("negative favorite position parses");
        assert!(matches!(
            cli.command,
            Some(SubCmd::Me {
                cmd: MeCmd::Favorite { index: -2, .. },
                ..
            })
        ));
    }
}
