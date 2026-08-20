//! The resident ACP session host: its socket, its fan-out, and the mail row
//! that becomes a `session/prompt`.
//!
//! FAIL-PRE-FIX: no boop process outlived a CLI invocation, so a mail row
//! addressed to a coordinator could only ever be typed at a pane. `grep -rn
//! "UnixListener" crates/*/src` returned nothing before this arc.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// Every wait in this file. Nothing here is a model call, so a leg that has
/// not landed inside it is a defect and never a slow machine.
const CAP: Duration = Duration::from_secs(10);

/// `sun_path` is 104 bytes, and `std::env::temp_dir()` is a 49-byte
/// `/var/folders/...` path on Darwin, which leaves no room for a route.
fn root(name: &str) -> PathBuf {
    let dir = PathBuf::from("/tmp").join(format!("bacph-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for leaf in ["mail", "repo", "bin", "home"] {
        std::fs::create_dir_all(dir.join(leaf)).unwrap();
    }
    dir
}

fn write_executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A stub ACP agent, shadowing the `npx` that `CODEX_ADAPTER` spawns. It logs
/// every frame it reads and answers the five methods the host sends. The SDK
/// mints string rpc ids, so the id is echoed as JSON rather than reformatted.
fn write_stub(bin: &Path) {
    write_executable(
        &bin.join("npx"),
        r#"#!/usr/bin/env python3
import json, os, sys

with open(os.environ["BOOP_TEST_ACP_PIDS"], "a") as pids:
    pids.write("spawn %d\n" % os.getpid())

REPLIES = {
    "initialize": {
        "protocolVersion": 1,
        "authMethods": [],
        "agentInfo": {"name": "stub", "version": "0.0.1"},
        "agentCapabilities": {
            "loadSession": True,
            "_meta": {"claudeCode": {"promptQueueing": True}},
        },
    },
    "session/new": {"sessionId": "stub-session-1"},
    "session/load": {},
    "session/set_config_option": {"configOptions": []},
    "session/prompt": {"stopReason": "end_turn"},
}

log = open(os.environ["BOOP_TEST_ACP_LOG"], "a")
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    log.write(line + "\n")
    log.flush()
    frame = json.loads(line)
    method = frame.get("method")
    if method not in REPLIES or "id" not in frame:
        continue
    sys.stdout.write(
        json.dumps({"jsonrpc": "2.0", "id": frame["id"], "result": REPLIES[method]}) + "\n"
    )
    sys.stdout.flush()
"#,
    );
}

struct Fixture {
    root: PathBuf,
    mail: PathBuf,
    repo: PathBuf,
    bin: PathBuf,
    log: PathBuf,
    pids: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let root = root(name);
        let fixture = Fixture {
            mail: root.join("mail"),
            repo: root.join("repo"),
            bin: root.join("bin"),
            log: root.join("acp-rpc.ndjson"),
            pids: root.join("acp-pids.txt"),
            root,
        };
        write_stub(&fixture.bin);
        std::fs::write(
            fixture.mail.join("registry.json"),
            r#"{"coord":{"kind":"coordinator","harness":"codex","mode":"interactive"}}"#,
        )
        .unwrap();
        fixture
    }

    fn command(&self, args: &[&str]) -> Command {
        let mut command = Command::new(BOOP);
        command
            .args(args)
            .arg("--mail-dir")
            .arg(&self.mail)
            .env("PATH", format!("{}:/usr/bin:/bin", self.bin.display()))
            .env("HOME", self.root.join("home"))
            .env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_TEST_ACP_LOG", &self.log)
            .env("BOOP_TEST_ACP_PIDS", &self.pids)
            .current_dir(&self.repo);
        command
    }

    fn boop(&self, args: &[&str]) -> std::process::Output {
        self.command(args).output().unwrap()
    }

    /// Start a host and wait for its socket to answer.
    fn host(&self, route: &str) -> Host {
        let child = self
            .command(&["acp", "host", route, "--cwd", &self.repo.display().to_string()])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let host = Host(child);
        let socket = self.socket(route);
        wait_for("the host socket to answer", || {
            std::os::unix::net::UnixStream::connect(&socket).is_ok()
        });
        host
    }

    fn socket(&self, route: &str) -> PathBuf {
        self.root.join("acp").join(format!("{route}.sock"))
    }

    fn rpc_log(&self) -> Vec<serde_json::Value> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    fn frames(&self, method: &str) -> Vec<serde_json::Value> {
        self.rpc_log()
            .into_iter()
            .filter(|frame| frame["method"] == method)
            .collect()
    }

    fn session_id(&self, route: &str) -> String {
        let raw = std::fs::read_to_string(self.mail.join("registry.json")).unwrap();
        let map: serde_json::Value = serde_json::from_str(&raw).unwrap();
        map[route]["sessionId"].as_str().unwrap_or_default().to_owned()
    }

    fn spawned_children(&self) -> usize {
        std::fs::read_to_string(&self.pids)
            .unwrap_or_default()
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
    }
}

/// A running host. Killed on drop so no test leaves a resident process or a
/// bound socket behind.
struct Host(Child);

