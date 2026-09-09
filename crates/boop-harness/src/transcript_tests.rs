use super::*;
use serde_json::json;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::harness::claude::{
    claude_project_dir, claude_session_path, content_has_tool_result, first_text, read_claude,
    Claude, INJECTED_TAGS,
};
use crate::harness::codex::Codex;
use crate::harness::kimi::Kimi;
use crate::harness::opencode::Opencode;
use crate::harness::{Harness, HarnessId, SessionRef};
use crate::live::{interactive_session_id, LiveSession, LiveSessionScope};
use rusqlite::Connection;

static SEQ: AtomicU64 = AtomicU64::new(0);
static TEST_SEQ: AtomicU64 = AtomicU64::new(0);

fn fixture_home() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "boop_transcript_{}_{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}

/// A session boop-harness would hand a caller, minted here so shaping is tested
/// without a fixture HOME the registry cannot be pointed at.
fn session_ref(harness: HarnessId, id: &str, path: PathBuf) -> SessionRef {
    SessionRef {
        harness,
        session_id: id.to_string(),
        nickname: id.to_string(),
        cwd: Some("/fixture".to_string()),
        git_branch: None,
        modified_ms: mtime(&path),
        size: 0,
        tmux: None,
        tmux_socket: None,
        parent: None,
        path,
    }
}

fn live_session(
    id: &str,
    started_ms: u64,
    scope: LiveSessionScope,
    parent_session: Option<&str>,
) -> LiveSession {
    LiveSession {
        harness: HarnessId::Codex,
        session_id: id.to_owned(),
        pid: None,
        cwd: Some("/fixture".into()),
        tmux_pane: None,
        status: crate::live::LiveStatus::Unknown,
        door: crate::live::DoorAddress::None,
        observed_ms: started_ms,
        started_ms: Some(started_ms),
        scope,
        parent_session: parent_session.map(str::to_owned),
    }
}

// One temp file per test, keyed by pid + a counter, so parallel `cargo
// test` runs never collide on the same path.
fn write_temp(lines: &[&str]) -> PathBuf {
    let n = TEST_SEQ.fetch_add(1, Ordering::SeqCst);
    let path =
        std::env::temp_dir().join(format!("transcript_test_{}_{n}.jsonl", std::process::id()));
    let mut file = fs::File::create(&path).expect("write temp fixture");
    for line in lines {
        writeln!(file, "{line}").expect("write temp fixture line");
    }
    path
}

// Captured from a real session by instant's `scripts/capture-transcripts.mjs`,
// trimmed and de-identified. Regenerate with that script when the harness
// wire format changes.
fn fixture(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("transcripts")
        .join(rel)
}

#[test]
fn an_existing_guardian_route_resolves_to_the_interactive_parent() {
    let parent = live_session("parent", 1_788_113_899_820, LiveSessionScope::Root, None);
    let guardian = live_session(
        "guardian",
        1_788_113_899_931,
        LiveSessionScope::Child,
        Some("parent"),
    );
    assert_eq!(
        interactive_session_id(&guardian, &[parent, guardian.clone()]),
        "parent"
    );
}

#[test]
fn an_unparented_child_never_infers_a_same_cwd_root() {
    let root = live_session("root", 1_788_113_899_820, LiveSessionScope::Root, None);
    let guardian = live_session("guardian", 1_788_113_899_931, LiveSessionScope::Child, None);
    assert_eq!(
        interactive_session_id(&guardian, &[root, guardian.clone()]),
        "guardian"
    );
}

