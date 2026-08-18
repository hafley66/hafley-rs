use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::Result;
use boop::channel::{ChannelSpec, Delivery, LaneChannel, TurnEvent};
use boop::harness::{Harness, ReadChunk, SessionRef};
use boop::host::{run_chat, run_chat_with_adapter, ChatRequest, ChatResponse};

struct EchoHarness {
    db: PathBuf,
    fresh_opens: AtomicUsize,
    resumed_opens: AtomicUsize,
    next_turn: &'static AtomicI64,
}

impl Harness for EchoHarness {
    fn id(&self) -> &'static str {
        "echo"
    }

    fn sessions(&self) -> Result<Vec<SessionRef>> {
        Ok(Vec::new())
    }

    fn read_from(&self, _: &SessionRef, _: u64) -> Result<ReadChunk> {
        Ok(ReadChunk {
            events: Vec::new(),
            next_offset: 0,
            reset: false,
            skipped: 0,
        })
    }

    fn open_channel(&self, spec: &ChannelSpec) -> Result<Box<dyn LaneChannel>> {
        if spec.resume.is_some() {
            self.resumed_opens.fetch_add(1, Ordering::SeqCst);
        } else {
            self.fresh_opens.fetch_add(1, Ordering::SeqCst);
        }
        Ok(Box::new(EchoChannel {
            db: self.db.clone(),
            next_turn: &self.next_turn,
        }))
    }
}

struct EchoChannel {
    db: PathBuf,
    next_turn: &'static AtomicI64,
}

impl LaneChannel for EchoChannel {
    fn conversation_id(&self) -> Option<String> {
        Some("echo-session".to_owned())
    }

    fn start_turn(&mut self, text: &str) -> Result<()> {
        let turn = self.next_turn.fetch_add(1, Ordering::SeqCst);
        boop::Store::open(self.db.clone())?.write_turn(
            "echo-session",
            turn as u64,
            turn as u64,
            "assistant",
            &format!("echo {text}"),
        )?;
        Ok(())
    }

    fn steer(&mut self, _: &str) -> Result<Delivery> {
        Ok(Delivery::MidTurn)
    }

    fn next_event(&mut self, _: Duration) -> Result<Option<TurnEvent>> {
        Ok(Some(TurnEvent::ok("done")))
    }

    fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

fn temp_dir(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("boop-host-chat-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[test]
fn two_requests_resume_one_resident_conversation() {
    let root = temp_dir("root");
    let db = root.join("boop.db");
    boop::Store::open(db.clone())
        .unwrap()
        .project_discovered_session(&SessionRef {
            harness: "echo",
            session_id: "echo-session".into(),
            nickname: "echo-session".into(),
            path: PathBuf::new(),
            cwd: None,
            git_branch: None,
            modified_ms: 0,
            size: 0,
            tmux: None,
            tmux_socket: None,
            parent: None,
        })
        .unwrap();
    let adapter: &'static EchoHarness = Box::leak(Box::new(EchoHarness {
        db: db.clone(),
        fresh_opens: AtomicUsize::new(0),
        resumed_opens: AtomicUsize::new(0),
        next_turn: Box::leak(Box::new(AtomicI64::new(1))),
    }));
    let first = run_chat_with_adapter(
        ChatRequest {
            resident: "review".into(),
            model: "echo".into(),
            goal: Some("goal".into()),
            prompt: "first".into(),
        },
        adapter,
        &root,
        &db,
    )
    .unwrap();
    let second = run_chat_with_adapter(
        ChatRequest {
            resident: "review".into(),
            model: "echo".into(),
            goal: Some("ignored".into()),
            prompt: "second".into(),
        },
        adapter,
        &root,
        &db,
    )
    .unwrap();

    assert_eq!(
        serde_json::to_string(&[first, second]).unwrap(),
        r#"[{"reply_turn":2,"reply":"echo first"},{"reply_turn":3,"reply":"echo second"}]"#
    );
    assert_eq!(adapter.fresh_opens.load(Ordering::SeqCst), 1);
    assert_eq!(adapter.resumed_opens.load(Ordering::SeqCst), 1);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn failure_is_a_row() {
    let response = run_chat(ChatRequest {
        resident: "review".into(),
        model: "".into(),
        goal: None,
        prompt: "first".into(),
    });
    assert_eq!(
        serde_json::to_string(&response).unwrap(),
        r#"{"outcome":"failed","detail":"model `` names no harness"}"#
    );
    assert!(matches!(response, ChatResponse::Failed { .. }));
}

#[test]
fn command_failure_prints_a_row_and_exits_zero() {
    let mut child = Command::new(env!("CARGO_BIN_EXE_boop"))
        .args(["host", "chat"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(br#"{"resident":"review","model":"","prompt":"first"}"#)
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "{\"outcome\":\"failed\",\"detail\":\"model `` names no harness\"}\n"
    );
}
