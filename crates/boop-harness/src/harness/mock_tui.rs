//! The mock-TUI launch layer, ported from instant's 2_agentTuiReplay.ts:
//! each adapter runs its real TUI against a loopback llmock provider.

use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};

/// The one canned user turn every recipe opens with.
pub const MOCK_PROMPT: &str = "render the terminal flow";
/// The first line of every canned reply; the driver waits for it.
pub const MOCK_REPLY_MARKER: &str = "FIXED_TERMINAL_REPLY";

/// What one adapter's `mock_tui_launch` writes and spawns against.
pub struct MockTuiContext<'a> {
    /// Scratch HOME the configs land in; created here.
    pub home: &'a Path,
    /// The trusted project dir the TUI runs in.
    pub workspace: &'a Path,
    /// The loopback port the llmock provider serves.
    pub port: u16,
}

/// The launch one adapter hands back: run `executable args` in `workspace`
/// with `env`, and the config files the recipe already wrote.
pub struct MockTuiLaunch {
    pub executable: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub config_paths: Vec<PathBuf>,
    pub replay: MockTuiReplay,
}

/// How the canned prompt starts, because two harnesses differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MockTuiReplay {
    /// The prompt rides argv; the driver only observes.
    PromptArg,
    /// Wait for `readiness` on screen, type the prompt, press Enter.
    TypePrompt { readiness: &'static str },
}

/// instant's `0_terminal-flow.yaml` reply, paced a word every 250 ms so an
/// interrupt lands mid-stream (instant replays the same reply at 0 ms).
pub const DEFAULT_FIXTURE_YAML: &str = r#"rules:
  - match: {}
    respond:
      content: |-
        FIXED_TERMINAL_REPLY
        ```mermaid
        flowchart LR
          PTY --> tmux
          tmux --> xterm
          xterm --> Markdown
        ```
      usage:
        prompt_tokens: 12
        completion_tokens: 18
      stream:
        ttft_ms: 300
        inter_token_ms: 250
        chunk_by: word
"#;

/// The env every recipe carries, ported from instant's
/// `inheritedTerminalEnvironment`: scratch HOME plus the rendering vars.
pub fn terminal_env(home: &Path) -> Vec<(String, String)> {
    let mut env = Vec::new();
    for name in ["PATH", "TMPDIR", "LANG", "LC_ALL", "COLORTERM"] {
        if let Ok(value) = std::env::var(name) {
            if !value.is_empty() {
                env.push((name.to_owned(), value));
            }
        }
    }
    // A scratch npx cache re-extracts codex-acp's 211 MB codex per run, and macOS
    // shows a focus-stealing "Verifying" window for each new copy.
    let npm_cache = std::env::var("npm_config_cache")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .filter(|value| !value.is_empty())
                .map(|real_home| format!("{real_home}/.npm"))
        });
    if let Some(cache) = npm_cache {
        env.push(("npm_config_cache".to_owned(), cache));
    }
    env.push(("HOME".to_owned(), home.display().to_string()));
    env.push(("TERM".to_owned(), "xterm-256color".to_owned()));
    env
}

/// The workspace as named and as resolved, deduped: trust tables key by path
/// spelling, and /tmp vs /private/tmp differs by launcher.
pub fn workspace_spellings(workspace: &Path) -> Result<Vec<String>> {
    let mut spellings = vec![workspace.display().to_string()];
    if let Ok(resolved) = std::fs::canonicalize(workspace) {
        let resolved = resolved.display().to_string();
        if !spellings.contains(&resolved) {
            spellings.push(resolved);
        }
    }
    Ok(spellings)
}

