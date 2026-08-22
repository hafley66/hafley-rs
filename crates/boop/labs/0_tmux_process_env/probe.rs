//! A host receipt for resolving a live harness process from one reused tmux pane.
//!
//! Run with:
//!
//!     cargo run -p boop --example tmux_process_env
//!
//! The probe creates one tmux pane with its normal interactive shell. One sourced
//! driver runs a short-lived Claude-shaped child and then a Codex-shaped child in
//! that same pane. The Rust parent changes phases through files, rather than using
//! tmux key injection after the initial `source` command.

use std::{
    env,
    error::Error,
    ffi::OsString,
    fs, io,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    process::{Command, Output},
    thread,
    time::{Duration, Instant},
};

use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};

const ENV_KEYS: [&str; 4] = [
    "BOOP_HARNESS",
    "BOOP_ROUTE_ID",
    "BOOP_INHERITED_MARKER",
    "BOOP_MUTATED_MARKER",
];

type Result<T> = std::result::Result<T, Box<dyn Error>>;

struct Fixture {
    root: PathBuf,
    session: String,
    pane: String,
}

impl Fixture {
    fn new() -> Result<Self> {
        let unique = format!("boop-tmux-process-env-{}", std::process::id());
        let root = env::temp_dir().join(&unique);
        if root.exists() {
            fs::remove_dir_all(&root)?;
        }
        fs::create_dir(&root)?;

        let session = unique;
        tmux(["new-session", "-d", "-s", &session])?;
        let pane = tmux_text(["display-message", "-p", "-t", &session, "#{pane_id}"])?;

        Ok(Self {
            root,
            session,
            pane,
        })
    }

    fn write_scripts(&self) -> Result<()> {
        write_executable(
            &self.root.join("harness.sh"),
            r#"#!/bin/bash
set -eu
phase="$1"
printf '%s\n' "$$" > "$phase.pid"
export BOOP_MUTATED_MARKER="mutated-${BOOP_HARNESS}"
touch "$phase.ready"
while [ ! -e "$phase.release" ]; do sleep 0.05; done
"#,
        )?;
        write_executable(
            &self.root.join("driver.sh"),
            r#"#!/bin/bash
set -eu
state="$1"
harness="$2"
BOOP_HARNESS=claude BOOP_ROUTE_ID=route-claude BOOP_INHERITED_MARKER=inherited-claude "$harness" "$state/first"
BOOP_HARNESS=codex BOOP_ROUTE_ID=route-codex BOOP_INHERITED_MARKER=inherited-codex "$harness" "$state/second"
"#,
        )?;
        fs::create_dir(self.root.join("state"))?;
        Ok(())
    }

    fn start_driver(&self) -> Result<()> {
        let driver = self.root.join("driver.sh");
        let state = self.root.join("state");
        let harness = self.root.join("harness.sh");
        tmux([
            "send-keys",
            "-t",
            &self.pane,
            &format!(
                "source {} {} {}",
                driver.display(),
                state.display(),
                harness.display()
            ),
            "Enter",
        ])?;
        Ok(())
    }

    fn phase(&self, name: &'static str) -> Phase {
        Phase {
            name,
            root: self.root.join("state").join(name),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = tmux(["kill-session", "-t", &self.session]);
        let _ = fs::remove_dir_all(&self.root);
    }
}

struct Phase {
    name: &'static str,
    root: PathBuf,
}

impl Phase {
    fn wait_ready(&self) -> Result<u32> {
        wait_for(&self.root.with_extension("ready"))?;
        Ok(fs::read_to_string(self.root.with_extension("pid"))?
            .trim()
            .parse()?)
    }

