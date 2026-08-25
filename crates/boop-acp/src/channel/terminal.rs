//! The client half of the ACP `terminal/*` methods: kimi routes every shell
//! command through them, so a lane without them cannot run a shell at all.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use agent_client_protocol::schema::v1::{CreateTerminalRequest, TerminalExitStatus, TerminalId};
use anyhow::{Context, Result};
use tokio::sync::watch;
use tracing::{debug, warn};

/// How much output one terminal keeps when the agent names no limit.
pub const DEFAULT_OUTPUT_BYTE_LIMIT: u64 = 1 << 20;

/// The floor under an agent-named limit; zero would leave the buffer empty.
const MIN_OUTPUT_BYTE_LIMIT: usize = 1024;

/// Polling keeps `kill` free to take the child lock, which a blocking `wait`
/// would hold for the whole run.
const REAP_INTERVAL: Duration = Duration::from_millis(20);

const PUMP_CHUNK: usize = 8192;

/// How long a reaped child's pipes get to drain before the exit status is
/// published anyway; a daemonized grandchild holds them open forever.
const PUMP_DRAIN_GRACE: Duration = Duration::from_millis(500);

/// Output kept for one terminal, oldest bytes dropped first.
#[derive(Debug)]
struct OutputBuffer {
    bytes: Vec<u8>,
    truncated: bool,
    limit: usize,
}

impl OutputBuffer {
    fn new(limit: usize) -> OutputBuffer {
        OutputBuffer {
            bytes: Vec::new(),
            truncated: false,
            limit: limit.max(MIN_OUTPUT_BYTE_LIMIT),
        }
    }

    /// The front is walked past any UTF-8 continuation byte, so what stays
    /// still begins on a character boundary as the spec asks.
    fn push(&mut self, chunk: &[u8]) {
        self.bytes.extend_from_slice(chunk);
        if self.bytes.len() <= self.limit {
            return;
        }
        let mut cut = self.bytes.len() - self.limit;
        while cut < self.bytes.len() && (self.bytes[cut] & 0xC0) == 0x80 {
            cut += 1;
        }
        self.bytes.drain(..cut);
        self.truncated = true;
    }

    fn text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

struct Terminal {
    /// `None` once reaped, so `kill` on a finished terminal is a no-op.
    child: Mutex<Option<Child>>,
    output: Arc<Mutex<OutputBuffer>>,
    exit: watch::Receiver<Option<TerminalExitStatus>>,
}

/// Every terminal one ACP connection has open. Children spawn under the
/// session cwd with the lane env unless the request names its own.
pub struct Terminals {
    cwd: PathBuf,
    next: AtomicU64,
    live: Mutex<HashMap<TerminalId, Arc<Terminal>>>,
}

impl Terminals {
    pub fn new(cwd: PathBuf) -> Terminals {
        Terminals {
            cwd,
            next: AtomicU64::new(1),
            live: Mutex::new(HashMap::new()),
        }
    }

    /// How many terminals are still registered; released ones are gone.
    pub fn open_count(&self) -> usize {
        self.live.lock().expect("terminal registry lock").len()
    }

