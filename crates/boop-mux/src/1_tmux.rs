use super::*;
use tracing::warn;

/// The one `Multiplexer` implementation: tmux itself, driven by a mix of raw
/// spawns and `tmux_interface` one-shot builders.
pub struct Tmux;

impl Multiplexer for Tmux {
    fn current_pane(&self, socket: Option<&str>) -> Option<String> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["display-message", "-p", "#{pane_id}"]);
        let output = builder.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let pane = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!pane.is_empty()).then_some(pane)
    }

    fn session_of_pane(&self, socket: Option<&str>, pane: &str) -> Option<String> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["display-message", "-p", "-t", pane, "#{session_name}"]);
        let output = builder.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!name.is_empty()).then_some(name)
    }

    fn pane_id(&self, socket: Option<&str>, target: &str) -> Option<String> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args([
            "display-message",
            "-p",
            "-t",
            &exact_pane_target(target),
            "#{pane_id}",
        ]);
        let output = builder.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let pane = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        (!pane.is_empty()).then_some(pane)
    }

    fn pane_pid(&self, socket: Option<&str>, target: &str) -> Option<u32> {
        if !target.contains(':')
            && !target.starts_with('%')
            && !self
                .has_session(socket, target)
                .ok()
                .filter(|alive| *alive)?
        {
            return None;
        }
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        let output = builder
            .args([
                "list-panes",
                "-t",
                &exact_pane_target(target),
                "-F",
                "#{pane_pid}",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .next()
            .and_then(|line| line.trim().parse().ok())
    }

    fn pane_pids(&self, socket: Option<&str>) -> Option<std::collections::BTreeMap<String, u32>> {
        let panes = self.list_panes(socket)?;
        let mut map = std::collections::BTreeMap::new();
        let mut first_pane_sessions = std::collections::BTreeSet::new();
        for pane in panes {
            let Some(pid) = pane.pid else {
                continue;
            };
            map.insert(pane.id, pid);
            map.insert(pane.target, pid);
            if first_pane_sessions.insert(pane.session.clone()) {
                map.insert(pane.session, pid);
            }
        }
        Some(map)
    }

    fn live_sessions(&self, socket: Option<&str>) -> Option<LiveSessions> {
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["list-sessions", "-F", "#{session_name}"]);
        let output = builder.output().ok()?;
        if !output.status.success() {
            return None;
        }
        let mut names = LiveSessions::default();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if !line.trim().is_empty() {
                names.names.insert(line.trim().to_owned());
            }
        }
        Some(names)
    }

    fn list_panes(&self, socket: Option<&str>) -> Option<Vec<Pane>> {
        let mut builder = tmux_command(socket);
        let output = builder
            .args([
                "list-panes",
                "-a",
                "-F",
                "#{session_name}\t#{pane_id}\t#{session_name}:#{window_index}.#{pane_index}\t#{pane_tty}\t#{pane_pid}\t#{pane_current_path}\t#{pane_current_command}",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(parse_panes(&String::from_utf8_lossy(&output.stdout)))
    }

    fn list_panes_formatted(
        &self,
        socket: Option<&str>,
        target: Option<&str>,
        format: &str,
    ) -> Result<std::process::Output> {
        let mut command = tmux_command(socket);
        command.arg("list-panes");
        match target {
            Some(target) => {
                command.args(["-t", target]);
            }
            None => {
                command.arg("-a");
            }
        }
        command
            .args(["-F", format])
            .output()
            .context("run tmux list-panes")
    }

    fn attach_session(
        &self,
        socket: Option<&str>,
        target: &str,
    ) -> Result<std::process::ExitStatus> {
        tmux_command(socket)
            .args(["attach-session", "-t", target])
            .status()
            .context("run tmux attach-session")
    }

    fn live_sessions_detailed(&self, socket: Option<&str>) -> Option<Vec<Session>> {
        let panes = self.list_panes(socket)?;
        let mut panes_by_session = std::collections::BTreeMap::<String, Vec<Pane>>::new();
        for pane in panes {
            panes_by_session
                .entry(pane.session.clone())
                .or_default()
                .push(pane);
        }
        let mut builder = tmux_command(socket);
        let output = builder
            .args([
                "list-sessions",
                "-F",
                "#{session_name}\t#{session_windows}\t#{session_attached}\t#{session_activity}\t#{session_created}",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        Some(parse_sessions(
            &String::from_utf8_lossy(&output.stdout),
            &mut panes_by_session,
        ))
    }

    fn has_session(&self, socket: Option<&str>, session: &str) -> Result<bool> {
        use tmux_interface::{HasSession, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        // `output` and not `status`: a miss makes tmux print `can't find
        // session` on the inherited stderr, and a liveness probe must not
        // write to the caller's terminal.
        let output = builder
            .add_command(HasSession::new().target_session(exact_target(session)))
            .output()
            .context("tmux has-session")?;
        let alive = output.status().success();
        debug!(
            session,
            socket = socket.unwrap_or_default(),
            alive,
            "tmux has-session completed"
        );
        Ok(alive)
    }

    fn kill_session(&self, socket: Option<&str>, session: &str) -> Result<()> {
        debug!(
            session,
            socket = socket.unwrap_or_default(),
            "tmux kill-session starting"
        );
        use tmux_interface::{KillSession, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        builder
            .add_command(KillSession::new().target_session(exact_target(session)))
            .output()
            .context("tmux kill-session")?;
        debug!(session, "tmux kill-session completed");
        Ok(())
    }

    fn target_alive(&self, socket: Option<&str>, target: &str) -> bool {
        if !target.contains(':') && !target.starts_with('%') {
            return self.has_session(socket, target).unwrap_or(false);
        }
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        let target = exact_pane_target(target);
        let output = builder
            .args(["list-panes", "-t", &target, "-F", "#{pane_pid}"])
            .output()
            .ok();
        matches!(output, Some(out) if out.status.success())
    }

    fn capture_pane(
        &self,
        socket: Option<&str>,
        target: &str,
        lines: Option<u32>,
    ) -> Result<String> {
        if !target.contains(':')
            && !target.starts_with(['%', '@', '$'])
            && !self.has_session(socket, target)?
        {
            anyhow::bail!("no such tmux session {target}");
        }
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["capture-pane", "-p", "-t", &exact_pane_target(target)]);
        let start;
        if let Some(lines) = lines {
            start = format!("-{lines}");
            builder.args(["-S", &start]);
        }
        let output = builder.output().context("tmux capture-pane")?;
        if !output.status.success() {
            warn!(
                target,
                socket = socket.unwrap_or_default(),
                "tmux capture-pane failed"
            );
            anyhow::bail!(
                "capture-pane {target}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }

    /// Three one-shot reads, not one: `display-message` carries the facts and
    /// the size, `capture-pane -p` carries the rows, and `capture-pane -pJ`
    /// carries tmux's own wrap flags for the same rows. They race under live
    /// output, which is what `generation` is for — this source has no memory of
    /// its last answer, so it reports `0` and callers compare with `grid_eq`.
    fn pane_snapshot(
        &self,
        socket: Option<&str>,
        target: &str,
        above: u32,
    ) -> Option<TerminalSnapshot> {
        let mut facts_builder = tmux_command(socket);
        let facts_output = facts_builder
            .args([
                "display-message",
                "-p",
                "-t",
                &exact_pane_target(target),
                "#{pane_id}\t#{pane_width}\t#{pane_height}\t#{history_size}\t#{history_limit}\t#{cursor_x}\t#{cursor_y}\t#{alternate_on}\t#{pid}\t#{scroll_position}",
            ])
            .output()
            .ok()?;
        if !facts_output.status.success() {
            return None;
        }
        let facts = parse_snapshot_facts(&String::from_utf8_lossy(&facts_output.stdout))?;
        // History reaches `above` rows over the reader's window, which sits
        // `scroll` rows over the live bottom.
        let depth = above.saturating_add(facts.scroll);
        let start = format!("-{depth}");
        let capture = |extra: Option<&str>| {
            let mut builder = tmux_command(socket);
            builder.args(["capture-pane", "-p", "-t", &exact_pane_target(target)]);
            if depth > 0 {
                builder.args(["-S", &start]);
            }
            if let Some(extra) = extra {
                builder.arg(extra);
            }
            let output = builder.output().ok()?;
            if !output.status.success() {
                return None;
            }
            Some(String::from_utf8_lossy(&output.stdout).into_owned())
        };
        let trimmed = capture(None)?;
        let joined = capture(Some("-J"))?;
        let mut rows = rows_from_capture(&trimmed, &joined, facts.size);
        // The live rows are the capture's tail; a capture short of the history
        // tmux holds plus the pane is short at the bottom, where tmux pads.
        let held = match facts.history {
            History::Retained { rows, .. } => rows.min(depth),
            History::Unavailable => 0,
        };
        while rows.len() < held as usize + facts.size.rows as usize {
            rows.push(TerminalRow {
                viewport_row: rows.len() as u16,
                text: String::new(),
                wraps_previous: false,
            });
        }
        Some(TerminalSnapshot {
            target: TerminalTarget {
                host: "tmux".to_owned(),
                terminal: facts.pane_id,
                incarnation: facts.server_pid,
            },
            generation: 0,
            size: facts.size,
            screen: facts.screen,
            history: facts.history,
            cursor: facts.cursor,
            scroll: facts.scroll,
            rows,
        })
    }

    fn pane_at(&self, socket: Option<&str>, session: &str, col: u16, row: u16) -> Option<PaneHit> {
        let target = exact_pane_target(session);
        let output = tmux_command(socket)
            .args([
                "list-panes",
                "-t",
                &target,
                "-F",
                _1_pane_at::PANE_AT_FORMAT,
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        parse_pane_at(&String::from_utf8_lossy(&output.stdout), col, row)
    }

    fn new_detached_session(
        &self,
        socket: Option<&str>,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<()> {
        debug!(
            session = name,
            cwd,
            socket = socket.unwrap_or_default(),
            "tmux new detached session starting"
        );
        use tmux_interface::{NewSession, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        let new = NewSession::new()
            .detached()
            .start_directory(cwd)
            .session_name(name)
            .shell_command(command);
        let output = builder
            .add_command(new)
            .output()
            .context("tmux new-session")?;
        if !output.success() {
            let rc = output.code().unwrap_or(-1);
            warn!(session = name, rc, "tmux new detached session failed");
            anyhow::bail!(
                "tmux new-session failed rc={rc}: {}",
                String::from_utf8_lossy(&output.stderr()).trim()
            );
        }
        if !self.has_session(socket, name)? {
            warn!(
                session = name,
                "tmux new-session reported success but the session is not live"
            );
            anyhow::bail!(
                "tmux new-session failed: {name} exited 0 but has-session found no such session"
            );
        }
        debug!(session = name, "tmux new detached session completed");
        Ok(())
    }

    fn new_bare_session(&self, socket: Option<&str>, name: &str) -> Result<()> {
        debug!(
            session = name,
            socket = socket.unwrap_or_default(),
            "tmux new bare session starting"
        );
        use tmux_interface::{NewSession, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        let new = NewSession::new().detached().session_name(name);
        let output = builder
            .add_command(new)
            .output()
            .context("tmux new-session")?;
        if !output.success() {
            anyhow::bail!(
                "new-session {name}: {}",
                String::from_utf8_lossy(&output.stderr()).trim()
            );
        }
        debug!(session = name, "tmux new bare session completed");
        Ok(())
    }

    fn new_window(
        &self,
        socket: Option<&str>,
        session: &str,
        name: &str,
        cwd: &str,
        command: &str,
    ) -> Result<String> {
        debug!(
            session,
            window = name,
            cwd,
            socket = socket.unwrap_or_default(),
            "tmux new window starting"
        );
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        // The returned target must survive a window move or an index shift:
        // an index spelling goes stale and tmux clamps it onto a neighbour.
        builder.args([
            "new-window",
            "-d",
            "-P",
            "-F",
            "#{session_name}:#{window_id}",
            "-t",
            session,
            "-n",
            name,
            "-c",
            cwd,
            command,
        ]);
        let output = builder.output().context("tmux new-window")?;
        if !output.status.success() {
            warn!(session, window = name, "tmux new window failed");
            anyhow::bail!(
                "new-window in {session}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let target = String::from_utf8_lossy(&output.stdout).trim().to_owned();
        debug!(session, window = name, target, "tmux new window completed");
        Ok(target)
    }

    fn swap_windows(&self, socket: Option<&str>, source: &str, destination: &str) -> Result<()> {
        debug!(
            source,
            destination,
            socket = socket.unwrap_or_default(),
            "tmux swap windows starting"
        );
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.args(["swap-window", "-s", source, "-t", destination]);
        let output = builder.output().context("tmux swap-window")?;
        if !output.status.success() {
            warn!(source, destination, "tmux swap windows failed");
            anyhow::bail!(
                "swap-windows {source} <-> {destination}: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        debug!(source, destination, "tmux swap windows completed");
        Ok(())
    }

    fn kill_window(&self, socket: Option<&str>, target: &str) -> Result<()> {
        debug!(
            target,
            socket = socket.unwrap_or_default(),
            "tmux kill-window starting"
        );
        use tmux_interface::{KillWindow, Tmux};
        let builder = Tmux::new();
        let builder = match socket {
            Some(socket) => builder.socket_name(socket),
            None => builder,
        };
        let output = builder
            .add_command(KillWindow::new().target_window(target))
            .output()
            .context("tmux kill-window")?;
        if !output.success() {
            anyhow::bail!(
                "kill-window {target}: {}",
                String::from_utf8_lossy(&output.stderr()).trim()
            );
        }
        debug!(target, "tmux kill-window completed");
        Ok(())
    }

    fn send_key_named(&self, socket: Option<&str>, pane: &str, key: &str) -> Result<()> {
        let output = tmux_command(socket)
            .args(["send-keys", "-t", pane, key])
            .output()
            .context("tmux send-keys")?;
        anyhow::ensure!(
            output.status.success(),
            "tmux send-keys into {pane} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }

    fn send_text(&self, socket: Option<&str>, pane: &str, text: &str) -> Result<()> {
        let buffer = format!(
            "boop-{}-{}",
            std::process::id(),
            NEXT_PASTE_BUFFER.fetch_add(1, Ordering::Relaxed)
        );
        let mut child = tmux_command(socket)
            .args(["load-buffer", "-b", &buffer, "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("spawn tmux load-buffer")?;
        child
            .stdin
            .take()
            .context("tmux load-buffer stdin")?
            .write_all(text.as_bytes())
            .context("write tmux paste buffer")?;
        let loaded = child
            .wait_with_output()
            .context("wait for tmux load-buffer")?;
        anyhow::ensure!(
            loaded.status.success(),
            "tmux load-buffer failed: {}",
            String::from_utf8_lossy(&loaded.stderr).trim()
        );

        let pasted = tmux_command(socket)
            .args(["paste-buffer", "-d", "-p", "-b", &buffer, "-t", pane])
            .output()
            .context("tmux paste-buffer")?;
        if !pasted.status.success() {
            let _ = tmux_command(socket)
                .args(["delete-buffer", "-b", &buffer])
                .output();
            anyhow::bail!(
                "tmux paste-buffer into {pane} failed: {}",
                String::from_utf8_lossy(&pasted.stderr).trim()
            );
        }
        Ok(())
    }

    fn send_keys_literal(&self, socket: Option<&str>, pane: &str, text: &str) -> Result<()> {
        let output = tmux_command(socket)
            .args(["send-keys", "-t", pane, "-l", text])
            .output()
            .context("tmux send-keys literal")?;
        anyhow::ensure!(
            output.status.success(),
            "tmux send-keys literal into {pane} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
        Ok(())
    }
}