#[test]
fn claude_messages_resolve_one_exact_file_without_session_discovery() {
    let home = fixture_home();
    let cwd = "/fixture";
    let project = claude_project_dir(&home, cwd);
    fs::create_dir_all(project.join("parent/subagents")).unwrap();
    fs::write(
        project.join("unrelated.jsonl"),
        "{ invalid and intentionally unreadable\n",
    )
    .unwrap();
    fs::write(
        project.join("wanted.jsonl"),
        r#"{"type":"user","uuid":"message-1","timestamp":"2026-07-20T10:00:10.000Z","promptSource":"typed","origin":{"kind":"human"},"message":{"role":"user","content":"direct"}}"#,
    )
    .unwrap();
    fs::write(
        project.join("parent/subagents/agent-1.jsonl"),
        r#"{"type":"user","uuid":"message-2","timestamp":"2026-07-20T10:00:11.000Z","promptSource":"typed","origin":{"kind":"human"},"message":{"role":"user","content":"subagent"}}"#,
    )
    .unwrap();

    let direct_path = claude_session_path(&home, cwd, "wanted").unwrap();
    let subagent_path = claude_session_path(&home, cwd, "agent-1").unwrap();
    let direct = read_claude(&direct_path, "wanted", None);
    let subagent = read_claude(&subagent_path, "agent-1", None);
    let receipt = json!({
        "direct": direct.iter().map(|message| (&message.id, &message.text)).collect::<Vec<_>>(),
        "missing": claude_session_path(&home, cwd, "absent"),
        "subagent": subagent.iter().map(|message| (&message.id, &message.text)).collect::<Vec<_>>(),
    });

    assert_eq!(
        serde_json::to_string_pretty(&receipt).unwrap(),
        r#"{
  "direct": [
    [
      "message-1",
      "direct"
    ]
  ],
  "missing": null,
  "subagent": [
    [
      "message-2",
      "subagent"
    ]
  ]
}"#
    );
}

#[test]
fn four_harnesses_lower_into_one_session_shape() {
    let home = fixture_home();

    let claude = home.join("claude-1.jsonl");
    fs::write(
        &claude,
        concat!(
            r#"{"cwd":"/fixture","timestamp":"2026-01-02T03:04:05Z"}"#,
            "\n",
            r#"{"message":{"usage":{"input_tokens":7,"cache_read_input_tokens":2,"cache_creation_input_tokens":1}}}"#,
            "\n"
        ),
    )
    .unwrap();

    let codex = home.join("rollout.jsonl");
    fs::write(
        &codex,
        concat!(
            r#"{"timestamp":"2026-01-02T03:04:05Z","type":"session_meta","payload":{"id":"codex-1","cwd":"/fixture","parent_thread_id":"codex-parent"}}"#,
            "\n",
            r#"{"type":"turn_context","payload":{"model":"gpt-fixture","model_provider":"openai"}}"#,
            "\n",
            r#"{"payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":21}}}}"#,
            "\n"
        ),
    )
    .unwrap();

    let kimi_session = home.join("kimi/session_kimi-1");
    fs::create_dir_all(kimi_session.join("agents/main")).unwrap();
    fs::write(kimi_session.join("state.json"), r#"{"workDir":"/fixture"}"#).unwrap();
    let kimi = kimi_session.join("agents/main/wire.jsonl");
    fs::write(&kimi, "").unwrap();

    let opencode = home.join("opencode.db");
    let db = Connection::open(&opencode).unwrap();
    db.execute_batch(
        "CREATE TABLE session (id TEXT, directory TEXT, title TEXT, time_created INTEGER, time_updated INTEGER, time_archived INTEGER);
         CREATE TABLE message (session_id TEXT, time_created INTEGER, data TEXT);
         INSERT INTO session VALUES ('opencode-1', '/fixture', 'OpenCode fixture', 100, 200, NULL);
         INSERT INTO message VALUES ('opencode-1', 200, '{\"tokens\":{\"input\":13},\"providerID\":\"openrouter\"}');",
    )
    .unwrap();
    drop(db);

    let mut codex_ref = session_ref(HarnessId::Codex, "codex-1", codex);
    codex_ref.parent = Some("codex-parent".to_string());
    let registry = crate::registry::Registry::discover();
    let summary: Vec<Value> = [
        session_ref(HarnessId::Claude, "claude-1", claude),
        session_ref(HarnessId::Opencode, "opencode-1", opencode),
        codex_ref,
        session_ref(HarnessId::Kimi, "kimi-1", kimi),
    ]
    .iter()
    .map(|session| registry.get(session.harness).describe(session).unwrap())
    .map(|session| {
        json!({
            "cwd": session.cwd,
            "harness": session.harness,
            "id": session.id,
            "inputTokens": session.input_tokens,
            "model": session.model,
            "parentId": session.parent_id,
            "parentKind": session.parent_kind,
            "provider": session.provider,
            "sourceFile": session.source_path.as_deref().and_then(|path| Path::new(path).file_name()).and_then(|name| name.to_str()),
            "title": session.title,
        })
    })
    .collect();

    assert_eq!(
        serde_json::to_string_pretty(&summary).unwrap(),
        r#"[
  {
    "cwd": "/fixture",
    "harness": "claude",
    "id": "claude-1",
    "inputTokens": 10,
    "model": null,
    "parentId": null,
    "parentKind": null,
    "provider": "anthropic",
    "sourceFile": "claude-1.jsonl",
    "title": null
  },
  {
    "cwd": "/fixture",
    "harness": "opencode",
    "id": "opencode-1",
    "inputTokens": 13,
    "model": null,
    "parentId": null,
    "parentKind": null,
    "provider": "openrouter",
    "sourceFile": null,
    "title": "OpenCode fixture"
  },
  {
    "cwd": "/fixture",
    "harness": "codex",
    "id": "codex-1",
    "inputTokens": 21,
    "model": "gpt-fixture",
    "parentId": "codex-parent",
    "parentKind": "subagent",
    "provider": "openai",
    "sourceFile": "rollout.jsonl",
    "title": null
  },
  {
    "cwd": "/fixture",
    "harness": "kimi",
    "id": "kimi-1",
    "inputTokens": null,
    "model": null,
    "parentId": null,
    "parentKind": null,
    "provider": null,
    "sourceFile": "wire.jsonl",
    "title": null
  }
]"#
    );
}