    /// `terminal/create`: spawn the command and start buffering its output.
    pub fn create(&self, request: &CreateTerminalRequest) -> Result<TerminalId> {
        let cwd = request.cwd.clone().unwrap_or_else(|| self.cwd.clone());
        let limit = request
            .output_byte_limit
            .unwrap_or(DEFAULT_OUTPUT_BYTE_LIMIT)
            .min(usize::MAX as u64) as usize;

        let mut command = Command::new(&request.command);
        command
            .args(&request.args)
            .current_dir(&cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Its own process group, so a kill reaches every child the command
        // forked rather than the shell alone.
        #[cfg(unix)]
        std::os::unix::process::CommandExt::process_group(&mut command, 0);
        for variable in &request.env {
            command.env(&variable.name, &variable.value);
        }
        let mut child = command.spawn().with_context(|| {
            format!(
                "spawn terminal command {} in {}",
                request.command,
                cwd.display()
            )
        })?;

        let output = Arc::new(Mutex::new(OutputBuffer::new(limit)));
        let stdout = child.stdout.take().context("terminal child kept stdout")?;
        let stderr = child.stderr.take().context("terminal child kept stderr")?;
        let pumps = Drain::with(2);
        pump(stdout, Arc::clone(&output), Arc::clone(&pumps));
        pump(stderr, Arc::clone(&output), Arc::clone(&pumps));

        let (exit_tx, exit_rx) = watch::channel(None);
        let terminal = Arc::new(Terminal {
            child: Mutex::new(Some(child)),
            output,
            exit: exit_rx,
        });
        reap(Arc::clone(&terminal), Arc::clone(&pumps), exit_tx);

        let id = TerminalId::new(format!(
            "term_{}",
            self.next.fetch_add(1, Ordering::Relaxed)
        ));
        debug!(
            terminal = %id.0,
            command = %request.command,
            cwd = %cwd.display(),
            output_byte_limit = limit,
            "acp terminal created"
        );
        self.live
            .lock()
            .expect("terminal registry lock")
            .insert(id.clone(), terminal);
        Ok(id)
    }

    /// `terminal/output`: the text so far, whether it was truncated, and the
    /// exit status once the child is done.
    pub fn output(&self, id: &TerminalId) -> Result<(String, bool, Option<TerminalExitStatus>)> {
        let terminal = self.find(id)?;
        let buffer = terminal.output.lock().expect("terminal output lock");
        let status = terminal.exit.borrow().clone();
        Ok((buffer.text(), buffer.truncated, status))
    }

    /// A watch on the exit status, so `terminal/wait_for_exit` awaits off the
    /// dispatch loop.
    pub fn exit_watch(
        &self,
        id: &TerminalId,
    ) -> Result<watch::Receiver<Option<TerminalExitStatus>>> {
        Ok(self.find(id)?.exit.clone())
    }

    /// `terminal/kill`: signal the child and leave the terminal registered,
    /// so the agent can still read its output.
    pub fn kill(&self, id: &TerminalId) -> Result<()> {
        let terminal = self.find(id)?;
        kill_child(&terminal, id);
        Ok(())
    }

    /// `terminal/release`: kill anything still running and forget the id.
    pub fn release(&self, id: &TerminalId) -> Result<()> {
        let terminal = self
            .live
            .lock()
            .expect("terminal registry lock")
            .remove(id)
            .with_context(|| format!("no terminal {}", id.0))?;
        kill_child(&terminal, id);
        debug!(terminal = %id.0, "acp terminal released");
        Ok(())
    }

    fn find(&self, id: &TerminalId) -> Result<Arc<Terminal>> {
        self.live
            .lock()
            .expect("terminal registry lock")
            .get(id)
            .cloned()
            .with_context(|| format!("no terminal {}", id.0))
    }
}

/// Block until the child exits. An already-reaped terminal answers at once.
pub async fn await_exit(
    watch: &mut watch::Receiver<Option<TerminalExitStatus>>,
) -> TerminalExitStatus {
    if let Some(status) = watch.borrow().clone() {
        return status;
    }
    match watch.wait_for(Option::is_some).await {
        Ok(status) => status.clone().unwrap_or_default(),
        Err(_) => TerminalExitStatus::default(),
    }
}

fn kill_child(terminal: &Terminal, id: &TerminalId) {
    let mut guard = terminal.child.lock().expect("terminal child lock");
    let Some(child) = guard.as_mut() else {
        return;
    };
    #[cfg(unix)]
    // SAFETY: the pid is this process's own live child, held under the lock
    // that the reaper takes before it waits, so it cannot have been recycled.
    unsafe {
        libc::killpg(child.id() as i32, libc::SIGKILL);
    }
    if let Err(error) = child.kill() {
        warn!(terminal = %id.0, %error, "acp terminal kill failed");
    }
}

/// Drain one child pipe into the shared buffer until EOF.
fn pump(mut reader: impl Read + Send + 'static, sink: Arc<Mutex<OutputBuffer>>, live: Arc<Drain>) {
    std::thread::spawn(move || {
        let mut chunk = [0u8; PUMP_CHUNK];
        loop {
            match reader.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(read) => sink
                    .lock()
                    .expect("terminal output lock")
                    .push(&chunk[..read]),
            }
        }
        live.finish();
    })
    .thread();
}

/// How many pipes are still being drained, so the reaper can wait for the
/// last byte without joining a thread that may never end.
#[derive(Default)]
struct Drain {
    open: Mutex<usize>,
    done: std::sync::Condvar,
}

impl Drain {
    fn with(pipes: usize) -> Arc<Drain> {
        Arc::new(Drain {
            open: Mutex::new(pipes),
            done: std::sync::Condvar::new(),
        })
    }