impl Drop for Host {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// Poll `ready` until it holds or the cap runs out. Every wait in this file is
/// bounded; nothing is waited out.
fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + CAP;
    while Instant::now() < deadline {
        if ready() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("waited {CAP:?} for {what}");
}

/// Speak ACP to a host through the attach shim, exactly as an editor would:
/// spawn `boop acp attach`, write frames to its stdin, read frames off its
/// stdout. Bounded by the same cap as every other wait in this file.
fn through_the_shim(
    fixture: &Fixture,
    route: &str,
    frames: &[serde_json::Value],
) -> Vec<serde_json::Value> {
    let mut shim = fixture
        .command(&["acp", "attach", route])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = shim.stdin.take().unwrap();
    let stdout = BufReader::new(shim.stdout.take().unwrap());
    let wanted = frames.len();
    let (done, answers) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut collected: Vec<serde_json::Value> = Vec::new();
        for line in stdout.lines().map_while(Result::ok) {
            let Ok(frame) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            if frame.get("id").is_some() && frame.get("method").is_none() {
                collected.push(frame);
            }
            if collected.len() == wanted {
                break;
            }
        }
        let _ = done.send(collected);
    });
    for frame in frames {
        writeln!(stdin, "{frame}").unwrap();
        stdin.flush().unwrap();
    }
    let collected = answers.recv_timeout(CAP).unwrap_or_default();
    drop(stdin);
    let _ = shim.kill();
    let _ = shim.wait();
    collected
}

/// Hold a shim open and stream every frame the host sends it. The client is
/// initialized so the host treats it as attached.
fn attached(
    fixture: &Fixture,
    route: &str,
) -> (Child, std::sync::mpsc::Receiver<serde_json::Value>) {
    let mut shim = fixture
        .command(&["acp", "attach", route])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = shim.stdin.take().unwrap();
    let stdout = BufReader::new(shim.stdout.take().unwrap());
    let (frames, stream) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in stdout.lines().map_while(Result::ok) {
            if let Ok(frame) = serde_json::from_str::<serde_json::Value>(&line) {
                if frames.send(frame).is_err() {
                    return;
                }
            }
        }
    });
    writeln!(
        stdin,
        r#"{{"jsonrpc":"2.0","id":1,"method":"initialize","params":{{"protocolVersion":1,"clientCapabilities":{{}}}}}}"#
    )
    .unwrap();
    stdin.flush().unwrap();
    std::mem::forget(stdin);
    (shim, stream)
}

/// RECEIPT. An unmodified ACP client spawns the shim as if it were an agent
/// binary and gets a real turn out of the session boop already owns.
#[test]
fn a_host_and_an_attached_shim_exchange_a_prompt() {
    let fixture = Fixture::new("shim-prompt");
    let _host = fixture.host("coord");

    let answers = through_the_shim(
        &fixture,
        "coord",
        &[
            serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":1,"clientCapabilities":{}}}),
            serde_json::json!({"jsonrpc":"2.0","id":2,"method":"session/new","params":{"cwd":fixture.repo.display().to_string(),"mcpServers":[]}}),
            serde_json::json!({"jsonrpc":"2.0","id":3,"method":"session/prompt","params":{"sessionId":"stub-session-1","prompt":[{"type":"text","text":"HELLO_FROM_THE_SHIM"}]}}),
        ],
    );

    assert_eq!(answers.len(), 3, "{answers:#?}");
    // The host answers `initialize` with the upstream's own capabilities.
    assert_eq!(answers[0]["result"]["agentCapabilities"]["loadSession"], true);
    // The client believes it opened a session; it got the one already open.
    assert_eq!(answers[1]["result"]["sessionId"], "stub-session-1");
    assert_eq!(answers[2]["result"]["stopReason"], "end_turn");

    let prompts = fixture.frames("session/prompt");
    assert_eq!(prompts.len(), 1, "{prompts:#?}");
    assert_eq!(prompts[0]["params"]["prompt"][0]["text"], "HELLO_FROM_THE_SHIM");
}

/// RECEIPT. The bind is the uniqueness proof, and it is taken before any
/// child is spawned, so a losing host never leaves an orphan adapter.
#[test]
fn a_second_host_on_one_route_loses_the_bind_before_it_forks() {
    let fixture = Fixture::new("one-host");
    let _host = fixture.host("coord");
    // The socket is bound before the adapter is spawned, so a live socket is
    // not yet evidence of a child.
    wait_for("the first host's adapter to spawn", || {
        fixture.spawned_children() == 1
    });

    let second = fixture.boop(&["acp", "host", "coord", "--cwd", &fixture.repo.display().to_string()]);

    assert!(!second.status.success(), "the second host bound the route");
    let stderr = String::from_utf8_lossy(&second.stderr);
    assert!(stderr.contains("already has a live acp host"), "{stderr}");
    assert_eq!(
        fixture.spawned_children(),
        1,
        "the losing host spawned an adapter child"
    );
}