    fn release(&self) -> Result<()> {
        fs::write(self.root.with_extension("release"), [])?;
        Ok(())
    }
}

fn main() -> Result<()> {
    if env::consts::OS != "macos" {
        return Err("this lab records macOS tmux and process APIs".into());
    }

    let fixture = Fixture::new()?;
    fixture.write_scripts()?;
    fixture.start_driver()?;

    let first = fixture.phase("first");
    let first_pid = first.wait_ready()?;
    let first_receipt = observe(&fixture, first.name, first_pid)?;
    assert_route(&first_receipt, "claude", "route-claude")?;
    first.release()?;

    let second = fixture.phase("second");
    let second_pid = second.wait_ready()?;
    let second_receipt = observe(&fixture, second.name, second_pid)?;
    assert_route(&second_receipt, "codex", "route-codex")?;
    second.release()?;

    if first_pid == second_pid {
        return Err("the two harness phases reused a PID; rerun the probe".into());
    }
    if first_receipt.pane_id != second_receipt.pane_id {
        return Err("the two harness phases did not remain in one tmux pane".into());
    }

    println!("result=PASS");
    println!("same_pane={}", first_receipt.pane_id);
    println!("first_pid={first_pid}");
    println!("second_pid={second_pid}");
    println!(
        "first_route={}",
        first_receipt.route_id.as_deref().unwrap_or("unavailable")
    );
    println!(
        "second_route={}",
        second_receipt.route_id.as_deref().unwrap_or("unavailable")
    );
    println!(
        "mutation_ps={}",
        first_receipt.ps_mutation.as_deref().unwrap_or("absent")
    );
    println!(
        "mutation_sysinfo={}",
        first_receipt
            .sysinfo_mutation
            .as_deref()
            .unwrap_or("absent")
    );
    Ok(())
}

struct Receipt {
    pane_id: String,
    route_id: Option<String>,
    ps_mutation: Option<String>,
    sysinfo_mutation: Option<String>,
}

fn observe(fixture: &Fixture, phase: &str, harness_pid: u32) -> Result<Receipt> {
    let pane_pid = tmux_text(["display-message", "-p", "-t", &fixture.pane, "#{pane_pid}"])?;
    let pane_tty = tmux_text(["display-message", "-p", "-t", &fixture.pane, "#{pane_tty}"])?;
    let pane_command = tmux_text([
        "display-message",
        "-p",
        "-t",
        &fixture.pane,
        "#{pane_current_command}",
    ])?;
    let pane_id = tmux_text(["display-message", "-p", "-t", &fixture.pane, "#{pane_id}"])?;
    let harness_pid_text = harness_pid.to_string();
    let process_before = ps_text([
        "-p",
        &harness_pid_text,
        "-o",
        "pid=,ppid=,pgid=,tpgid=,lstart=,command=",
    ])?;
    let tty = pane_tty.trim_start_matches("/dev/");
    let tty_processes = ps_text(["-t", tty, "-o", "pid=,ppid=,pgid=,tpgid=,state=,command="])?;
    let ps_environment = ps_text(["eww", "-p", &harness_pid_text, "-o", "command="])?;
    let sysinfo_environment = sysinfo_environment(harness_pid)?;
    let process_after = ps_text([
        "-p",
        &harness_pid_text,
        "-o",
        "pid=,ppid=,pgid=,tpgid=,lstart=,command=",
    ])?;

    if process_before != process_after {
        return Err(format!("{phase}: harness pid changed while reading it").into());
    }

    let ps_values = extract_environment(&ps_environment);
    let sysinfo_values = extract_os_environment(&sysinfo_environment);
    println!("phase={phase}");
    println!("pane_id={pane_id}");
    println!("pane_pid={pane_pid}");
    println!("pane_tty={pane_tty}");
    println!("pane_current_command={pane_command}");
    println!("process={process_before}");
    println!("tty_processes={}", one_line(&tty_processes));
    println!("ps_env={}", environment_receipt(&ps_values));
    println!("sysinfo_env={}", environment_receipt(&sysinfo_values));
    println!("ps_env_key_count={}", ps_values.len());
    println!("sysinfo_env_key_count={}", sysinfo_values.len());
    let ps_route = value(&ps_values, "BOOP_ROUTE_ID");
    let sysinfo_route = value(&sysinfo_values, "BOOP_ROUTE_ID");
    if ps_route.is_some() && sysinfo_route.is_some() && ps_route != sysinfo_route {
        return Err(format!("{phase}: ps and sysinfo selected different route ids").into());
    }
    let route_id = sysinfo_route.or(ps_route);
    println!(
        "route_selection={}",
        route_id.as_deref().unwrap_or("unavailable")
    );

    Ok(Receipt {
        pane_id,
        route_id,
        ps_mutation: value(&ps_values, "BOOP_MUTATED_MARKER"),
        sysinfo_mutation: value(&sysinfo_values, "BOOP_MUTATED_MARKER"),
    })
}

fn assert_route(receipt: &Receipt, harness: &str, route: &str) -> Result<()> {
    let Some(received) = receipt.route_id.as_deref() else {
        return Ok(());
    };
    if received != route {
        return Err(format!(
            "route selection expected {route} for {harness}, read {}",
            received
        )
        .into());
    }
    Ok(())
}

fn sysinfo_environment(pid: u32) -> Result<Vec<OsString>> {
    let pid = Pid::from_u32(pid);
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_environ(UpdateKind::Always),
    );
    system
        .process(pid)
        .map(|process| process.environ().to_vec())
        .ok_or_else(|| format!("sysinfo did not find pid {pid}").into())
}

fn extract_environment(text: &str) -> Vec<(String, String)> {
    ENV_KEYS
        .iter()
        .filter_map(|key| {
            text.split_whitespace().find_map(|word| {
                word.strip_prefix(&format!("{key}="))
                    .map(|value| ((*key).to_owned(), value.to_owned()))
            })
        })
        .collect()
}

fn extract_os_environment(entries: &[OsString]) -> Vec<(String, String)> {
    ENV_KEYS
        .iter()
        .filter_map(|key| {
            entries.iter().find_map(|entry| {
                entry
                    .to_string_lossy()
                    .strip_prefix(&format!("{key}="))
                    .map(|value| ((*key).to_owned(), value.to_owned()))
            })
        })
        .collect()
}

fn value(values: &[(String, String)], key: &str) -> Option<String> {
    values
        .iter()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.clone())
}

fn environment_receipt(values: &[(String, String)]) -> String {
    values
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join(",")
}

fn one_line(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn wait_for(path: &Path) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !path.exists() {
        if Instant::now() >= deadline {
            return Err(format!("timed out waiting for {}", path.display()).into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}

fn write_executable(path: &Path, body: &str) -> io::Result<()> {
    fs::write(path, body)?;
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(path, permissions)
}

fn tmux<const N: usize>(args: [&str; N]) -> Result<Output> {
    command("tmux", args)
}

fn tmux_text<const N: usize>(args: [&str; N]) -> Result<String> {
    Ok(String::from_utf8(tmux(args)?.stdout)?.trim().to_owned())
}

fn ps_text<const N: usize>(args: [&str; N]) -> Result<String> {
    Ok(String::from_utf8(command("ps", args)?.stdout)?
        .trim()
        .to_owned())
}

fn command<const N: usize>(program: &str, args: [&str; N]) -> Result<Output> {
    let output = Command::new(program).args(args).output()?;
    if output.status.success() {
        return Ok(output);
    }
    Err(format!(
        "{program} failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
    .into())
}