    fn finish(&self) {
        let mut open = self.open.lock().expect("terminal drain lock");
        *open = open.saturating_sub(1);
        self.done.notify_all();
    }

    fn wait(&self, grace: Duration) {
        let open = self.open.lock().expect("terminal drain lock");
        let _ = self
            .done
            .wait_timeout_while(open, grace, |open| *open > 0)
            .expect("terminal drain lock");
    }
}

/// Draining both pipes before publishing the status is what makes an
/// `output` call after `wait_for_exit` carry the whole run.
fn reap(
    terminal: Arc<Terminal>,
    pumps: Arc<Drain>,
    exit: watch::Sender<Option<TerminalExitStatus>>,
) {
    std::thread::spawn(move || {
        let status = loop {
            let reaped = {
                let mut guard = terminal.child.lock().expect("terminal child lock");
                match guard.as_mut() {
                    Some(child) => child.try_wait().ok().flatten(),
                    None => break None,
                }
            };
            match reaped {
                Some(status) => {
                    *terminal.child.lock().expect("terminal child lock") = None;
                    break Some(status);
                }
                None => std::thread::sleep(REAP_INTERVAL),
            }
        };
        pumps.wait(PUMP_DRAIN_GRACE);
        let _ = exit.send(Some(status.map(exit_status).unwrap_or_default()));
    });
}

/// The ACP shape of a unix exit: a code, or the signal that killed it.
fn exit_status(status: std::process::ExitStatus) -> TerminalExitStatus {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        TerminalExitStatus::new()
            .exit_code(status.code().map(|code| code as u32))
            .signal(status.signal().map(signal_name))
    }
    #[cfg(not(unix))]
    {
        TerminalExitStatus::new().exit_code(status.code().map(|code| code as u32))
    }
}

