//! omp (oh-my-pi) registration and lane-channel receipts, through the real
//! binary. Every leg that needs `omp` on PATH returns early when it is absent;
//! the mock-provider leg is the only seam, and it is a real llmock process.

use std::path::PathBuf;
use std::sync::Mutex;

use boop_acp::channel::ChannelSpec;
use boop_harness::harness::mock_tui::{
    resolve_llmock, MockProvider, MOCK_PROMPT, MOCK_REPLY_MARKER,
};
use boop_harness::harness::omp::Omp;
use boop_harness::{Harness, HarnessId, OneShotSpec, Registry};

/// Tests that rewrite `HOME` serialize here; the child harness reads it.
static ENV_LOCK: Mutex<()> = Mutex::new(());

/// The `omp` executable this machine would spawn, or `None` when it is absent.
fn omp_binary() -> Option<PathBuf> {
    boop_harness::harness::mock_tui::resolve_executable("omp", "OMP_BIN")
}

/// Point omp's config root at `home` by rewriting `HOME` for the spawned child.
fn with_home<T>(home: &std::path::Path, run: impl FnOnce() -> T) -> T {
    let saved = std::env::var_os("HOME");
    std::env::set_var("HOME", home);
    let outcome = run();
    match saved {
        Some(value) => std::env::set_var("HOME", value),
        None => std::env::remove_var("HOME"),
    }
    outcome
}

/// Set `name` for the spawned child and restore it afterward.
fn with_env<T>(name: &str, value: Option<&str>, run: impl FnOnce() -> T) -> T {
    let saved = std::env::var_os(name);
    match value {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    let outcome = run();
    match saved {
        Some(value) => std::env::set_var(name, value),
        None => std::env::remove_var(name),
    }
    outcome
}

/// The working OpenRouter key, read from the caller's own opencode config. The
/// env `OPENROUTER_API_KEY` is expired, so a live omp leg names this key in
/// the child's env instead; it is never written to any file.
fn openrouter_key() -> Option<String> {
    let home = dirs::home_dir().unwrap_or_default();
    let path = home.join(".config").join("opencode").join("opencode.json");
    let text = std::fs::read_to_string(&path).ok()?;
    let root = serde_json::from_str::<serde_json::Value>(&text).ok()?;
    root.get("provider")
        .and_then(|provider| provider.get("openrouter"))
        .and_then(|openrouter| openrouter.get("options"))
        .and_then(|options| options.get("apiKey"))
        .and_then(serde_json::Value::as_str)
        .filter(|key| !key.is_empty())
        .map(str::to_owned)
}

#[test]
fn omp_is_registered_and_the_registry_is_complete() {
    let registry = Registry::discover();
    assert_eq!(registry.get(HarnessId::Omp).id(), HarnessId::Omp);
    assert_eq!(registry.all().len(), HarnessId::ALL.len());
}

#[test]
fn omp_short_id_round_trips() {
    assert_eq!(HarnessId::parse("omp"), Some(HarnessId::Omp));
    assert_eq!(
        "omp".parse::<HarnessId>().unwrap().as_str(),
        HarnessId::Omp.as_str()
    );
}

/// The `--print` one-shot runs the real omp against a real llmock provider.
/// Skipped when omp or llmock is not installed.
#[test]
fn one_shot_runs_omp_against_a_mock_provider() {
    let Some(_omp) = omp_binary() else {
        return;
    };
    let Some(llmock) = resolve_llmock() else {
        return;
    };
    let provider = MockProvider::spawn(&llmock, None).expect("start llmock");
    let home = tempfile::tempdir().expect("temp home");
    let agent_dir = home.path().join(".omp").join("agent");
    std::fs::create_dir_all(&agent_dir).expect("create omp agent dir");
    std::fs::write(
        agent_dir.join("models.yml"),
        format!(
            "providers:\n  llmock:\n    baseUrl: http://127.0.0.1:{}/openai/v1\n    apiKey: test\n    api: openai-completions\n    models:\n      - id: mock-model\n        name: Mock Model\n",
            provider.port
        ),
    )
    .expect("write models.yml");

    let Some(key) = openrouter_key() else {
        return;
    };
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let reply = with_home(home.path(), || {
        with_env("OPENROUTER_API_KEY", Some(key.as_str()), || {
            Omp.one_shot(&OneShotSpec {
                model: Some("llmock/mock-model".to_owned()),
                prompt: MOCK_PROMPT.to_owned(),
            })
        })
    })
    .expect("omp --print returns a reply");
    assert!(
        reply.contains(MOCK_REPLY_MARKER),
        "mock reply missing its marker: {reply:?}"
    );
}

/// The ACP handshake opens a real session: `conversation_id()` is some. Skipped
/// when omp is not installed.
#[test]
fn open_channel_reaches_a_real_omp_acp_session() {
    if omp_binary().is_none() {
        return;
    }
    let home = tempfile::tempdir().expect("temp home");
    let Some(key) = openrouter_key() else {
        return;
    };
    let _guard = ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let session = with_home(home.path(), || {
        with_env("OPENROUTER_API_KEY", Some(key.as_str()), || {
            let spec = ChannelSpec {
                model: None,
                effort: None,
                cwd: std::env::temp_dir(),
                resume: None,
                lane: None,
                executable: None,
            };
            let mut channel = Omp
                .open_channel(&spec)
                .expect("omp acp handshake fails with its own text");
            let session = channel.conversation_id();
            channel.close().expect("close the omp acp channel");
            session
        })
    });
    assert!(session.is_some(), "omp acp handshake reached no session");
}