#[test]
fn opencode_tokens_take_max_not_latest() {
    // The newest assistant turn carries tokens.input 0; MAX across the session
    // is the live context reading, not the trailing zero.
    let home = fixture_home();
    let path = home.join("opencode.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE session (id TEXT, directory TEXT, title TEXT, time_created INTEGER, time_updated INTEGER, time_archived INTEGER);
         CREATE TABLE message (session_id TEXT, time_created INTEGER, data TEXT);
         INSERT INTO session VALUES ('oc-1', '/fixture', NULL, 100, 300, NULL);
         INSERT INTO message VALUES ('oc-1', 100, '{\"tokens\":{\"input\":50},\"modelID\":\"a-model\",\"providerID\":\"provider-a\"}');
         INSERT INTO message VALUES ('oc-1', 200, '{\"tokens\":{\"input\":0}}');",
    )
    .unwrap();
    drop(db);
    let session = Opencode
        .describe(&session_ref(HarnessId::Opencode, "oc-1", path))
        .unwrap();
    assert_eq!(session.id, "oc-1");
    assert_eq!(session.input_tokens, Some(50));
    assert_eq!(session.model.as_deref(), Some("a-model"));
    assert_eq!(session.provider.as_deref(), Some("provider-a"));
}

#[test]
fn an_archived_opencode_session_is_not_a_row() {
    let home = fixture_home();
    let path = home.join("opencode.db");
    let db = Connection::open(&path).unwrap();
    db.execute_batch(
        "CREATE TABLE session (id TEXT, directory TEXT, title TEXT, time_created INTEGER, time_updated INTEGER, time_archived INTEGER);
         CREATE TABLE message (session_id TEXT, time_created INTEGER, data TEXT);
         INSERT INTO session VALUES ('oc-gone', '/fixture', NULL, 100, 300, 400);",
    )
    .unwrap();
    drop(db);
    assert!(Opencode
        .describe(&session_ref(HarnessId::Opencode, "oc-gone", path))
        .is_none());
}

