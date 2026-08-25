//! The blocking mail wait, taken from the real binary: what it prints, what it
//! stamps, and the exit code plus re-run line a timeout leaves behind.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};

static NEXT: AtomicUsize = AtomicUsize::new(0);

struct Fixture {
    home: PathBuf,
    mail: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Fixture {
        let home = std::env::temp_dir().join(format!(
            "boop-wait-mail-{}-{}-{name}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&home);
        let mail = home.join("mail");
        std::fs::create_dir_all(&mail).unwrap();
        Fixture { home, mail }
    }

    fn boop(&self, args: &[&str]) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_boop"));
        command
            .env_clear()
            .env("HOME", &self.home)
            .env("BOOP_DB", self.home.join("boop.db"))
            .env("PATH", "/usr/bin:/bin")
            .args(args)
            .arg("--mail-dir")
            .arg(&self.mail)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        command
    }

    fn rows(&self) -> Vec<serde_json::Value> {
        let text = std::fs::read_to_string(self.mail.join("bus.ndjson")).unwrap_or_default();
        text.lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.home);
    }
}

fn append(mail: &Path, row: serde_json::Value) {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(mail.join("bus.ndjson"))
        .unwrap();
    writeln!(file, "{row}").unwrap();
}

fn reply_row(id: &str, reply_to: &str, body: &str) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "from": "feature-answers",
        "to": "asker",
        "from_timestamp": boop::bus::now_iso(),
        "to_timestamp": null,
        "kind": "note",
        "reply_to": reply_to,
        "body": body,
        "ref": null,
    })
}

/// The id `hail` minted, read back off the row it appended.
fn queued_id(fixture: &Fixture) -> String {
    fixture.rows()[0]["id"].as_str().unwrap().to_owned()
}

#[test]
fn hail_prints_the_wait_hint_naming_the_id_it_queued() {
    let fixture = Fixture::new("hint");
    let output = fixture
        .boop(&[
            "beep",
            "hail",
            "feature-answers",
            "--body",
            "question",
            "--from",
            "asker",
        ])
        .output()
        .unwrap();
    assert!(output.status.success(), "hail exits 0");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = queued_id(&fixture);
    assert_eq!(
        stdout.lines().last().unwrap(),
        format!("to await the reply: boop wait {id}   (or: boop wait --me &)"),
        "the last line of a hail is the command that awaits its reply: {stdout}"
    );
}

#[test]
fn a_reply_ends_the_wait_with_its_body_and_a_delivery_stamp() {
    let fixture = Fixture::new("reply");
    fixture
        .boop(&[
            "beep",
            "hail",
            "feature-answers",
            "--body",
            "question",
            "--from",
            "asker",
        ])
        .output()
        .unwrap();
    let id = queued_id(&fixture);

    let waiting = fixture
        .boop(&["wait", &id, "--wait-timeout", "30"])
        .spawn()
        .unwrap();
    // The wait re-reads the mailbox once a second, so the reply lands after it
    // has already blocked at least once.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    append(&fixture.mail, reply_row("m-answer", &id, "the answer is 4"));
    let output = waiting.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(0), "an answered wait exits 0");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("the answer is 4"),
        "the wait prints the reply body: {stdout}"
    );
    assert_eq!(
        stdout.lines().last().unwrap(),
        "re-arm: boop wait --me &",
        "the last line is the next command: {stdout}"
    );
    let stamped = fixture
        .rows()
        .into_iter()
        .filter(|row| row["id"] == "m-answer")
        .any(|row| !row["to_timestamp"].is_null());
    assert!(stamped, "the printed reply is stamped delivered");
}

#[test]
fn a_delivered_reply_is_not_replayed_by_a_second_wait() {
    let fixture = Fixture::new("replay");
    fixture
        .boop(&[
            "beep",
            "hail",
            "feature-answers",
            "--body",
            "question",
            "--from",
            "asker",
        ])
        .output()
        .unwrap();
    let id = queued_id(&fixture);
    append(&fixture.mail, reply_row("m-answer", &id, "the answer is 4"));

    let first = fixture
        .boop(&["wait", &id, "--wait-timeout", "10"])
        .output()
        .unwrap();
    assert_eq!(first.status.code(), Some(0), "the first wait is answered");
    let second = fixture
        .boop(&["wait", &id, "--wait-timeout", "1"])
        .output()
        .unwrap();
    assert_eq!(
        second.status.code(),
        Some(124),
        "a taken reply is history, so the second wait blocks and times out"
    );
}

