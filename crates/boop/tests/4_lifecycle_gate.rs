//! Authenticated scenarios share one driver and the production Harness readers.
//! Each invocation owns its tmux server, Boop database and launch scripts.

use std::{path::{Path, PathBuf}, process::{Command, Stdio}, time::{Duration, Instant}};
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
        return Ok(());
    }
    let root = std::env::temp_dir().join(format!("boop-reader-exact-{}", std::process::id()));
    std::fs::create_dir(&root)?;
    let project = root.join(".claude/projects/-test-boop-reader");
    std::fs::create_dir_all(&project)?;
    std::fs::write(project.join("owned-reader.jsonl"), "{\"type\":\"assistant\",\"uuid\":\"a\",\"message\":{\"content\":[{\"type\":\"text\",\"text\":\"fixture-answer\"}]}}\n")?;
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
impl LifecycleHarness for Codex {
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
        args.extend(["--model".into(), "gpt-5.6-luna".into(), "-c".into(), "model_reasoning_effort=low".into(), "--no-alt-screen".into()]);
        args
    }
}

// ccz changes launch/configuration, retaining exactly the Claude operations.
struct Claude { entry: &'static str }
impl LifecycleHarness for Claude {
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
}

struct OpenCode;
impl LifecycleHarness for OpenCode {
    fn entry(&self) -> &str { "opencode" }
    fn id(&self) -> HarnessId { HarnessId::Opencode }
    fn launch_args(&self, resume: Option<&str>) -> Vec<String> {
        resume.map(|id| vec!["--session".into(), id.into()]).unwrap_or_default()
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
    fn store(&self) -> Result<Store> { Store::open(self.root.join("boop.db")) }
    fn route(&self) -> Result<bus::Route> {
        bus::routes_in(&self.store()?)?.remove(&self.route).context("wrapper route not observed")
    }
    fn boop(&self, args: &[&str]) -> Result<String> {
        use boop_store::testing::BoopCommandExt;
        bounded(Command::new(env!("CARGO_BIN_EXE_boop")).args(args)
            .boop_test_root(&self.root).env_remove("BOOP_READER_HOME")
            .env("BOOP_DOOR_FLOOR", "30")
            .env("BOOP_NO_SYNC", "1").env("BOOP_DB", self.root.join("boop.db"))
            .env("BOOP_MAIL_DIR", self.root.join("mail")))
    }
    fn launch(&self, harness: &dyn LifecycleHarness, resume: Option<&str>) -> Result<()> {
        let binary = Path::new(env!("CARGO_BIN_EXE_boop"));
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
            ("BOOP_DB", self.root.join("boop.db").display().to_string()),
            ("BOOP_MAIL_DIR", self.root.join("mail").display().to_string()),
            ("BOOP_CONFIG", self.root.join("config.json").display().to_string()),
            ("BOOP_SESSION", "e2e-parent".into()), ("BOOP_LANE", "e2e-parent".into()),
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
            "test_owned":true,"database":self.root.join("boop.db"),"at_ms":boop::live::now_ms()
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
        self.tmux(&["kill-server"])?;
        self.cleaned.set(true);
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            let alive: Vec<_> = owned.iter().copied().filter(|pid| boop::live::pid_alive(*pid)).collect();
            if alive.is_empty() { return Ok(json!({"owned_pids":owned,"surviving_pids":alive})); }
            ensure!(Instant::now() < deadline, "test-owned processes survived tmux cleanup: {alive:?}");
            std::thread::sleep(Duration::from_millis(250));
        }
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
                ensure!(route.kind == "coordinator" && route.harness == Some(harness.id()));
                ensure!(route.parent.as_deref() == Some("e2e-parent"));
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
    let body = format!("Bounded lifecycle test. Do not use tools. Reply exactly {answer}.");
    let sent = fixture.boop(&["beep", &fixture.route, &body, "--as", "e2e-parent", "--no-wait"])?;
    let deadline = Instant::now() + Duration::from_secs(65);
    loop {
        let route = fixture.route()?;
        if let Ok(rows) = harness.observe(registry, &route) {
            let users = rows.iter().filter(|row| matches!(row.role.as_str(), "user" | "meta") && row.text.contains(&body)).count();
            let answers = rows.iter().filter(|row| row.role == "assistant" && row.text.trim() == answer).count();
            ensure!(users <= 1 && answers <= 1, "duplicate native receipt: {users} user, {answers} assistant");
            if users == 1 && answers == 1 {
                let store = fixture.store()?;
                let message = bus::messages_in(&store)?.into_iter().find(|row| row.body == body).context("missing sent envelope")?;
                ensure!(store.delivery_accepted(&message.id, &fixture.route)?);
                let routes = bus::routes_in(&store)?;
                for _ in 0..2 { ensure!(boop::mail::deliver_hail(registry, &store, &routes, &message)?.rung == boop::mail::Rung::AlreadyAccepted); }
                std::thread::sleep(Duration::from_secs(5));
                let after = harness.observe(registry, &route)?;
                ensure!(after.iter().filter(|row| matches!(row.role.as_str(), "user" | "meta") && row.text.contains(&body)).count() == users);
                ensure!(after.iter().filter(|row| row.role == "assistant" && row.text.trim() == answer).count() == answers);
                return Ok(json!({"message_id":message.id,"thread":route.session_id,"user_count":users,"answer_count":answers,"send":sent}));
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
    fixture.launch(harness, Some(thread))?;
    let resumed = await_route(fixture, harness)?;
    ensure!(resumed.session_id == route.session_id, "resume selected a different conversation");
    let receipt = nonce(fixture, harness, registry, label)?;
    ensure!(fixture.store()?.trace_of(thread)?.as_deref() == Some(trace.as_str()), "resume changed Boop trace");
    let new_pid = fixture.store()?.live_row(thread)?.and_then(|row| row.pid).context("resumed session has no PID")?;
    ensure!(new_pid != old_pid && boop::live::pid_alive(new_pid as u32));
    Ok(json!({"receipt":receipt,"trace":trace,"old_pid":old_pid,"new_pid":new_pid,"graceful_exit":true}))
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
    let mut fixture = Fixture { root, socket: format!("boop-e2e-{}-{}", entry, std::process::id()), pane: String::new(), route: String::new(), cwd: std::env::current_dir()?, launch_index: std::cell::Cell::new(0), cleaned: std::cell::Cell::new(false) };
    let mut cases = Vec::new();
    let result = (|| -> Result<()> {
        let version = bounded(Command::new(harness.entry()).arg("--version"))
            .map_err(|error| anyhow::anyhow!("BLOCKED: native executable prerequisite: {error:#}"))?;
        cases.push(json!({"scenario":"native_executable","status":"PASS","version":version.trim()}));
        let registry = Registry::discover();
        let _adapter = harness.adapter(&registry);
        fixture.pane = fixture.tmux(&["new-session", "-d", "-P", "-F", "#{pane_id}", "-s", "lifecycle", "sleep 240"] )?.trim().into();
        fixture.tmux(&["set-option", "-g", "remain-on-exit", "on"])?;
        fixture.route = format!("{}-{}", harness.id(), fixture.pane.trim_start_matches('%'));
        fixture.boop(&["beep", "agent", "register", "e2e-parent", "--kind", "coordinator",
            "--harness", harness.id().as_str(), "--cwd", fixture.cwd.to_str().unwrap()])?;
        fixture.launch(harness.as_ref(), None)?;
        let route = await_route(&fixture, harness.as_ref())?;
        cases.push(json!({"scenario":"fresh_wrapper_identity","status":"PASS","route":fixture.route,
            "thread":route.session_id,"parent":route.parent,"pane":route.tmux,"kind":route.kind.as_str(),"mode":route.mode}));
        let receipt = nonce(&fixture, harness.as_ref(), &registry, "idle")?;
        cases.push(json!({"scenario":"idle_receipt_and_accepted_retry","status":"PASS","receipt":receipt}));
        fixture.capture("idle")?;
        let receipt = resume(&fixture, harness.as_ref(), &registry, "resume")?;
        cases.push(json!({"scenario":"exit_and_resume_across_processes","status":"PASS","receipt":receipt}));
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
        let receipt = resume(&fixture, harness.as_ref(), &registry, "compact_resume")?;
        cases.push(json!({"scenario":"resume_after_compact","status":"PASS","receipt":receipt}));
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
        let receipt = resume(&fixture, harness.as_ref(), &registry, "clear_resume")?;
        cases.push(json!({"scenario":"resume_after_clear","status":"PASS","receipt":receipt}));
        // Remaining transitions must enter this shared driver before full
        // assurance can pass. They are never ignored or reported as supported.
        anyhow::bail!("remaining shared lifecycle scenarios are not implemented yet")
    })();
    if let Err(error) = &result {
        let detail = format!("{error:#}");
        cases.push(json!({"scenario":"next_transition","status":if detail.starts_with("BLOCKED:") { "BLOCKED" } else { "FAIL" },"detail":detail}));
    }
    let cleanup = fixture.cleanup();
    match &cleanup {
        Ok(receipt) => cases.push(json!({"scenario":"process_cleanup","status":"PASS","receipt":receipt})),
        Err(error) => cases.push(json!({"scenario":"process_cleanup","status":"FAIL","detail":format!("{error:#}")})),
    }
    std::fs::write(fixture.root.join("matrix.json"), serde_json::to_vec_pretty(&json!({"entry":entry,"build":boop::BUILD,"cases":cases}))?)?;
    eprintln!("live matrix: {}", fixture.root.join("matrix.json").display());
    result.and(cleanup.map(|_| ()))
}