#[test]
fn kimi_wire_usage_sums_inputs() {
    // wire.jsonl carries per-turn usage; input = inputOther + cache read + cache
    // creation, taken from the last line that has it, with the model beside it.
    let home = fixture_home();
    let dir = home.join("session_kimi-2");
    fs::create_dir_all(dir.join("agents/main")).unwrap();
    fs::write(dir.join("state.json"), r#"{"workDir":"/fixture"}"#).unwrap();
    let wire = dir.join("agents/main/wire.jsonl");
    fs::write(
        &wire,
        concat!(
            r#"{"type":"assistant","model":"kimi-code/k3","usage":{"inputOther":10,"output":1,"inputCacheRead":100,"inputCacheCreation":5},"usageScope":"turn"}"#,
            "\n",
            r#"{"type":"assistant","model":"kimi-code/k3","usage":{"inputOther":15,"output":2,"inputCacheRead":200,"inputCacheCreation":0},"usageScope":"turn"}"#,
            "\n",
        ),
    )
    .unwrap();
    let session = Kimi
        .describe(&session_ref(HarnessId::Kimi, "kimi-2", wire))
        .unwrap();
    assert_eq!(session.id, "kimi-2");
    assert_eq!(session.input_tokens, Some(215));
    assert_eq!(session.model.as_deref(), Some("kimi-code/k3"));
}

/// RECEIPT. A claude subagent transcript resumes on its file stem, while every
/// other harness resumes on the id it published, so `--resume` gets the id the
/// harness answers to.
#[test]
fn a_resume_id_is_the_stem_for_claude_and_the_session_id_elsewhere() {
    let mut claude = session_ref(HarnessId::Claude, "parent/agent-9", PathBuf::new());
    claude.nickname = "agent-9".to_string();
    claude.parent = Some("parent".to_string());
    assert_eq!(Claude.resume_id(&claude), "agent-9");

    let mut codex = session_ref(HarnessId::Codex, "thread-9", PathBuf::new());
    codex.nickname = "rollout-2026".to_string();
    assert_eq!(Codex.resume_id(&codex), "thread-9");
}

#[test]
fn title_lookup_skips_leading_tool_and_meta_rows() {
    // Mirrors a title lookup's `.find(|m| m.role == "user")`: a
    // session whose first lines are tool output / injected content must
    // still resolve its title to the first real typed message.
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u1","timestamp":"2026-07-20T10:00:00.000Z","isMeta":false,"toolUseResult":{"stdout":"ok"},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":"ok"}]}}"#,
        r#"{"type":"user","uuid":"u2","timestamp":"2026-07-20T10:00:01.000Z","isMeta":true,"message":{"role":"user","content":"injected skill body"}}"#,
        r#"{"type":"user","uuid":"u3","timestamp":"2026-07-20T10:00:02.000Z","isMeta":false,"message":{"role":"user","content":"fix the off-by-one bug"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    let title = out
        .into_iter()
        .find(|m| m.role == "user")
        .map(|m| m.preview);
    assert_eq!(title.as_deref(), Some("fix the off-by-one bug"));
}

#[test]
fn string_content_is_a_user_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u1","timestamp":"2026-07-20T10:00:00.000Z","isMeta":false,"message":{"role":"user","content":"hello there"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "user");
    assert_eq!(out[0].subtype, None);
}

#[test]
fn tool_result_block_is_a_tool_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u2","timestamp":"2026-07-20T10:00:01.000Z","isMeta":false,"toolUseResult":{"stdout":"ok"},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":"ok"}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "tool");
    assert_eq!(out[0].subtype.as_deref(), Some("tool_result"));
}

#[test]
fn meta_string_content_is_a_meta_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u3","timestamp":"2026-07-20T10:00:02.000Z","isMeta":true,"message":{"role":"user","content":"<command-name>/compact</command-name> body"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "meta");
    assert_eq!(out[0].subtype.as_deref(), Some("command-name"));
}

#[test]
fn task_notification_is_a_meta_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u6","timestamp":"2026-07-20T10:00:06.000Z","promptSource":"system","origin":{"kind":"task-notification"},"message":{"role":"user","content":"<task-notification>\n<task-id>a06c1e6</task-id>\n<status>completed</status>\n</task-notification>"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "meta");
    assert_eq!(out[0].subtype.as_deref(), Some("task-notification"));
}

#[test]
fn compact_summary_is_a_meta_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u7","timestamp":"2026-07-20T10:00:07.000Z","isCompactSummary":true,"isVisibleInTranscriptOnly":true,"message":{"role":"user","content":"This session is being continued from a previous conversation…"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "meta");
    assert_eq!(out[0].subtype.as_deref(), Some("compact-summary"));
}

// Sessions written before `promptSource` existed carry no flag at all, so
// the wrapper tag is the only marker left.
#[test]
fn legacy_command_body_is_a_meta_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u8","timestamp":"2026-07-20T10:00:08.000Z","message":{"role":"user","content":[{"type":"text","text":"<command-name>/compact</command-name>\n<command-message>compact</command-message>"}]}}"#,
        r#"{"type":"user","uuid":"u9","timestamp":"2026-07-20T10:00:09.000Z","message":{"role":"user","content":[{"type":"text","text":"<local-command-stdout>Compacted</local-command-stdout>"}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].role, "meta");
    assert_eq!(out[0].subtype.as_deref(), Some("command-name"));
    assert_eq!(out[1].role, "meta");
    assert_eq!(out[1].subtype.as_deref(), Some("local-command-stdout"));
}