#[test]
fn a_timeout_exits_124_with_the_re_run_line_on_both_streams() {
    let fixture = Fixture::new("timeout");
    let output = fixture
        .boop(&["wait", "m-never", "--wait-timeout", "1"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(124), "a timeout exits 124");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let expected = format!(
        "timed out after 1s waiting for reply to m-never; re-run: boop wait m-never --wait-timeout 1 --mail-dir {}",
        fixture.mail.display()
    );
    assert_eq!(
        stdout.lines().last().unwrap(),
        expected,
        "stdout ends with the re-run line: {stdout}"
    );
    assert_eq!(
        stderr.lines().last().unwrap(),
        expected,
        "a redirected stdout must not hide the re-run line: {stderr}"
    );
}

#[test]
fn me_takes_every_unread_row_addressed_to_the_named_inbox() {
    let fixture = Fixture::new("me");
    append(
        &fixture.mail,
        serde_json::json!({
            "id": "m-one",
            "from": "coordinator",
            "to": "soopy-driver",
            "from_timestamp": boop::bus::now_iso(),
            "to_timestamp": null,
            "kind": "request",
            "reply_to": null,
            "body": "first order",
            "ref": null,
        }),
    );
    append(
        &fixture.mail,
        serde_json::json!({
            "id": "m-two",
            "from": "coordinator",
            "to": "soopy-driver",
            "from_timestamp": boop::bus::now_iso(),
            "to_timestamp": null,
            "kind": "hail",
            "reply_to": null,
            "body": "second order",
            "ref": null,
        }),
    );
    append(
        &fixture.mail,
        serde_json::json!({
            "id": "m-elsewhere",
            "from": "coordinator",
            "to": "someone-else",
            "from_timestamp": boop::bus::now_iso(),
            "to_timestamp": null,
            "kind": "request",
            "reply_to": null,
            "body": "not yours",
            "ref": null,
        }),
    );

    let output = fixture
        .boop(&[
            "wait",
            "--me",
            "--as",
            "soopy-driver",
            "--wait-timeout",
            "10",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "unread mail answers --me");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("first order") && stdout.contains("second order"),
        "every unread row is printed: {stdout}"
    );
    assert!(
        !stdout.contains("not yours"),
        "another inbox's mail is not mine: {stdout}"
    );

    let second = fixture
        .boop(&[
            "wait",
            "--me",
            "--as",
            "soopy-driver",
            "--wait-timeout",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(
        second.status.code(),
        Some(124),
        "the drained inbox blocks instead of replaying"
    );
}

#[test]
fn me_without_a_name_watches_the_inbox_the_spawn_stamp_names() {
    let fixture = Fixture::new("stamp");
    append(
        &fixture.mail,
        serde_json::json!({
            "id": "m-stamped",
            "from": "coordinator",
            "to": "feature-stamped-lane",
            "from_timestamp": boop::bus::now_iso(),
            "to_timestamp": null,
            "kind": "request",
            "reply_to": null,
            "body": "for the lane the env names",
            "ref": null,
        }),
    );
    let output = fixture
        .boop(&["wait", "--me", "--wait-timeout", "10"])
        .env("BOOP_SESSION", "feature-stamped-lane")
        .env("BOOP_LANE", "feature-stamped-lane")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "the env rung resolves --me");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("for the lane the env names"),
        "the lane's own mail is what --me takes: {stdout}"
    );
}

#[test]
fn hail_with_a_wait_timeout_sends_then_blocks_and_times_out() {
    let fixture = Fixture::new("hailwait");
    let output = fixture
        .boop(&[
            "beep",
            "hail",
            "feature-answers",
            "--body",
            "question",
            "--from",
            "asker",
            "--wait-timeout",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(124),
        "hail --wait-timeout blocks on the reply and times out with it"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    let id = queued_id(&fixture);
    assert!(
        stdout.contains(&format!("to await the reply: boop wait {id}")),
        "the hint still prints before the block: {stdout}"
    );
    assert!(
        stdout.lines().last().unwrap().starts_with(&format!(
            "timed out after 1s waiting for reply to {id}; re-run:"
        )),
        "the last line is the re-run command: {stdout}"
    );
}

// FAIL-PRE-FIX: a parent had to send with one verb and wait with another,
// pasting the id between them; `push` is both, and its exits are typed.
#[test]
fn push_sends_and_blocks_until_the_reply_arrives() {
    let fixture = Fixture::new("push-reply");
    let pushing = fixture
        .boop(&[
            "push",
            "feature-answers",
            "--body",
            "question",
            "--from",
            "asker",
            "--timeout",
            "30",
        ])
        .spawn()
        .unwrap();
    // The push re-reads the mailbox twice a second, so the reply lands after
    // it has already blocked at least once.
    std::thread::sleep(std::time::Duration::from_millis(1500));
    let id = queued_id(&fixture);
    append(
        &fixture.mail,
        reply_row("m-push-answer", &id, "the answer is 4"),
    );
    let output = pushing.wait_with_output().unwrap();

    assert_eq!(output.status.code(), Some(0), "an answered push exits 0");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let lines: Vec<&str> = stdout.lines().collect();
    assert!(
        lines.iter().any(|line| line.contains(&id)
            && (line.starts_with("held") || line.starts_with("delivered"))),
        "one delivery line naming the rung: {stdout}"
    );
    assert!(
        lines.contains(&"the answer is 4"),
        "the reply body is printed: {stdout}"
    );
    assert_eq!(
        lines.last(),
        Some(&format!("boop wait {id}").as_str()),
        "the last line is the next command: {stdout}"
    );
}

/// RECEIPT (push). A route that never answers exits 124, and both streams
/// carry the command that reads what happened.
#[test]
fn push_exits_124_and_names_the_debug_command() {
    let fixture = Fixture::new("push-timeout");
    let output = fixture
        .boop(&[
            "push",
            "feature-silent",
            "--body",
            "question",
            "--from",
            "asker",
            "--timeout",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(124), "a silent push exits 124");
    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert_eq!(
        stdout.lines().last(),
        Some("boop debug feature-silent"),
        "the last line reads what happened: {stdout}"
    );
    assert!(
        stderr.contains("boop debug feature-silent"),
        "the next command survives a redirected stdout: {stderr}"
    );
}

/// RECEIPT (push). A push at a name with no route still lands: the row is
/// held in the mailbox and the sender is told which rung took it.
#[test]
fn push_at_an_unregistered_name_still_reports_a_rung() {
    let fixture = Fixture::new("push-noroute");
    let output = fixture
        .boop(&[
            "push",
            "nobody",
            "--body",
            "into the void",
            "--from",
            "asker",
            "--timeout",
            "1",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(124), "nothing answered");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("held") && stdout.contains("no registry route for nobody"),
        "the rung and its reason are printed: {stdout}"
    );
}

/// Defect 3 (addendum 2026-08-25): `wait --me` handed the lane its own
/// dispatch row back as the next unread row, hours after the spawn turn ate
/// it. With that row the only one in the box, the wait has nothing and times
/// out.
#[test]
fn me_never_hands_back_the_lanes_own_dispatch_row() {
    let fixture = Fixture::new("dispatch");
    append(
        &fixture.mail,
        serde_json::json!({
            "id": "m-d2252d54",
            "from": "coordinator",
            "to": "feature-cx-a",
            "from_timestamp": boop::bus::now_iso(),
            "to_timestamp": null,
            "kind": "dispatch",
            "reply_to": null,
            "body": "the brief the spawn already injected",
            "ref": null,
        }),
    );
    let output = fixture
        .boop(&["wait", "--me", "--as", "feature-cx-a", "--wait-timeout", "1"])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(124),
        "a dispatch row is not unread mail"
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("the brief the spawn already injected"),
        "stdout: {stdout}"
    );
}

/// Defect 3, the other half: a row to a route naming no harness stops at
/// `held-in-mailbox`, and a polling `wait --me` is the only thing that ever
/// finds it. That transition must not read as delivered.
#[test]
fn me_still_takes_a_row_held_in_the_mailbox_for_a_native_route() {
    let fixture = Fixture::new("held");
    std::fs::write(
        fixture.mail.join("registry.json"),
        serde_json::json!({
            "native-n1": {"kind": "native", "parent": "feature-cx-a"},
            "native-n2": {"kind": "native", "parent": "feature-cx-b"},
        })
        .to_string(),
    )
    .unwrap();
    let sent = fixture
        .boop(&[
            "beep",
            "hail",
            "native-n1",
            "--as",
            "native-n2",
            "--body",
            "ping from the sibling native",
        ])
        .output()
        .unwrap();
    assert_eq!(sent.status.code(), Some(0), "hail to a native route lands");
    assert!(
        String::from_utf8_lossy(&sent.stdout).contains("held-in-mailbox")
            || String::from_utf8_lossy(&sent.stdout).contains("held"),
        "stdout: {}",
        String::from_utf8_lossy(&sent.stdout)
    );
    let output = fixture
        .boop(&["wait", "--me", "--as", "native-n1", "--wait-timeout", "10"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(0), "the held row answers --me");
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("ping from the sibling native"),
        "stdout: {stdout}"
    );
}