/// RECEIPT. A mail row addressed to a hosted route reaches the session as a
/// `session/prompt` carrying a text block, not as a keystroke into a pane.
#[test]
fn a_mail_row_reaches_a_live_host_as_a_prompt() {
    let fixture = Fixture::new("mail-prompt");
    let _host = fixture.host("coord");
    let (mut client, updates) = attached(&fixture, "coord");

    let hail = fixture.boop(&[
        "hail",
        "--to",
        "coord",
        "--body",
        "MAIL_BODY_SENTINEL",
        "--from",
        "tester",
    ]);
    assert!(hail.status.success(), "{}", String::from_utf8_lossy(&hail.stderr));
    let stdout = String::from_utf8_lossy(&hail.stdout);
    assert!(stdout.contains("(acp host delivers it)"), "{stdout}");

    wait_for("the mail row to become a prompt", || {
        !fixture.frames("session/prompt").is_empty()
    });
    let prompts = fixture.frames("session/prompt");
    let block = &prompts[0]["params"]["prompt"][0];
    assert_eq!(block["type"], "text", "{block:#?}");
    let text = block["text"].as_str().unwrap();
    assert!(text.contains("MAIL_BODY_SENTINEL"), "{text}");
    assert!(text.contains("from tester"), "{text}");
    assert_eq!(prompts[0]["params"]["sessionId"], "stub-session-1");

    // The row is stamped delivered once the prompt is accepted, so the next
    // host never replays it.
    wait_for("the row to be acked", || {
        std::fs::read_to_string(fixture.mail.join("bus.ndjson"))
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
            .any(|row| row["to"] == "coord" && !row["to_timestamp"].is_null())
    });

    // The attached human sees the turn boop spoke, as user content, which is
    // the frame ACP already defines for it.
    let deadline = Instant::now() + CAP;
    let mut mirrored = None;
    while Instant::now() < deadline && mirrored.is_none() {
        let Ok(frame) = updates.recv_timeout(Duration::from_millis(250)) else {
            continue;
        };
        if frame["method"] == "session/update"
            && frame["params"]["update"]["sessionUpdate"] == "user_message_chunk"
        {
            mirrored = Some(frame);
        }
    }
    let mirrored = mirrored.expect("no user_message_chunk reached the attached client");
    let text = mirrored["params"]["update"]["content"]["text"]
        .as_str()
        .unwrap();
    assert!(text.contains("MAIL_BODY_SENTINEL"), "{mirrored:#?}");
    let _ = client.kill();
    let _ = client.wait();
}

/// RECEIPT. Without a host the five existing arms are untouched: a
/// coordinator route with no pane still queues exactly as it did.
#[test]
fn a_route_with_no_host_takes_the_old_path() {
    let fixture = Fixture::new("no-host");

    let hail = fixture.boop(&["hail", "--to", "coord", "--body", "unhosted", "--from", "tester"]);

    assert!(hail.status.success(), "{}", String::from_utf8_lossy(&hail.stderr));
    let stdout = String::from_utf8_lossy(&hail.stdout);
    assert!(stdout.contains("(no pane)"), "{stdout}");
    assert!(!stdout.contains("acp host"), "{stdout}");
    assert!(fixture.frames("session/prompt").is_empty());
}

/// RECEIPT. Restart costs no new persisted field: `sessionId` was already on
/// the route, and the second host loads it rather than opening a new one.
#[test]
fn a_restarted_host_loads_the_pinned_session() {
    let fixture = Fixture::new("restart");
    let first = fixture.host("coord");
    wait_for("the session id to reach the route", || {
        fixture.session_id("coord") == "stub-session-1"
    });
    assert_eq!(fixture.frames("session/new").len(), 1);
    drop(first);
    wait_for("the socket to be released", || {
        !std::os::unix::net::UnixStream::connect(fixture.socket("coord")).is_ok()
    });

    let _second = fixture.host("coord");

    wait_for("the second host to load the session", || {
        !fixture.frames("session/load").is_empty()
    });
    let loads = fixture.frames("session/load");
    assert_eq!(loads[0]["params"]["sessionId"], "stub-session-1");
    assert_eq!(
        fixture.frames("session/new").len(),
        1,
        "the restarted host opened a second session"
    );
}

/// RECEIPT. The queueing capability is read off `initialize` and carried, and
/// the delivery timing is unchanged while the policy is unsettled.
#[test]
fn the_queueing_capability_is_read_off_initialize() {
    let fixture = Fixture::new("queueing");
    let mut child = fixture
        .command(&["acp", "host", "coord", "--cwd", &fixture.repo.display().to_string()])
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stderr = BufReader::new(child.stderr.take().unwrap());
    // Read to EOF rather than stopping at the match: dropping the reader early
    // closes the host's stderr and kills it mid-handshake.
    let found = std::thread::spawn(move || {
        stderr.lines().map_while(Result::ok).fold(false, |seen, line| {
            // The fmt layer paints the `=` between a field name and its value,
            // so the pair is matched in two pieces.
            seen || (line.contains("prompt_queueing") && line.contains("advertised"))
        })
    });
    let host = Host(child);
    wait_for("the handshake to reach the route", || {
        fixture.session_id("coord") == "stub-session-1"
    });
    drop(host);

    assert!(found.join().unwrap(), "the host never logged the capability");
}