#[test]
fn typed_and_queued_input_stay_user_rows() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"ua","timestamp":"2026-07-20T10:00:10.000Z","promptSource":"typed","origin":{"kind":"human"},"message":{"role":"user","content":"ship it"}}"#,
        r#"{"type":"user","uuid":"ub","timestamp":"2026-07-20T10:00:11.000Z","promptSource":"queued","origin":{"kind":"human"},"message":{"role":"user","content":"and then this"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 2);
    let roles: Vec<&str> = out.iter().map(|m| m.role.as_str()).collect();
    assert_eq!(roles, ["user", "user"]);
}

// A message that merely mentions a tag is not an injection: the wrapper has
// to open the message.
#[test]
fn prose_about_a_tag_stays_a_user_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"uc","timestamp":"2026-07-20T10:00:12.000Z","message":{"role":"user","content":"why does <command-name> show up in the sidebar"}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "user");
}

#[test]
fn meta_text_block_is_a_meta_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u4","timestamp":"2026-07-20T10:00:03.000Z","isMeta":true,"message":{"role":"user","content":[{"type":"text","text":"injected content"}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "meta");
    assert_eq!(out[0].subtype, None);
}

// Recorded on purpose as a user row: it's something the user actually did.
#[test]
fn interrupted_message_stays_a_user_row() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u5","timestamp":"2026-07-20T10:00:04.000Z","isMeta":false,"message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "user");
    assert_eq!(out[0].subtype, None);
}

#[test]
fn assistant_line_is_unaffected() {
    let path = write_temp(&[
        r#"{"type":"assistant","uuid":"a1","timestamp":"2026-07-20T10:00:05.000Z","message":{"role":"assistant","content":[{"type":"text","text":"here is my answer"}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].role, "assistant");
    assert_eq!(out[0].subtype, None);
}

#[test]
fn assistant_d2_write_retains_the_complete_fenced_file() {
    let path = write_temp(&[
        r#"{"type":"assistant","uuid":"a2","timestamp":"2026-07-20T10:00:06.000Z","message":{"role":"assistant","content":[{"type":"tool_use","name":"Write","input":{"file_path":"/tmp/large.d2","content":"direction: right\na -> b\nb -> c"}}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 1);
    assert_eq!(
        out[0].text,
        "[Write] /tmp/large.d2\n```d2\ndirection: right\na -> b\nb -> c\n```"
    );
}

#[test]
fn seq_is_the_line_index_across_skipped_lines() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u1","timestamp":"2026-07-20T10:00:00.000Z","isMeta":false,"message":{"role":"user","content":"first"}}"#,
        r#"{"type":"system","content":"mode change"}"#,
        r#"{"type":"user","uuid":"u2","timestamp":"2026-07-20T10:00:01.000Z","isMeta":false,"toolUseResult":{"stdout":"ok"},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":"ok"}]}}"#,
        r#"{"type":"assistant","uuid":"a1","timestamp":"2026-07-20T10:00:02.000Z","message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}"#,
    ]);
    let out = read_claude(&path, "s", None);
    assert_eq!(out.len(), 3);
    assert_eq!(out[0].seq, 0);
    assert_eq!(out[1].seq, 2);
    assert_eq!(out[2].seq, 3);
}

#[test]
fn after_seq_skips_up_to_and_including_that_index() {
    let path = write_temp(&[
        r#"{"type":"user","uuid":"u1","timestamp":"2026-07-20T10:00:00.000Z","isMeta":false,"message":{"role":"user","content":"first"}}"#,
        r#"{"type":"system","content":"mode change"}"#,
        r#"{"type":"user","uuid":"u2","timestamp":"2026-07-20T10:00:01.000Z","isMeta":false,"toolUseResult":{"stdout":"ok"},"message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_x","content":"ok"}]}}"#,
        r#"{"type":"assistant","uuid":"a1","timestamp":"2026-07-20T10:00:02.000Z","message":{"role":"assistant","content":[{"type":"text","text":"answer"}]}}"#,
    ]);
    let out = read_claude(&path, "s", Some(0));
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].seq, 2);
    assert_eq!(out[1].seq, 3);
}

