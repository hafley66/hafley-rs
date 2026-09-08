//! Authenticated scenarios share one driver and the production Harness readers.
//! Each invocation owns its tmux server, Boop database and launch scripts.

use std::{path::PathBuf, process::{Command, Stdio}, time::{Duration, Instant}};
use anyhow::{Context, Result, ensure};
use boop::{bus, harness::{Harness, HarnessId, shell_quote}, Registry, Store};
use serde_json::{Value, json};
use wait_timeout::ChildExt;

#[test]
fn claude_reader_resolves_exact_session() -> Result<()> {
    let cwd = "/test/boop-reader";
    if std::env::var_os("BOOP_E2E_READER_CHILD").is_some() {
        let registry = Registry::discover();
        let adapter = registry.get(HarnessId::Claude);
        let session = adapter.session_by_id("owned-reader", Some(cwd)).context("exact fixture session was not resolved")?;
        let messages = adapter.messages(&session, None);
        ensure!(messages.iter().map(|m| (m.role.as_str(), m.text.as_str())).collect::<Vec<_>>() == vec![("assistant", "fixture-answer")]);
        ensure!(adapter.native_settings(&session) == Some(boop::harness::NativeTuiEvent::Settings {
            session_id: "owned-reader".into(), model: Some("fixture-model".into()), effort: Some("high".into())
        }), "native effort metadata was not observed");
        return Ok(());
    }
    let root = std::env::temp_dir().join(format!("boop-reader-exact-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    let project = root.join(".claude/projects/-test-boop-reader");
    std::fs::create_dir_all(&project)?;
    std::fs::write(project.join("owned-reader.jsonl"), "{\"type\":\"assistant\",\"uuid\":\"a\",\"effort\":\"high\",\"message\":{\"model\":\"fixture-model\",\"content\":[{\"type\":\"text\",\"text\":\"fixture-answer\"}]}}\n")?;
    let result = bounded(Command::new(std::env::current_exe()?)
        .args(["--exact", "t4_lifecycle_gate::claude_reader_resolves_exact_session", "--nocapture"])
        .env("BOOP_E2E_READER_CHILD", "1").env("BOOP_READER_HOME", &root));
    std::fs::remove_file(project.join("owned-reader.jsonl"))?;
    std::fs::remove_dir(&project)?;
    std::fs::remove_dir(root.join(".claude/projects"))?;
    std::fs::remove_dir(root.join(".claude"))?;
    std::fs::remove_dir(&root)?;
    result.map(|_| ())
}

#[test]
fn native_registry_process_transition_preserves_trace() -> Result<()> {
    use boop_store::testing::BoopCommandExt;
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!("boop-registry-transition-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    let sessions = root.join("sessions");
    std::fs::create_dir(&sessions)?;
    let native = root.join("claude-fixture");
    std::fs::write(&native, r#"#!/bin/sh
printf '{"pid":%s,"sessionId":"first","cwd":"%s","updatedAt":9999999999999}\n' "$$" "$PWD" > "$BOOP_CLAUDE_SESSIONS_DIR/$$.json"
sleep 2
printf '{"pid":%s,"sessionId":"second","cwd":"%s","updatedAt":9999999999999}\n' "$$" "$PWD" > "$BOOP_CLAUDE_SESSIONS_DIR/$$.json"
exec sleep 30
"#)?;
    std::fs::set_permissions(&native, std::fs::Permissions::from_mode(0o700))?;
    let spec = boop::harness::NativeTuiSpec { executable: env!("CARGO_BIN_EXE_boop").into(), cwd: root.clone(), args: Vec::new(), env: Vec::new() };
    let mut owner = boop::harness::NativeTuiPlan::direct(&spec);
    owner.frontend = Some(Command::new(&spec.executable)
        .args(["tui", "claude", "--name", "owned", "--bin"]).arg(&native).arg("--cwd").arg(&root)
        .boop_test_root(&root).env("BOOP_CLAUDE_SESSIONS_DIR", &sessions)
        .env("BOOP_NO_SYNC", "1").env("BOOP_DB", root.join("boop.db"))
        .env("BOOP_MAIL_DIR", root.join("mail"))
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?);
    let store = Store::open(root.join("boop.db"))?;
    let deadline = Instant::now() + Duration::from_secs(12);
    let pid = loop {
        if let Some(route) = bus::routes_in(&store)?.get("owned") {
            if route.session_id.as_deref() == Some("second") {
                let pid = store.live_row("second")?.and_then(|row| row.pid).context("no second process observation")?;
                ensure!(store.trace_of("first")?.as_deref() == Some("trace-first"));
                ensure!(store.trace_of("second")?.as_deref() == Some("trace-first"));
                ensure!(store.live_row("first")?.unwrap().status.as_deref() == Some("detached"));
                break pid;
            }
        }
        ensure!(Instant::now() < deadline, "same-process registry transition was not bound");
        std::thread::sleep(Duration::from_millis(100));
    };
    owner.stop();
    ensure!(!boop::live::pid_alive(pid as u32), "fixture native process survived owner cleanup");
    Ok(())
}

trait LifecycleHarness {
    fn entry(&self) -> &str;
    fn id(&self) -> HarnessId;
    fn launch_args(&self, resume: Option<&str>) -> Vec<String>;
    fn control(&self, fixture: &Fixture, command: &str) -> Result<()> {
        fixture.tmux(&["send-keys", "-t", &fixture.pane, "-l", command])?;
        std::thread::sleep(Duration::from_millis(250));
        fixture.tmux(&["send-keys", "-t", &fixture.pane, "Enter"])?;
        Ok(())
    }
    fn exit(&self, fixture: &Fixture) -> Result<()> { self.control(fixture, "/exit") }
    fn automatic_restart(&self) -> bool { false }
    fn approve_completion(&self, _fixture: &Fixture, _command: &str) -> Result<()> { Ok(()) }
    fn backend(&self, _fixture: &Fixture) -> Result<Option<(u32, bool)>> { Ok(None) }
    fn change_settings(&self, _fixture: &Fixture) -> Result<(String, String)> {
        anyhow::bail!("settings controls have not been verified for this adapter")
    }
    fn busy(&self, fixture: &Fixture, registry: &Registry) -> Result<bool> {
        let route = fixture.route()?;
        Ok(self.adapter(registry).live().live_session_for_route(&route)?
            .is_some_and(|session| Some(&session.session_id) == route.session_id.as_ref()
                && session.status == boop::live::LiveStatus::Busy))
    }
    fn compact_count(&self, _registry: &Registry, _route: &bus::Route) -> Result<usize> {
        anyhow::bail!("compact observation is not implemented for this adapter")
    }
    fn observe(&self, registry: &Registry, route: &bus::Route) -> Result<Vec<boop_harness::transcript::Message>> {
        let adapter = registry.get(self.id());
        let session = adapter.session_by_id(route.session_id.as_deref().context("unbound route")?, route.cwd.as_deref())
            .context("native transcript not observed")?;
        Ok(adapter.messages(&session, None))
    }
    fn adapter<'a>(&self, registry: &'a Registry) -> &'a dyn Harness { registry.get(self.id()) }
}

struct Codex;
impl Codex {
    fn rpc(&self, fixture: &Fixture, method: &str, params: Value) -> Result<Value> {
        let route = fixture.route()?;
        let stream = std::os::unix::net::UnixStream::connect(route.app_server_socket.context("owned Codex socket absent")?)?;
        stream.set_read_timeout(Some(Duration::from_secs(15)))?;
        stream.set_write_timeout(Some(Duration::from_secs(15)))?;
        let (mut socket, _) = tungstenite::client("ws://localhost/", stream)?;
        for (id, method, params) in [
            (1, "initialize", json!({"clientInfo":{"name":"boop-lifecycle-gate","version":"1"},"capabilities":{"experimentalApi":true}})),
            (2, method, params),
        ] {
            socket.send(tungstenite::Message::Text(json!({"id":id,"method":method,"params":params}).to_string().into()))?;
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                ensure!(Instant::now() < deadline, "Codex {method} deadline exceeded");
                let message = socket.read()?;
                if !message.is_text() { continue; }
                let value: Value = serde_json::from_str(message.to_text()?)?;
                if value["id"] != id { continue; }
                ensure!(value.get("error").is_none(), "Codex {method}: {}", value["error"]);
                if id == 2 { return Ok(value["result"].clone()); }
                socket.send(tungstenite::Message::Text(json!({"method":"initialized","params":{}}).to_string().into()))?;
                break;
            }
        }
        unreachable!()
    }
}
impl LifecycleHarness for Codex {
    fn backend(&self, fixture: &Fixture) -> Result<Option<(u32, bool)>> {
        let socket = fixture.route()?.app_server_socket.context("owned Codex backend address absent")?;
        Ok(Some((fixture.owned_command_pid(&socket)?, true)))
    }
    fn entry(&self) -> &str { "codex" }
    fn id(&self) -> HarnessId { HarnessId::Codex }
    fn compact_count(&self, registry: &Registry, route: &bus::Route) -> Result<usize> {
        Ok(native_records(self.adapter(registry), route)?.iter().filter(|row| row["type"] == "compacted").count())
    }
    fn exit(&self, fixture: &Fixture) -> Result<()> {
        fixture.tmux(&["send-keys", "-t", &fixture.pane, "C-d"])?;
        Ok(())
    }
    fn launch_args(&self, resume: Option<&str>) -> Vec<String> {
        let mut args = resume.map(|id| vec!["resume".into(), id.into()]).unwrap_or_default();
        if resume.is_none() {
            args.extend(["--model".into(), "gpt-5.6-luna".into(), "-c".into(), "model_reasoning_effort=low".into()]);
        }
        args.push("--no-alt-screen".into());
        args
    }
    fn change_settings(&self, fixture: &Fixture) -> Result<(String, String)> {
        self.rpc(fixture, "thread/settings/update", json!({"threadId":fixture.route()?.session_id,"model":"gpt-5.6-terra","effort":"high"}))?;
        Ok(("gpt-5.6-terra".into(), "high".into()))
    }
    fn busy(&self, fixture: &Fixture, _registry: &Registry) -> Result<bool> {
        let result = self.rpc(fixture, "thread/read", json!({"threadId":fixture.route()?.session_id,"includeTurns":false}))?;
        Ok(result.pointer("/thread/status/type").and_then(Value::as_str) == Some("active"))
    }
}

// ccz changes launch/configuration, retaining exactly the Claude operations.
struct Claude { entry: &'static str }
impl LifecycleHarness for Claude {
    fn approve_completion(&self, fixture: &Fixture, command: &str) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(35);
        let compact = |text: &str| text.chars().filter(|c| !c.is_whitespace() && !matches!(c, '│' | '┃')).collect::<String>();
        loop {
            let screen = fixture.tmux(&["capture-pane", "-p", "-t", &fixture.pane, "-S", "-60"])?;
            if screen.contains("This command requires approval") {
                ensure!(compact(&screen).contains(&compact(command)), "approval prompt differs from the authorized test-owned completion command");
                std::fs::write(fixture.root.join("completion-approval.txt"), screen)?;
                fixture.tmux(&["send-keys", "-t", &fixture.pane, "Enter"])?;
                return Ok(());
            }
            if bus::messages_in(&fixture.store()?)?.iter().any(|message| message.from == fixture.route && message.to == fixture.parent && message.kind == "result") {
                return Ok(());
            }
            ensure!(Instant::now() < deadline, "completion command neither executed nor requested its scoped approval");
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    fn entry(&self) -> &str { self.entry }
    fn id(&self) -> HarnessId { HarnessId::Claude }
    fn compact_count(&self, registry: &Registry, route: &bus::Route) -> Result<usize> {
        Ok(native_records(self.adapter(registry), route)?.iter().filter(|row| row["isCompactSummary"] == true).count())
    }
    fn launch_args(&self, resume: Option<&str>) -> Vec<String> {
        let mut args = resume.map(|id| vec!["--resume".into(), id.into()]).unwrap_or_default();
        if let Ok(model) = std::env::var(format!("BOOP_E2E_{}_MODEL", self.entry.to_uppercase())) {
            args.extend(["--model".into(), model]);
        }
        args
    }
    fn change_settings(&self, fixture: &Fixture) -> Result<(String, String)> {
        let _ = fixture;
        anyhow::bail!("BLOCKED: installed Claude /model and /effort persist user defaults; isolated authenticated settings storage is required")
    }
}

struct OpenCode;
impl LifecycleHarness for OpenCode {
    fn backend(&self, fixture: &Fixture) -> Result<Option<(u32, bool)>> {
        let address = url::Url::parse(&fixture.route()?.app_server_socket.context("owned OpenCode backend address absent")?)?;
        let port = address.port().context("owned OpenCode port absent")?;
        Ok(Some((fixture.owned_command_pid(&format!("serve --port {port}"))?, false)))
    }
    fn entry(&self) -> &str { "opencode" }
    fn id(&self) -> HarnessId { HarnessId::Opencode }
    fn compact_count(&self, registry: &Registry, route: &bus::Route) -> Result<usize> {
        let session = self.adapter(registry).session_by_id(route.session_id.as_deref().context("no OpenCode thread")?, route.cwd.as_deref()).context("OpenCode native database absent")?;
        let db = rusqlite::Connection::open_with_flags(session.path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        Ok(db.query_row("SELECT COUNT(*) FROM message WHERE session_id=?1 AND json_extract(data,'$.role')='assistant' AND json_extract(data,'$.summary')=1 AND json_extract(data,'$.time.completed') IS NOT NULL",
            [&session.session_id], |row| row.get::<_, i64>(0))? as usize)
    }
    fn launch_args(&self, resume: Option<&str>) -> Vec<String> {
        let mut args = resume.map(|id| vec!["--session".into(), id.into()]).unwrap_or_default();
        if let Ok(model) = std::env::var("BOOP_E2E_OPENCODE_MODEL") {
            args.extend(["--model".into(), model]);
        }
        args
    }
}

fn native_records(adapter: &dyn Harness, route: &bus::Route) -> Result<Vec<Value>> {
    let session = adapter.session_by_id(route.session_id.as_deref().context("no bound session")?, route.cwd.as_deref())
        .context("no exact native session")?;
    Ok(std::fs::read_to_string(session.path)?.lines().filter_map(|line| serde_json::from_str(line).ok()).collect())
}

fn bounded(command: &mut Command) -> Result<String> {
    let mut child = command.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn()?;
    // CLI responses are bounded in size here. TUI output stays in tmux.
    if child.wait_timeout(Duration::from_secs(20))?.is_none() {
        child.kill()?;
        let _ = child.wait();
        anyhow::bail!("command exceeded 20 seconds");
    }
    let output = child.wait_with_output()?;
    ensure!(output.status.success(), "command exited {}: {}", output.status, String::from_utf8_lossy(&output.stderr));
    Ok(String::from_utf8(output.stdout)?)
}

struct Fixture {
    root: PathBuf,
    binary: PathBuf,
    store_root: PathBuf,
    parent: String,
    owns_server: bool,
    socket: String,
    pane: String,
    route: String,
    cwd: PathBuf,
    launch_index: std::cell::Cell<usize>,
    cleaned: std::cell::Cell<bool>,
}
impl Fixture {
    fn tmux(&self, args: &[&str]) -> Result<String> {
        bounded(Command::new("tmux").args(["-L", &self.socket, "-f", "/dev/null"]).args(args))
    }
    fn store(&self) -> Result<Store> { Store::open(self.store_root.join("boop.db")) }
    fn route(&self) -> Result<bus::Route> {
        bus::routes_in(&self.store()?)?.remove(&self.route).context("wrapper route not observed")
    }
    fn boop(&self, args: &[&str]) -> Result<String> {
        use boop_store::testing::BoopCommandExt;
        bounded(Command::new(&self.binary).args(args)
            .boop_test_root(&self.root).env_remove("BOOP_READER_HOME")
            .env("BOOP_DOOR_FLOOR", "30")
            .env("BOOP_NO_SYNC", "1").env("BOOP_DB", self.store_root.join("boop.db"))
            .env("BOOP_MAIL_DIR", self.store_root.join("mail")))
    }
    fn launch(&self, harness: &dyn LifecycleHarness, resume: Option<&str>) -> Result<()> {
        let binary = self.binary.as_path();
        let path = format!("{}:{}", binary.parent().unwrap().display(), std::env::var("PATH").unwrap_or_default());
        let mut script = String::from("set -eu\n");
        for (key, _) in std::env::vars() {
            if key.starts_with("BOOP_") || ["CODEX_THREAD_ID", "CLAUDE_SESSION_ID", "CLAUDE_CODE_SESSION_ID", "OPENCODE_SESSION_ID", "KIMI_SESSION_ID"].contains(&key.as_str()) {
                // Environment names are shell identifiers, values never printed.
                if key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') { script.push_str(&format!("unset {key}\n")); }
            }
        }
        for (key, value) in [
            ("PATH", path), ("BOOP_NO_SYNC", "1".into()),
            ("BOOP_DOOR_FLOOR", "30".into()),
            ("BOOP_DB", self.store_root.join("boop.db").display().to_string()),
            ("BOOP_MAIL_DIR", self.store_root.join("mail").display().to_string()),
            ("BOOP_CONFIG", self.root.join("config.json").display().to_string()),
            ("BOOP_SESSION", self.parent.clone()), ("BOOP_LANE", self.parent.clone()),
        ] { script.push_str(&format!("export {key}={}\n", shell_quote(&value))); }
        script.push_str("eval \"$(boop shell-init bash)\"\n");
        script.push_str(&shell_quote(harness.entry()));
        for arg in harness.launch_args(resume) { script.push(' '); script.push_str(&shell_quote(&arg)); }
        script.push('\n');
        let index = self.launch_index.get();
        self.launch_index.set(index + 1);
        let launch = self.root.join(format!("{index}_launch.bash"));
        std::fs::write(&launch, script)?;
        let command = format!("timeout -s TERM -k 10s 240s bash {}", shell_quote(&launch.display().to_string()));
        let hash = bounded(Command::new("shasum").args(["-a", "256"]).arg(binary))?;
        std::fs::write(self.root.join(format!("{index}_launch.json")), serde_json::to_vec_pretty(&json!({
            "entry":harness.entry(),"args":harness.launch_args(resume),"binary":binary,
            "sha256":hash.split_whitespace().next(),"build":boop::BUILD,
            "command":command,"cwd":self.cwd,"pane":self.pane,"tmux_socket":self.socket,
            "test_owned":true,"database":self.store_root.join("boop.db"),"at_ms":boop::live::now_ms()
        }))?)?;
        self.tmux(&["respawn-pane", "-k", "-t", &self.pane, "-c", self.cwd.to_str().unwrap(), &command])?;
        Ok(())
    }
    fn capture(&self, label: &str) -> Result<()> {
        let text = self.tmux(&["capture-pane", "-p", "-t", &self.pane, "-S", "-200"])?;
        std::fs::write(self.root.join(format!("{label}.txt")), text)?;
        Ok(())
    }
    fn cleanup(&self) -> Result<Value> {
        if self.cleaned.get() { return Ok(json!({"already_cleaned":true})); }
        if self.pane.is_empty() {
            self.cleaned.set(true);
            return Ok(json!({"started":false}));
        }
        let _ = self.capture("final-pane");
        let owned = self.owned_pids()?;
        if self.owns_server { self.tmux(&["kill-server"])?; }
        else { self.tmux(&["kill-pane", "-t", &self.pane])?; }
        self.cleaned.set(true);
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let alive: Vec<_> = owned.iter().copied().filter(|pid| boop::live::pid_alive(*pid)).collect();
            if alive.is_empty() { return Ok(json!({"owned_pids":owned,"surviving_pids":alive})); }
            ensure!(Instant::now() < deadline, "test-owned processes survived tmux cleanup: {alive:?}");
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    fn owned_pids(&self) -> Result<std::collections::BTreeSet<u32>> {
        let pid: u32 = self.tmux(&["display-message", "-p", "-t", &self.pane, "#{pane_pid}"])?.trim().parse()?;
        let processes = bounded(Command::new("ps").args(["-axo", "pid=,ppid="]))?;
        let processes: Vec<(u32, u32)> = processes.lines().filter_map(|line| {
            let mut columns = line.split_whitespace();
            Some((columns.next()?.parse().ok()?, columns.next()?.parse().ok()?))
        }).collect();
        let mut owned = std::collections::BTreeSet::from([pid]);
        loop {
            let children: Vec<_> = processes.iter().filter(|(_, parent)| owned.contains(parent)).map(|(pid, _)| *pid).collect();
            let before = owned.len();
            owned.extend(children);
            if owned.len() == before { break; }
        }
        Ok(owned)
    }
    fn owned_command_pid(&self, needle: &str) -> Result<u32> {
        let owned = self.owned_pids()?;
        let ids = owned.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
        let output = bounded(Command::new("ps").args(["-p", &ids, "-o", "pid=,command="]))?;
        let matches: Vec<u32> = output.lines().filter(|line| line.contains(needle))
            .filter_map(|line| line.split_whitespace().next()?.parse().ok()).collect();
        ensure!(matches.len() == 1 && owned.contains(&matches[0]), "backend selector did not identify one test-owned process");
        Ok(matches[0])
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn await_route(fixture: &Fixture, harness: &dyn LifecycleHarness) -> Result<bus::Route> {
    let deadline = Instant::now() + Duration::from_secs(35);
    loop {
        if let Ok(route) = fixture.route() {
            if route.session_id.is_some() {
                let live = fixture.store()?.live_row(route.session_id.as_deref().unwrap())?;
                if !live.and_then(|row| row.pid).is_some_and(|pid| boop::live::pid_alive(pid as u32)) {
                    ensure!(fixture.tmux(&["display-message", "-p", "-t", &fixture.pane, "#{pane_dead}"])?.trim() != "1", "native frontend exited before live identity was established");
                    std::thread::sleep(Duration::from_millis(250));
                    ensure!(Instant::now() < deadline, "no live frontend identity within 35 seconds");
                    continue;
                }
                ensure!(route.kind == "coordinator" && route.harness == Some(harness.id()));
                ensure!(route.parent.as_deref() == Some(fixture.parent.as_str()));
                ensure!(route.tmux.as_deref() == Some(fixture.pane.as_str()));
                ensure!(route.cwd.as_deref() == fixture.cwd.to_str());
                return Ok(route);
            }
        }
        ensure!(Instant::now() < deadline, "no observed wrapper identity within 35 seconds");
        std::thread::sleep(Duration::from_millis(250));
    }
}

fn nonce(fixture: &Fixture, harness: &dyn LifecycleHarness, registry: &Registry, label: &str) -> Result<Value> {
    let answer = format!("ACK_BOOP_E2E_{}_{}_{}", harness.entry(), std::process::id(), label);
    let body = format!("Bounded lifecycle test. Do not use tools. Reply with this exact line, without added punctuation or commentary:\n{answer}");
    let sent = fixture.boop(&["beep", &fixture.route, &body, "--as", "e2e-parent", "--no-wait"])?;
    await_receipt(fixture, harness, registry, &body, &answer, &sent)
}

fn incoming_body_matches(text: &str, body: &str) -> bool {
    text.trim() == body || text.contains(&format!("\n{body}\n"))
}

#[test]
fn inbound_receipts_exclude_instructions_quoting_the_body() {
    let body = "lane owned done rc=0";
    assert_eq!([
        body, "prefix\nlane owned done rc=0\nsuffix", "When the message \"lane owned done rc=0\" arrives, reply ACK",
    ].map(|text| incoming_body_matches(text, body)), [true, true, false]);
}

fn await_receipt(fixture: &Fixture, harness: &dyn LifecycleHarness, registry: &Registry, body: &str, answer: &str, sent: &str) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(65);
    loop {
        let route = fixture.route()?;
        if let Ok(rows) = harness.observe(registry, &route) {
            let users = rows.iter().filter(|row| matches!(row.role.as_str(), "user" | "meta") && incoming_body_matches(&row.text, body)).count();
            let answers = rows.iter().filter(|row| row.role == "assistant" && row.text.lines().any(|line| line.trim() == answer)).count();
            ensure!(users <= 1 && answers <= 1, "duplicate native receipt: {users} user, {answers} assistant");
            if users == 1 && answers == 1 {
                let screen = fixture.tmux(&["capture-pane", "-p", "-t", &fixture.pane, "-S", "-100"])?;
                if !screen.contains(answer) {
                    ensure!(Instant::now() < deadline, "native transcript answered, but the intended TUI did not display its answer");
                    std::thread::sleep(Duration::from_millis(500));
                    continue;
                }
                let store = fixture.store()?;
                let message = bus::messages_in(&store)?.into_iter().find(|row| row.body == body).context("missing sent envelope")?;
                ensure!(store.delivery_accepted(&message.id, &fixture.route)?);
                let routes = bus::routes_in(&store)?;
                for _ in 0..2 { ensure!(boop::mail::deliver_hail(registry, &store, &routes, &message)?.rung == boop::mail::Rung::AlreadyAccepted); }
                std::thread::sleep(Duration::from_secs(5));
                let after = harness.observe(registry, &route)?;
                ensure!(after.iter().filter(|row| matches!(row.role.as_str(), "user" | "meta") && incoming_body_matches(&row.text, body)).count() == users);
                ensure!(after.iter().filter(|row| row.role == "assistant" && row.text.lines().any(|line| line.trim() == answer)).count() == answers);
                let session = harness.adapter(registry).session_by_id(route.session_id.as_deref().unwrap(), route.cwd.as_deref()).context("receipt session disappeared")?;
                let settings = harness.adapter(registry).native_settings(&session).context("native settings absent after answer")?;
                let boop::harness::NativeTuiEvent::Settings { session_id, model, effort } = settings else { anyhow::bail!("unexpected native settings event") };
                let current = fixture.route()?;
                ensure!(Some(&session_id) == current.session_id.as_ref(), "settings belong to another thread");
                ensure!(model.is_some() && current.model == model, "route model differs from native execution: {:?} vs {:?}", current.model, model);
                ensure!(store.session_attr(&session_id, "effort")? == effort, "stored effort differs from native execution");
                let pid = store.live_row(&session_id)?.and_then(|row| row.pid).context("receipt has no native PID")?;
                ensure!(boop::live::pid_alive(pid as u32), "receipt PID is no longer live");
                ensure!(current.parent.as_deref() == Some(fixture.parent.as_str()) && current.tmux.as_deref() == Some(fixture.pane.as_str()), "receipt route ownership changed");
                let pane_receipt = fixture.root.join(format!("receipt-{}.txt", message.id));
                std::fs::write(&pane_receipt, screen)?;
                return Ok(json!({"message_id":message.id,"thread":route.session_id,"user_count":users,"answer_count":answers,"send":sent,"model":model,"effort":effort,
                    "route":fixture.route,"kind":current.kind.as_str(),"parent":current.parent,"pane":current.tmux,"pid":pid,
                    "trace":store.trace_of(&session_id)?,"native_source":session.path,"pane_receipt":pane_receipt}));
            }
        }
        if Instant::now() >= deadline {
            let pane = fixture.tmux(&["capture-pane", "-p", "-t", &fixture.pane, "-S", "-80"])?;
            if let Some(error) = pane.lines().find(|line| line.contains("subscription plan") && line.contains("include access")) {
                anyhow::bail!("BLOCKED: provider rejected current subscription access: {}", error.trim());
            }
            anyhow::bail!("no native nonce answer within 65 seconds");
        }
        std::thread::sleep(Duration::from_millis(500));
    }
}

fn resume(fixture: &Fixture, harness: &dyn LifecycleHarness, registry: &Registry, label: &str) -> Result<Value> {
    let route = fixture.route()?;
    let thread = route.session_id.as_deref().context("no conversation to resume")?;
    let trace = fixture.store()?.trace_of(thread)?.context("native session has no Boop trace")?;
    let old_pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid).context("native session has no observed PID")?;
    harness.exit(fixture)?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while fixture.tmux(&["display-message", "-p", "-t", &fixture.pane, "#{pane_dead}"])?.trim() != "1" {
        ensure!(Instant::now() < deadline, "native exit control did not finish within 15 seconds");
        std::thread::sleep(Duration::from_millis(250));
    }
    ensure!(!boop::live::pid_alive(old_pid as u32), "old frontend survived exit");
    let answer = format!("STALE_ACK_{}_{}_{}", harness.entry(), std::process::id(), label);
    let body = format!("Bounded lifecycle test. Do not use tools. Reply with this exact line, without added punctuation or commentary:\n{answer}");
    let sent = fixture.boop(&["beep", &fixture.route, &body, "--as", "e2e-parent", "--no-wait"])?;
    let message = bus::messages_in(&fixture.store()?)?.into_iter().find(|message| message.body == body).context("stale-route envelope absent")?;
    ensure!(!fixture.store()?.delivery_accepted(&message.id, &fixture.route)?, "stale route falsely accepted delivery");
    ensure!(!harness.observe(registry, &route)?.iter().any(|row| row.text.contains(&body)), "stale route reached a native transcript before resume");
    fixture.launch(harness, Some(thread))?;
    let resumed = await_route(fixture, harness)?;
    ensure!(resumed.session_id == route.session_id, "resume selected a different conversation");
    let receipt = await_receipt(fixture, harness, registry, &body, &answer, &sent)?;
    ensure!(fixture.store()?.trace_of(thread)?.as_deref() == Some(trace.as_str()), "resume changed Boop trace");
    let new_pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid).context("resumed session has no PID")?;
    ensure!(new_pid != old_pid && boop::live::pid_alive(new_pid as u32));
    Ok(json!({"receipt":receipt,"trace":trace,"old_pid":old_pid,"new_pid":new_pid,"graceful_exit":true,"held_while_stale":true}))
}

#[test]
#[ignore = "authenticated four-harness matrix: just boop-check live"]
fn authenticated_matrix() -> Result<()> {
    let entry = std::env::var("BOOP_E2E_ENTRY").context("BOOP_E2E_ENTRY required")?;
    let harness: Box<dyn LifecycleHarness> = match entry.as_str() {
        "codex" => Box::new(Codex), "claude" => Box::new(Claude { entry: "claude" }),
        "ccz" => Box::new(Claude { entry: "ccz" }), "opencode" => Box::new(OpenCode),
        _ => anyhow::bail!("unknown matrix entry {entry}"),
    };
    let root = PathBuf::from(std::env::var("BOOP_E2E_ROOT").context("BOOP_E2E_ROOT required")?)
        .join(format!("{}-{}", entry, std::process::id()));
    std::fs::create_dir_all(root.parent().unwrap())?;
    std::fs::create_dir(&root).context("each live invocation requires a fresh directory")?;
    std::fs::create_dir(root.join("bin"))?;
    let binary = root.join("bin/boop");
    std::fs::copy(env!("CARGO_BIN_EXE_boop"), &binary)?;
    let cwd = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("reports/lifecycle-consolidation/.live-workspaces")
        .join(format!("{}-{}", entry, std::process::id()));
    std::fs::create_dir_all(cwd.parent().unwrap())?;
    std::fs::create_dir(&cwd)?;
    let mut fixture = Fixture { binary, store_root: root.clone(), parent: "e2e-parent".into(), owns_server: true, root, socket: format!("boop-e2e-{}-{}", entry, std::process::id()), pane: String::new(), route: String::new(), cwd, launch_index: std::cell::Cell::new(0), cleaned: std::cell::Cell::new(false) };
    let mut cases = Vec::new();
    let mut active_scenario = "native_executable";
    let result = (|| -> Result<()> {
        let version = bounded(Command::new(harness.entry()).arg("--version"))
            .map_err(|error| anyhow::anyhow!("BLOCKED: native executable prerequisite: {error:#}"))?;
        cases.push(json!({"scenario":"native_executable","status":"PASS","version":version.trim()}));
        active_scenario = "fresh_wrapper_identity";
        let registry = Registry::discover();
        let _adapter = harness.adapter(&registry);
        fixture.pane = fixture.tmux(&["new-session", "-d", "-P", "-F", "#{pane_id}", "-s", "lifecycle", "sleep 240"] )?.trim().into();
        fixture.tmux(&["set-option", "-g", "remain-on-exit", "on"])?;
        fixture.route = format!("{}-{}", harness.id(), fixture.pane.trim_start_matches('%'));
        fixture.boop(&["beep", "agent", "register", "e2e-parent", "--kind", "coordinator",
            "--harness", harness.id().as_str(), "--cwd", fixture.cwd.to_str().unwrap()])?;
        fixture.launch(harness.as_ref(), None)?;
        let route = await_route(&fixture, harness.as_ref())?;
        let receipt = nonce(&fixture, harness.as_ref(), &registry, "idle")?;
        let thread = route.session_id.as_deref().context("fresh thread absent")?;
        let trace = fixture.store()?.trace_of(thread)?.context("fresh trace absent")?;
        let pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid).context("fresh PID absent")?;
        cases.push(json!({"scenario":"fresh_wrapper_identity","status":"PASS","route":fixture.route,
            "thread":route.session_id,"parent":route.parent,"pane":route.tmux,"kind":route.kind.as_str(),"mode":route.mode,"pid":pid,"trace":trace}));
        cases.push(json!({"scenario":"idle_receipt_and_accepted_retry","status":"PASS","receipt":receipt}));
        fixture.capture("idle")?;
        active_scenario = "busy_receipt";
        let busy_answer = format!("BUSY_DONE_{}_{}", harness.entry(), std::process::id());
        harness.control(&fixture, &format!("Bounded lifecycle test: run the shell command sleep 8 once, then reply exactly {busy_answer}. This authorizes that sleep only. Preserve and answer any incoming peer message separately."))?;
        let deadline = Instant::now() + Duration::from_secs(25);
        while !harness.busy(&fixture, &registry)? {
            ensure!(Instant::now() < deadline, "native busy state not observed within 25 seconds");
            std::thread::sleep(Duration::from_millis(100));
        }
        let receipt = nonce(&fixture, harness.as_ref(), &registry, "busy")?;
        let rows = harness.observe(&registry, &fixture.route()?)?;
        ensure!(rows.iter().filter(|row| row.role == "assistant" && row.text.lines().any(|line| line.trim() == busy_answer)).count() == 1,
            "busy initiating turn did not complete exactly once");
        cases.push(json!({"scenario":"busy_receipt","status":"PASS","native_busy_observed":true,"receipt":receipt}));
        active_scenario = "exit_and_resume_across_processes";
        let receipt = resume(&fixture, harness.as_ref(), &registry, "resume")?;
        cases.push(json!({"scenario":"exit_and_resume_across_processes","status":"PASS","receipt":receipt}));
        cases.push(json!({"scenario":"stale_route","status":"PASS","held_before_resume":receipt["held_while_stale"],"receipt":receipt["receipt"]}));
        active_scenario = "compact";
        let before = harness.compact_count(&registry, &fixture.route()?)?;
        harness.control(&fixture, "/compact")?;
        let deadline = Instant::now() + Duration::from_secs(65);
        while harness.compact_count(&registry, &fixture.route()?)? <= before {
            ensure!(Instant::now() < deadline, "native compaction receipt not observed within 65 seconds");
            std::thread::sleep(Duration::from_millis(500));
        }
        ensure!(fixture.route()?.session_id == route.session_id, "compact changed native conversation");
        let receipt = nonce(&fixture, harness.as_ref(), &registry, "compact")?;
        cases.push(json!({"scenario":"compact","status":"PASS","receipt":receipt}));
        active_scenario = "resume_after_compact";
        let receipt = resume(&fixture, harness.as_ref(), &registry, "compact_resume")?;
        cases.push(json!({"scenario":"resume_after_compact","status":"PASS","receipt":receipt}));
        active_scenario = "clear_new_session";
        let old = fixture.route()?;
        let trace = fixture.store()?.trace_of(old.session_id.as_deref().unwrap())?;
        harness.control(&fixture, "/clear")?;
        let deadline = Instant::now() + Duration::from_secs(25);
        while fixture.route()?.session_id.as_ref().is_none_or(|id| Some(id) == old.session_id.as_ref()) {
            ensure!(Instant::now() < deadline, "clear did not rebind to a new native session within 25 seconds");
            std::thread::sleep(Duration::from_millis(250));
        }
        let new = await_route(&fixture, harness.as_ref())?;
        ensure!(fixture.store()?.trace_of(new.session_id.as_deref().unwrap())? == trace, "clear changed Boop trace");
        let receipt = nonce(&fixture, harness.as_ref(), &registry, "clear")?;
        cases.push(json!({"scenario":"clear_new_session","status":"PASS","old_thread":old.session_id,"new_thread":new.session_id,"receipt":receipt}));
        active_scenario = "resume_after_clear";
        let receipt = resume(&fixture, harness.as_ref(), &registry, "clear_resume")?;
        cases.push(json!({"scenario":"resume_after_clear","status":"PASS","receipt":receipt}));
        active_scenario = "model_and_effort_change";
        match harness.change_settings(&fixture) {
            Ok((model, effort)) => {
                let receipt = nonce(&fixture, harness.as_ref(), &registry, "settings")?;
                ensure!(receipt["model"] == model && receipt["effort"] == effort, "requested settings differ from native execution");
                cases.push(json!({"scenario":"model_and_effort_change","status":"PASS","receipt":receipt}));
                active_scenario = "settings_across_resume";
                let receipt = resume(&fixture, harness.as_ref(), &registry, "settings_resume")?;
                ensure!(receipt["receipt"]["model"] == model && receipt["receipt"]["effort"] == effort, "resume did not preserve observed settings");
                cases.push(json!({"scenario":"settings_across_resume","status":"PASS","receipt":receipt}));
            }
            Err(error) if error.to_string().starts_with("BLOCKED:") => {
                for scenario in ["model_and_effort_change", "settings_across_resume"] {
                    cases.push(json!({"scenario":scenario,"status":"BLOCKED","detail":error.to_string()}));
                }
            }
            Err(error) => return Err(error),
        }
        active_scenario = "abnormal_exit";
        let before = fixture.route()?;
        let thread = before.session_id.as_deref().context("crash test has no thread")?;
        let model = json!(before.model);
        let effort = json!(fixture.store()?.session_attr(thread, "effort")?);
        let old_pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid).context("crash test has no PID")? as u32;
        ensure!(fixture.owned_pids()?.contains(&old_pid), "refusing to signal PID outside this test pane");
        std::thread::sleep(Duration::from_secs(10));
        bounded(Command::new("kill").args(["-KILL", &old_pid.to_string()]))?;
        let deadline = Instant::now() + Duration::from_secs(35);
        while boop::live::pid_alive(old_pid) {
            ensure!(Instant::now() < deadline, "test frontend survived SIGKILL");
            std::thread::sleep(Duration::from_millis(100));
        }
        if !harness.automatic_restart() {
            while fixture.tmux(&["display-message", "-p", "-t", &fixture.pane, "#{pane_dead}"])?.trim() != "1" {
                ensure!(Instant::now() < deadline, "wrapper did not exit after frontend crash");
                std::thread::sleep(Duration::from_millis(250));
            }
            fixture.launch(harness.as_ref(), Some(thread))?;
        }
        let restarted = await_route(&fixture, harness.as_ref())?;
        ensure!(restarted.session_id == before.session_id && restarted.parent == before.parent, "crash restart changed identity linkage");
        let new_pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid).context("restart has no PID")? as u32;
        ensure!(new_pid != old_pid && !boop::live::pid_alive(old_pid), "crash did not replace frontend PID");
        let receipt = nonce(&fixture, harness.as_ref(), &registry, "crash_restart")?;
        ensure!(receipt["model"] == model && receipt["effort"] == effort, "crash restart lost settings");
        cases.push(json!({"scenario":"abnormal_exit","status":"PASS","automatic_restart":harness.automatic_restart(),"old_pid":old_pid,"new_pid":new_pid,"receipt":receipt}));
        active_scenario = "backend_restart";
        if let Some((backend, automatic)) = harness.backend(&fixture)? {
            ensure!(fixture.owned_pids()?.contains(&backend), "refusing to kill a backend outside the test pane");
            std::thread::sleep(Duration::from_secs(10));
            bounded(Command::new("kill").args(["-KILL", &backend.to_string()]))?;
            let deadline = Instant::now() + Duration::from_secs(35);
            if !automatic {
                while fixture.tmux(&["display-message", "-p", "-t", &fixture.pane, "#{pane_dead}"])?.trim() != "1" {
                    ensure!(Instant::now() < deadline, "wrapper did not exit after backend crash");
                    std::thread::sleep(Duration::from_millis(250));
                }
                fixture.launch(harness.as_ref(), Some(thread))?;
            }
            loop {
                let pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid);
                if pid.is_some_and(|pid| pid as u32 != new_pid && boop::live::pid_alive(pid as u32)) { break; }
                ensure!(Instant::now() < deadline, "backend restart did not replace the frontend");
                std::thread::sleep(Duration::from_millis(250));
            }
            let restored = await_route(&fixture, harness.as_ref())?;
            ensure!(restored.session_id == before.session_id && restored.parent == before.parent, "backend restart changed linkage");
            ensure!(!boop::live::pid_alive(backend) && !boop::live::pid_alive(new_pid), "old backend/frontend survived restart");
            let receipt = nonce(&fixture, harness.as_ref(), &registry, "backend_restart")?;
            ensure!(receipt["model"] == model && receipt["effort"] == effort, "backend restart lost settings");
            cases.push(json!({"scenario":"backend_restart","status":"PASS","automatic":automatic,"old_backend":backend,"receipt":receipt}));
        } else {
            cases.push(json!({"scenario":"backend_restart","status":"UNSUPPORTED","detail":"production native launch plan runs this harness directly, without a separate owned backend"}));
        }
        active_scenario = "concurrent_session_isolation";
        let root = fixture.root.join("concurrent");
        std::fs::create_dir(&root)?;
        let pane = fixture.tmux(&["new-window", "-d", "-P", "-F", "#{pane_id}", "-t", "lifecycle", "-n", "concurrent", "sleep 240"] )?.trim().to_owned();
        let other = Fixture { root, binary: fixture.binary.clone(), store_root: fixture.store_root.clone(), parent: "e2e-parent".into(), owns_server: false,
            route: format!("{}-{}", harness.id(), pane.trim_start_matches('%')), pane, socket: fixture.socket.clone(), cwd: fixture.cwd.clone(),
            launch_index: std::cell::Cell::new(0), cleaned: std::cell::Cell::new(false) };
        other.launch(harness.as_ref(), None)?;
        let second = await_route(&other, harness.as_ref())?;
        let first = fixture.route()?;
        ensure!(first.session_id != second.session_id && first.tmux != second.tmux && first.cwd == second.cwd, "concurrent identities collided");
        ensure!(fixture.store()?.trace_of(first.session_id.as_deref().unwrap())? != fixture.store()?.trace_of(second.session_id.as_deref().unwrap())?, "unrelated concurrent sessions share a trace");
        let a = nonce(&fixture, harness.as_ref(), &registry, "isolation_a")?;
        let b = nonce(&other, harness.as_ref(), &registry, "isolation_b")?;
        let first_rows = harness.observe(&registry, &first)?;
        let second_rows = harness.observe(&registry, &second)?;
        ensure!(!first_rows.iter().any(|row| row.text.contains(&format!("ACK_BOOP_E2E_{}_{}_isolation_b", harness.entry(), std::process::id()))), "second-session message leaked into first transcript");
        ensure!(!second_rows.iter().any(|row| row.text.contains(&format!("ACK_BOOP_E2E_{}_{}_isolation_a", harness.entry(), std::process::id()))), "first-session message leaked into second transcript");
        cases.push(json!({"scenario":"concurrent_session_isolation","status":"PASS","first":a,"second":b,"same_cwd":fixture.cwd}));
        active_scenario = "child_completion_parent_receipt";
        fixture.boop(&["beep", "agent", "register", &fixture.route, "--parent", &other.route])?;
        fixture.parent = other.route.clone();
        ensure!(fixture.route()?.parent.as_deref() == Some(other.route.as_str()), "child parent edge was not updated");
        let completion = format!("lane {} done rc=0", fixture.route);
        let answer = format!("PARENT_DONE_{}_{}", harness.entry(), std::process::id());
        let ready = format!("PARENT_READY_{}_{}", harness.entry(), std::process::id());
        harness.control(&other, &format!("Bounded parent-receipt test. When the peer completion message {completion:?} arrives, reply with this exact line:\n{answer}\nFor now reply with this exact line:\n{ready}"))?;
        let deadline = Instant::now() + Duration::from_secs(35);
        while !harness.observe(&registry, &other.route()?)?.iter().any(|row| row.role == "assistant" && row.text.lines().any(|line| line.trim() == ready)) {
            ensure!(Instant::now() < deadline, "parent did not acknowledge completion expectation");
            std::thread::sleep(Duration::from_millis(250));
        }
        let command = format!("{} beep agent done {} --rc 0", shell_quote(&fixture.binary.display().to_string()), shell_quote(&fixture.route));
        harness.control(&fixture, &format!("Bounded child-completion test. Run exactly this shell command to complete your own test-owned Boop route. It uses your inherited test database. Do not edit any files.\n{command}\nThen reply CHILD_COMPLETION_SENT."))?;
        harness.approve_completion(&fixture, &command)?;
        let receipt = await_receipt(&other, harness.as_ref(), &registry, &completion, &answer, "completion command executed by live child")?;
        let results: Vec<_> = bus::messages_in(&fixture.store()?)?.into_iter().filter(|message| message.from == fixture.route && message.to == other.route && message.kind == "result").collect();
        ensure!(results.len() == 1 && results[0].rc == Some(0), "child completion envelope is missing or duplicated");
        cases.push(json!({"scenario":"child_completion_parent_receipt","status":"PASS","relationship":"Boop route parent","child_thread":first.session_id,"parent_thread":second.session_id,"receipt":receipt}));
        let cleanup = other.cleanup()?;
        cases.push(json!({"scenario":"concurrent_process_cleanup","status":"PASS","receipt":cleanup}));
        Ok(())
    })();
    if let Err(error) = &result {
        let detail = format!("{error:#}");
        cases.push(json!({"scenario":active_scenario,"status":if detail.starts_with("BLOCKED:") { "BLOCKED" } else { "FAIL" },"detail":detail}));
    }
    let cleanup = fixture.cleanup();
    match &cleanup {
        Ok(receipt) => cases.push(json!({"scenario":"process_cleanup","status":"PASS","receipt":receipt})),
        Err(error) => cases.push(json!({"scenario":"process_cleanup","status":"FAIL","detail":format!("{error:#}")})),
    }
    for scenario in ["fresh_wrapper_identity", "idle_receipt_and_accepted_retry", "busy_receipt",
        "exit_and_resume_across_processes", "compact", "resume_after_compact", "clear_new_session",
        "resume_after_clear", "model_and_effort_change", "settings_across_resume", "abnormal_exit",
        "stale_route", "backend_restart", "concurrent_session_isolation", "child_completion_parent_receipt"] {
        if !cases.iter().any(|case| case["scenario"] == scenario) {
            cases.push(json!({"scenario":scenario,"status":"BLOCKED","detail":format!("not executed: shared driver stopped at {active_scenario}")}));
        }
    }
    std::fs::write(fixture.root.join("matrix.json"), serde_json::to_vec_pretty(&json!({"entry":entry,"build":boop::BUILD,"cases":cases}))?)?;
    eprintln!("live matrix: {}", fixture.root.join("matrix.json").display());
    result?;
    cleanup?;
    let incomplete: Vec<_> = cases.iter().filter(|case| matches!(case["status"].as_str(), Some("FAIL" | "BLOCKED")))
        .filter_map(|case| case["scenario"].as_str()).collect();
    ensure!(incomplete.is_empty(), "live coverage incomplete: {incomplete:?}");
    Ok(())
}