/// The signals a killed lane command dies on; anything else takes its number.
#[cfg(unix)]
fn signal_name(signal: i32) -> String {
    match signal {
        1 => "SIGHUP".to_owned(),
        2 => "SIGINT".to_owned(),
        3 => "SIGQUIT".to_owned(),
        6 => "SIGABRT".to_owned(),
        9 => "SIGKILL".to_owned(),
        13 => "SIGPIPE".to_owned(),
        15 => "SIGTERM".to_owned(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{EnvVariable, SessionId};

    fn registry() -> Terminals {
        Terminals::new(std::env::temp_dir())
    }

    fn request(command: &str, args: &[&str]) -> CreateTerminalRequest {
        CreateTerminalRequest::new(SessionId::new("ses_test"), command)
            .args(args.iter().map(|arg| (*arg).to_owned()).collect())
    }

    /// Drive one terminal to its end without an async runtime.
    fn wait_blocking(terminals: &Terminals, id: &TerminalId) -> TerminalExitStatus {
        let watch = terminals.exit_watch(id).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            if let Some(status) = watch.borrow().clone() {
                return status;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "terminal never exited"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn a_created_terminal_runs_under_the_session_cwd() {
        let terminals = registry();
        let id = terminals.create(&request("pwd", &[])).unwrap();
        wait_blocking(&terminals, &id);
        let (text, _, _) = terminals.output(&id).unwrap();
        let expected = std::fs::canonicalize(std::env::temp_dir()).unwrap();
        assert_eq!(
            std::fs::canonicalize(text.trim()).unwrap(),
            expected,
            "{text}"
        );
    }

    #[test]
    fn a_created_terminal_takes_the_request_cwd_and_env() {
        let terminals = registry();
        let created = request("sh", &["-c", "pwd; printf %s \"$BOOP_TERMINAL_PROBE\""])
            .cwd(PathBuf::from("/"))
            .env(vec![EnvVariable::new("BOOP_TERMINAL_PROBE", "wave-b")]);
        let id = terminals.create(&created).unwrap();
        wait_blocking(&terminals, &id);
        let (text, _, _) = terminals.output(&id).unwrap();
        assert!(text.starts_with("/\n"), "{text}");
        assert!(text.ends_with("wave-b"), "{text}");
    }

    /// The lane env is whatever the boop process holds; the child takes it
    /// without the agent naming a single variable.
    #[test]
    fn a_created_terminal_inherits_the_lane_env() {
        std::env::set_var("BOOP_TERMINAL_LANE_ENV", "inherited");
        let terminals = registry();
        let id = terminals
            .create(&request(
                "sh",
                &["-c", "printf %s \"$BOOP_TERMINAL_LANE_ENV\""],
            ))
            .unwrap();
        wait_blocking(&terminals, &id);
        let (text, _, _) = terminals.output(&id).unwrap();
        assert_eq!(text, "inherited");
    }

    #[test]
    fn a_missing_command_fails_create_instead_of_registering_a_terminal() {
        let terminals = registry();
        let error = terminals
            .create(&request("boop-no-such-command", &[]))
            .unwrap_err();
        assert!(
            error.to_string().contains("spawn terminal command"),
            "{error}"
        );
        assert_eq!(terminals.open_count(), 0);
    }

    #[test]
    fn output_carries_both_pipes_and_the_exit_status() {
        let terminals = registry();
        let id = terminals
            .create(&request(
                "sh",
                &["-c", "printf out; printf err 1>&2; exit 3"],
            ))
            .unwrap();
        wait_blocking(&terminals, &id);
        let (text, truncated, status) = terminals.output(&id).unwrap();
        assert!(text.contains("out"), "{text}");
        assert!(text.contains("err"), "{text}");
        assert!(!truncated);
        assert_eq!(status.unwrap().exit_code, Some(3));
    }

    #[test]
    fn output_on_an_unknown_terminal_names_the_id() {
        let terminals = registry();
        let error = terminals.output(&TerminalId::new("term_99")).unwrap_err();
        assert_eq!(error.to_string(), "no terminal term_99");
    }

    #[test]
    fn a_byte_limit_drops_the_oldest_output_and_flags_the_truncation() {
        let terminals = registry();
        let created = request("sh", &["-c", "seq 1 20000"]).output_byte_limit(2048u64);
        let id = terminals.create(&created).unwrap();
        wait_blocking(&terminals, &id);
        let (text, truncated, _) = terminals.output(&id).unwrap();
        assert!(truncated);
        assert!(text.len() <= 2048, "{}", text.len());
        assert!(text.trim_end().ends_with("20000"), "{text}");
        assert!(!text.starts_with("1\n"), "{text}");
    }

    #[test]
    fn wait_for_exit_answers_with_the_childs_code() {
        let terminals = registry();
        let id = terminals
            .create(&request("sh", &["-c", "sleep 0.2; exit 7"]))
            .unwrap();
        let status = wait_blocking(&terminals, &id);
        assert_eq!(status.exit_code, Some(7));
        assert_eq!(status.signal, None);
    }

    #[test]
    fn a_killed_terminal_reports_its_signal_and_stays_readable() {
        let terminals = registry();
        let id = terminals
            .create(&request("sh", &["-c", "printf before; sleep 30"]))
            .unwrap();
        std::thread::sleep(Duration::from_millis(200));
        terminals.kill(&id).unwrap();
        let status = wait_blocking(&terminals, &id);
        assert_eq!(status.signal.as_deref(), Some("SIGKILL"));
        assert_eq!(terminals.open_count(), 1, "kill does not release");
        let (text, _, _) = terminals.output(&id).unwrap();
        assert_eq!(text, "before");
    }

    #[test]
    fn killing_a_finished_terminal_is_a_no_op() {
        let terminals = registry();
        let id = terminals.create(&request("true", &[])).unwrap();
        wait_blocking(&terminals, &id);
        terminals.kill(&id).unwrap();
        assert_eq!(terminals.output(&id).unwrap().2.unwrap().exit_code, Some(0));
    }

    #[test]
    fn release_forgets_the_terminal_and_stops_the_child() {
        let terminals = registry();
        let id = terminals
            .create(&request("sh", &["-c", "sleep 30"]))
            .unwrap();
        assert_eq!(terminals.open_count(), 1);
        terminals.release(&id).unwrap();
        assert_eq!(terminals.open_count(), 0);
        let error = terminals.output(&id).unwrap_err();
        assert_eq!(error.to_string(), format!("no terminal {}", id.0));
        let error = terminals.release(&id).unwrap_err();
        assert_eq!(error.to_string(), format!("no terminal {}", id.0));
    }

    /// Three-byte characters, so a byte-count cut lands mid-character.
    #[test]
    fn a_truncating_buffer_keeps_a_character_boundary() {
        let mut buffer = OutputBuffer::new(MIN_OUTPUT_BYTE_LIMIT);
        buffer.push("\u{4e2d}".repeat(1000).as_bytes());
        assert!(buffer.truncated);
        assert!(std::str::from_utf8(&buffer.bytes).is_ok());
        assert!(buffer.text().ends_with('\u{4e2d}'));
    }
}