#[test]
fn captured_claude_session_labels_every_row_by_its_wire_shape() {
    let path = fixture("claude/session.jsonl");
    let raw = fs::read_to_string(&path).expect("captured fixture missing");
    let out = read_claude(&path, "s", None);
    let (mut tool, mut user, mut meta) = (0, 0, 0);
    for (i, line) in raw.lines().enumerate() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v.get("type").and_then(|t| t.as_str()) != Some("user") {
            continue;
        }
        let Some(row) = out.iter().find(|m| m.seq == i as u64) else {
            continue;
        };
        let content = v
            .get("message")
            .and_then(|m| m.get("content"))
            .cloned()
            .unwrap_or(Value::Null);
        let tagged = first_text(&content).is_some_and(|t| {
            let t = t.trim_start();
            INJECTED_TAGS
                .iter()
                .any(|tag| t.starts_with(&format!("<{tag}>")))
        });
        let injected = tagged
            || v.get("isMeta").and_then(|m| m.as_bool()).unwrap_or(false)
            || v.get("isCompactSummary")
                .and_then(|m| m.as_bool())
                .unwrap_or(false)
            || v.get("promptSource").and_then(|p| p.as_str()) == Some("system");
        if content_has_tool_result(&content) {
            assert_eq!(row.role, "tool", "line {} carries tool output", i + 1);
            tool += 1;
        } else if injected {
            assert_eq!(row.role, "meta", "line {} is injected", i + 1);
            meta += 1;
        } else {
            assert_eq!(row.role, "user", "line {} was typed by the user", i + 1);
            user += 1;
        }
    }
    assert!(tool > 0, "fixture lost its tool rows");
    assert!(user > 0, "fixture lost its user rows");
    assert!(meta > 0, "fixture lost its meta rows");
}

#[test]
fn captured_claude_subagent_session_parses() {
    let path = fixture("claude/subagent.jsonl");
    let out = read_claude(&path, "s", None);
    assert!(!out.is_empty(), "subagent fixture produced no rows");
    assert!(
        out.iter().any(|m| m.role == "assistant"),
        "subagent fixture has no assistant rows"
    );
    let raw = fs::read_to_string(&path).expect("captured fixture missing");
    assert!(
        raw.lines().any(|l| l.contains(r#""isSidechain":true"#)),
        "subagent fixture must keep its sidechain marker"
    );
}

#[test]
fn wire_shapes_pin_the_instant_key_set() {
    let message = Message {
        harness: HarnessId::Claude,
        session_id: "s".to_owned(),
        id: "id".to_owned(),
        seq: 0,
        role: "user".to_owned(),
        subtype: None,
        ts: 1,
        preview: "p".to_owned(),
        text: "t".to_owned(),
        locator: "claude:path#L1".to_owned(),
    };
    let mut message_keys: Vec<String> = serde_json::to_value(&message)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    message_keys.sort();
    assert_eq!(
        message_keys,
        [
            "editor",
            "id",
            "locator",
            "preview",
            "role",
            "seq",
            "session_id",
            "text",
            "ts",
        ]
        .map(str::to_owned)
        .to_vec()
    );

    let meta = SessionMeta {
        id: "s".to_owned(),
        harness: HarnessId::Claude,
        cwd: "/fixture".to_owned(),
        source_path: Some("path".to_owned()),
        title: None,
        model: None,
        provider: None,
        input_tokens: Some(7),
        parent_id: None,
        parent_kind: None,
        created_at_ms: 1,
        last_activity_ms: 2,
    };
    let mut meta_keys: Vec<String> = serde_json::to_value(&meta)
        .unwrap()
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    meta_keys.sort();
    assert_eq!(
        meta_keys,
        [
            "createdAtMs",
            "cwd",
            "harness",
            "id",
            "inputTokens",
            "lastActivityMs",
            "model",
            "parentId",
            "parentKind",
            "provider",
            "sourcePath",
            "title",
        ]
        .map(str::to_owned)
        .to_vec()
    );
}
