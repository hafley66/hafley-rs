use super::{LiveSessions, Pane, PaneHit, Session, TerminalSnapshot};
use anyhow::Result;

/// The tmux multiplexer operations boop drives. Object-safe: every method takes
/// `&self` and returns a concrete or `anyhow` type.
pub trait Multiplexer {
    /// The pane selected by the calling tmux client. Codex tool subprocesses
    /// retain `TMUX` but omit `TMUX_PANE`, so this is their identity rung.
    fn current_pane(&self, socket: Option<&str>) -> Option<String>;
    /// The tmux session that owns a pane. `None` when tmux is unreachable or
    /// the pane is unknown.
    fn session_of_pane(&self, socket: Option<&str>, pane: &str) -> Option<String>;
    /// The pane id a target names. `boop adopt` records `session:window.pane`,
    /// which equals neither a pane id nor a session name, so a caller comparing
    /// raw target strings never matches its own pane.
    fn pane_id(&self, socket: Option<&str>, target: &str) -> Option<String>;
    /// The pid of the shell in the first pane of `target`.
    fn pane_pid(&self, socket: Option<&str>, target: &str) -> Option<u32>;
    /// Every pane's shell pid from one server-wide snapshot, keyed so an
    /// existing target string resolves without a tmux spawn: by pane id
    /// (`%N`), by `session:window.pane` target form, and by bare session name
    /// (first pane of the session wins, the same resolution `pane_pid` does).
    /// `None` means tmux itself is unreachable; callers fall back to the
    /// per-target `pane_pid` probe.
    fn pane_pids(&self, _socket: Option<&str>) -> Option<std::collections::BTreeMap<String, u32>> {
        None
    }
    /// One-shot `tmux list-sessions`. `None` means tmux itself is unreachable,
    /// which is NOT the same as "no sessions".
    fn live_sessions(&self, socket: Option<&str>) -> Option<LiveSessions>;
    /// Typed observations for every pane on the selected server. Pane id is
    /// retained so consumers never collapse a multi-pane session to paths[0].
    fn list_panes(&self, _socket: Option<&str>) -> Option<Vec<Pane>> {
        None
    }
    /// Raw formatted pane observations, preserving exit status and stderr.
    /// No target lists all panes; a target limits the read to that session.
    fn list_panes_formatted(
        &self,
        _socket: Option<&str>,
        _target: Option<&str>,
        _format: &str,
    ) -> Result<std::process::Output> {
        anyhow::bail!("multiplexer does not support formatted pane listings")
    }
    /// Attach with inherited terminal streams and return tmux's exit status.
    fn attach_session(
        &self,
        _socket: Option<&str>,
        _target: &str,
    ) -> Result<std::process::ExitStatus> {
        anyhow::bail!("multiplexer does not support attach")
    }
    /// Session metadata joined to typed pane observations from one snapshot.
    fn live_sessions_detailed(&self, _socket: Option<&str>) -> Option<Vec<Session>> {
        None
    }
    /// One-shot exact `has-session` probe.
    fn has_session(&self, socket: Option<&str>, session: &str) -> Result<bool>;
    /// One-shot exact `kill-session`.
    fn kill_session(&self, socket: Option<&str>, session: &str) -> Result<()>;
    /// Whether a route's tmux target is live, judged by a direct per-target
    /// probe: exact `has-session =` for a name, `list-panes` for a pane.
    fn target_alive(&self, socket: Option<&str>, target: &str) -> bool;
    /// Capture a pane's visible region, or the last `lines` rows of history.
    fn capture_pane(
        &self,
        socket: Option<&str>,
        target: &str,
        lines: Option<u32>,
    ) -> Result<String>;
    /// One immutable grid snapshot of the pane `target` names: its cell size,
    /// its visible rows with wrap flags, the screen it is on, the history it
    /// can hand back, and the cursor. This is the geometry-bearing read a
    /// renderer places an overlay against; `capture_pane` is the same text with
    /// none of it. `above` asks for that many history rows above the rows the
    /// reader sees, so a scrolled pane still carries its window plus `above`.
    /// `None` means tmux is unreachable or the target is unknown.
    fn pane_snapshot(
        &self,
        _socket: Option<&str>,
        _target: &str,
        _above: u32,
    ) -> Option<TerminalSnapshot> {
        None
    }
    /// The pane of `session`'s active window under client cell `col`,`row`
    /// (zero-based, status line included). `None`: unreachable, unknown, or a border.
    fn pane_at(
        &self,
        _socket: Option<&str>,
        _session: &str,
        _col: u16,
        _row: u16,
    ) -> Option<PaneHit> {
        None
    }
    /// Spawn a detached tmux session with a shell command.
    fn new_detached_session(
        &self,
        socket: Option<&str>,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<()>;
    /// Spawn a detached tmux session with no command and no cwd, for test
    /// scaffolding; production sessions go through `new_detached_session`.
    fn new_bare_session(&self, socket: Option<&str>, name: &str) -> Result<()>;

    /// Open a window in an existing session and return its target.
    fn new_window(
        &self,
        socket: Option<&str>,
        session: &str,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<String>;

    /// Exchange two windows' positions. Used to hold the interactive window
    /// at index 0 so a bare session attach lands in the agent TUI.
    fn swap_windows(&self, socket: Option<&str>, source: &str, destination: &str) -> Result<()>;

    /// Kill one window, leaving the rest of its session intact.
    fn kill_window(&self, socket: Option<&str>, target: &str) -> Result<()>;

    /// Send named tmux key names such as `Enter`, `C-u` or `Escape`.
    fn send_key_named(&self, _socket: Option<&str>, _pane: &str, _key: &str) -> Result<()> {
        anyhow::bail!("multiplexer does not support named keys")
    }
    /// Send text through tmux's bracketed paste buffer path.
    fn send_text(&self, _socket: Option<&str>, _pane: &str, _text: &str) -> Result<()> {
        anyhow::bail!("multiplexer does not support text paste")
    }
    /// Send literal text without interpreting it as tmux key names.
    fn send_keys_literal(&self, _socket: Option<&str>, _pane: &str, _text: &str) -> Result<()> {
        anyhow::bail!("multiplexer does not support literal keys")
    }
}