/// `$env_override` wins, else the first executable `name` on PATH.
pub fn resolve_executable(name: &str, env_override: &str) -> Option<PathBuf> {
    if let Ok(path) = std::env::var(env_override) {
        if !path.is_empty() {
            return Some(PathBuf::from(path));
        }
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

/// `LLMOCK_BIN` wins, else the first `llmock` on PATH.
pub fn resolve_llmock() -> Option<PathBuf> {
    resolve_executable("llmock", "LLMOCK_BIN")
}

/// A bound llmock provider. Killed on drop; the port dies with it.
pub struct MockProvider {
    child: Child,
    /// The default fixture this spawn wrote; a caller-named path is not ours.
    fixture: Option<tempfile::TempPath>,
    pub port: u16,
}

impl MockProvider {
    /// Serve `fixtures` (the default cast yaml when `None`) on an ephemeral
    /// loopback port, deterministic and zero-latency, and wait for the bind.
    pub fn spawn(llmock: &Path, fixtures: Option<&Path>) -> Result<Self> {
        let port = unused_loopback_port()?;
        let (fixture, fixture_path) = match fixtures {
            Some(path) => (None, path.display().to_string()),
            None => {
                let owned = write_default_fixture()?;
                let text = owned.display().to_string();
                (Some(owned), text)
            }
        };
        let mut child = Command::new(llmock)
            .arg("--port")
            .arg(port.to_string())
            .arg("--fixtures")
            .arg(fixture_path)
            .arg("--deterministic")
            .arg("--default-ttft-ms")
            .arg("0")
            .arg("--default-inter-token-ms")
            .arg("0")
            .stdin(Stdio::null())
            .spawn()
            .with_context(|| format!("spawn llmock {}", llmock.display()))?;
        wait_for_bind(port, &mut child)?;
        Ok(MockProvider {
            child,
            fixture,
            port,
        })
    }
}

impl Drop for MockProvider {
    fn drop(&mut self) {
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        // The fixture file outlives the server by design: a caller reading
        // llmock's log can still see which rules served.
        drop(self.fixture.take());
    }
}

/// Same dance instant's `unusedLoopbackPort` does: bind :0, read the port,
/// release it, and hand the number to llmock.
fn unused_loopback_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("bind an ephemeral loopback port")?;
    let port = listener
        .local_addr()
        .context("read the ephemeral port")?
        .port();
    drop(listener);
    Ok(port)
}

/// How long the bind wait polls before giving up.
const BIND_TIMEOUT: Duration = Duration::from_secs(10);

fn wait_for_bind(port: u16, child: &mut Child) -> Result<()> {
    let deadline = Instant::now() + BIND_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().context("poll llmock")? {
            anyhow::bail!("llmock exited with {status} before binding :{port}");
        }
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "llmock did not bind 127.0.0.1:{port} within {BIND_TIMEOUT:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn write_default_fixture() -> Result<tempfile::TempPath> {
    let mut file = tempfile::NamedTempFile::new().context("create the default llmock fixture")?;
    file.write_all(DEFAULT_FIXTURE_YAML.as_bytes())
        .context("write the default llmock fixture")?;
    Ok(file.into_temp_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RECEIPT. The default fixture keeps the marker every driver waits on;
    /// sabotage: editing the yaml without moving the marker strands the wait.
    #[test]
    fn default_fixture_answers_with_the_reply_marker() {
        assert!(DEFAULT_FIXTURE_YAML.contains(MOCK_REPLY_MARKER));
        assert!(DEFAULT_FIXTURE_YAML.contains("mermaid"));
        assert!(DEFAULT_FIXTURE_YAML.trim_start().starts_with("rules:"));
    }

    /// RECEIPT. The inherited env carries exactly the terminal vars instant
    /// allows, plus the scratch HOME and a color TERM.
    #[test]
    fn terminal_env_carries_home_and_term() {
        let env = terminal_env(Path::new("/scratch/home"));
        let home = env
            .iter()
            .find(|(name, _)| name == "HOME")
            .map(|(_, value)| value.clone())
            .unwrap();
        assert_eq!(home, "/scratch/home");
        assert!(env.iter().any(|(name, _)| name == "TERM"));
    }

    /// RECEIPT. The override env wins over PATH, as instant spells it.
    #[test]
    fn executable_override_wins() {
        // SAFETY: single-threaded test process, set_var is fine here.
        std::env::set_var("BOOP_TEST_MOCK_BIN", "/opt/other/agent");
        assert_eq!(
            resolve_executable("definitely-not-on-path", "BOOP_TEST_MOCK_BIN"),
            Some(PathBuf::from("/opt/other/agent"))
        );
        std::env::remove_var("BOOP_TEST_MOCK_BIN");
        assert_eq!(
            resolve_executable("definitely-not-on-path", "BOOP_TEST_MOCK_BIN"),
            None
        );
    }
}
