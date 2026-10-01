use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use anyhow::{Context, Result};
use tracing::{debug, warn};

/// A control-mode notification. Unknown `%`-prefixed lines are kept, never
/// dropped; tmux adds notification types across versions.
#[derive(Clone, Debug, PartialEq)]
pub enum Notification {
    Output { pane: String, text: String },
    SessionChanged { id: String, name: String },
    WindowAdd { id: String },
    Exit,
    Unknown(String),
}

/// A line read from the control-mode stream.
#[derive(Clone, Debug, PartialEq)]
pub enum ControlEvent {
    BlockBegin { num: usize },
    BlockEnd { num: usize },
    BlockError { num: usize },
    Body(String),
    Notification(Notification),
}

/// Parse one control-mode line into a control event.
pub fn parse_event(line: &str) -> ControlEvent {
    let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
    if let Some(rest) = trimmed.strip_prefix("%begin") {
        return match num_field(rest) {
            Some(num) => ControlEvent::BlockBegin { num },
            None => ControlEvent::Body(trimmed.to_owned()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("%end") {
        return match num_field(rest) {
            Some(num) => ControlEvent::BlockEnd { num },
            None => ControlEvent::Body(trimmed.to_owned()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("%error") {
        return match num_field(rest) {
            Some(num) => ControlEvent::BlockError { num },
            None => ControlEvent::Body(trimmed.to_owned()),
        };
    }
    if let Some(rest) = trimmed.strip_prefix("%output") {
        let (pane, text) = split_two(rest);
        return ControlEvent::Notification(Notification::Output { pane, text });
    }
    if let Some(rest) = trimmed.strip_prefix("%session-changed") {
        let (id, name) = split_two(rest);
        return ControlEvent::Notification(Notification::SessionChanged { id, name });
    }
    if trimmed.starts_with('%') {
        if trimmed == "%exit" {
            return ControlEvent::Notification(Notification::Exit);
        }
        if let Some(rest) = trimmed.strip_prefix("%window-add") {
            return ControlEvent::Notification(Notification::WindowAdd {
                id: rest.trim().to_owned(),
            });
        }
        return ControlEvent::Notification(Notification::Unknown(trimmed.to_owned()));
    }
    ControlEvent::Body(trimmed.to_owned())
}

/// The `%begin/%end/%error` line's command number, its second field (the first
/// is a timestamp).
fn num_field(rest: &str) -> Option<usize> {
    let mut fields = rest.split_whitespace();
    let _time = fields.next()?;
    fields.next()?.parse().ok()
}

fn split_two(rest: &str) -> (String, String) {
    let mut parts = rest.splitn(2, ' ');
    let first = parts.next().unwrap_or_default();
    let second = parts.next().unwrap_or_default();
    (first.trim().to_owned(), second.trim_start().to_owned())
}

/// A long-lived `tmux -C` child, kept across questions instead of forking a
/// tmux process per command the way `boop bus` does.
pub struct ControlClient {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// tmux emits an empty attach block on connect before any command; the
    /// first command must skip it.
    first_block: bool,
}

impl ControlClient {
    /// Spawn `tmux [-L socket] -C`. `-C` (not `-CC`) keeps the terminal in
    /// canonical mode with echo on; `boop` is not a terminal emulator and must
    /// not change terminal attributes.
    pub fn spawn(socket: Option<&str>) -> Result<Self> {
        debug!(
            socket = socket.unwrap_or_default(),
            "tmux control client starting"
        );
        let mut builder = Command::new("tmux");
        if let Some(socket) = socket {
            builder.arg("-L").arg(socket);
        }
        builder.arg("-C");
        let mut child = builder
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("spawn tmux -C; is tmux installed and reachable?")?;
        let stdin = child.stdin.take().context("tmux control stdin")?;
        let stdout = child.stdout.take().context("tmux control stdout")?;
        debug!(
            socket = socket.unwrap_or_default(),
            "tmux control client started"
        );
        Ok(ControlClient {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            first_block: true,
        })
    }

    /// Read one parsed event from the control stream. EOF and timeouts error.
    pub fn next_event(&mut self) -> Result<ControlEvent> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let line = next_line(&mut self.stdout, deadline)?;
        Ok(parse_event(&line))
    }

    /// Send one command, block for its `%begin`/`%end` (or `%error`) block,
    /// return the body. A `%error` block is a returned `Err`, never a panic
    /// and never a silent empty result. Blocks are paired by the command
    /// number tmux assigns (which is server-global and not predictable), and
    /// the first pair on a fresh connection is tmux's own attach block.
    pub fn command(&mut self, argv: &[&str]) -> Result<Vec<String>> {
        debug!(argc = argv.len(), "tmux control command starting");
        let line = argv
            .iter()
            .map(|arg| quote_arg(arg))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(self.stdin, "{line}").context("write tmux control command")?;
        self.stdin.flush().context("flush tmux control stdin")?;

        let mut open: Option<usize> = None;
        let mut body: Vec<String> = Vec::new();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let line = next_line(&mut self.stdout, deadline)?;
            match parse_event(&line) {
                ControlEvent::BlockBegin { num } => {
                    open = Some(num);
                    body.clear();
                }
                ControlEvent::BlockEnd { num } if Some(num) == open => {
                    if self.first_block {
                        self.first_block = false;
                        open = None;
                        continue;
                    }
                    debug!(
                        argc = argv.len(),
                        output_lines = body.len(),
                        "tmux control command completed"
                    );
                    return Ok(body);
                }
                ControlEvent::BlockError { num } if Some(num) == open => {
                    if self.first_block {
                        self.first_block = false;
                        open = None;
                        continue;
                    }
                    warn!(argc = argv.len(), "tmux control command failed");
                    anyhow::bail!("tmux command failed: {}", argv.join(" "));
                }
                ControlEvent::BlockEnd { .. } | ControlEvent::BlockError { .. } => {}
                ControlEvent::Body(text) => {
                    if open.is_some() {
                        body.push(text);
                    }
                }
                ControlEvent::Notification(_) => {}
            }
        }
    }
}

/// Quote an argv item for tmux's command-line parser when it contains a
/// character that would otherwise be parsed specially (a leading `#` begins a
/// comment, so format strings must be quoted).
fn quote_arg(arg: &str) -> String {
    if arg.contains([' ', '\t', '#', '"', '\'', '{', '}']) {
        format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\""))
    } else {
        arg.to_owned()
    }
}

/// Read the next line from the control stream. `ChildStdout` has no timeout,
/// so a read that reaches the deadline errors instead of hanging forever.
fn next_line(reader: &mut BufReader<ChildStdout>, deadline: std::time::Instant) -> Result<String> {
    let mut buffer = Vec::new();
    loop {
        if std::time::Instant::now() > deadline {
            anyhow::bail!("tmux control mode timed out");
        }
        let read = reader.fill_buf().context("read tmux control stdout")?;
        if read.is_empty() {
            anyhow::bail!("tmux control process closed its stdout");
        }
        match read.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                buffer.extend_from_slice(&read[..index]);
                reader.consume(index + 1);
                return Ok(String::from_utf8_lossy(&buffer).into_owned());
            }
            None => {
                buffer.extend_from_slice(read);
                let len = read.len();
                reader.consume(len);
            }
        }
    }
}

/// Rust does not reap a child on drop, so without this every dropped client
/// leaves a live `tmux -C` process holding its server open.
impl Drop for ControlClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_event_reads_notifications_blocks_and_reports_eof() {
        let mut child = Command::new("sh")
            .args([
                "-c",
                "printf '%s\\n' '%window-add @7' '%begin 100 42 0' 'body' '%end 100 42 0'",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let mut client = ControlClient {
            stdin: child.stdin.take().unwrap(),
            stdout: BufReader::new(child.stdout.take().unwrap()),
            child,
            first_block: true,
        };
        let events: Vec<_> = (0..4).map(|_| client.next_event().unwrap()).collect();
        assert_eq!(
            events,
            vec![
                ControlEvent::Notification(Notification::WindowAdd { id: "@7".into() }),
                ControlEvent::BlockBegin { num: 42 },
                ControlEvent::Body("body".into()),
                ControlEvent::BlockEnd { num: 42 },
            ]
        );
        assert_eq!(
            client.next_event().unwrap_err().to_string(),
            "tmux control process closed its stdout"
        );
    }
}
