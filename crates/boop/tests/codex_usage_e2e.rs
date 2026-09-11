//! A codex rollout carrying `token_usage_record` records must project them as
//! usage. Pre-fix, every sync-carrying `boop` verb warned
//! `projection_gap="token_usage_record"` and stored the record as a raw-JSON
//! turn instead of an `agent_usage` row.
//!
//! The rollouts here are a recorded transcript replay: the record shape is
//! copied from a real `~/.codex/sessions` rollout with the ids and paths
//! replaced, because llmock replay does not guarantee codex writes a
//! `token_usage_record`.

use std::path::Path;
use std::process::Command;

use boop_store::testing::BoopCommandExt;

const BOOP: &str = env!("CARGO_BIN_EXE_boop");

/// One codex rollout under the scratch `CODEX_HOME`, its first line a
/// `session_meta`, carrying one `token_usage_record` and no `token_count`.
fn write_session(home: &Path) -> String {
    let session = "01a09118-9f75-76d0-bde7-625fcc4329da";
    let dir = home.join(".codex/sessions/2026/09/11");
    std::fs::create_dir_all(&dir).unwrap();
    let body = format!(
        concat!(
            "{{\"timestamp\":\"2026-09-11T11:31:45.000Z\",\"ordinal\":1,\"type\":\"session_meta\",\"payload\":{{\"id\":\"{s}\",\"cwd\":\"/scrubbed/workspace\",\"agent_nickname\":\"Probe\",\"model_provider\":\"openai\"}}}}\n",
            "{{\"timestamp\":\"2026-09-11T11:31:45.100Z\",\"ordinal\":2,\"type\":\"turn_context\",\"payload\":{{\"cwd\":\"/scrubbed/workspace\",\"model\":\"gpt-5.6-luna\"}}}}\n",
            "{{\"timestamp\":\"2026-09-11T11:31:45.200Z\",\"ordinal\":3,\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"id\":\"msg_1\",\"role\":\"user\",\"content\":[{{\"type\":\"input_text\",\"text\":\"probe\"}}]}}}}\n",
            "{{\"timestamp\":\"2026-09-11T11:31:46.000Z\",\"ordinal\":4,\"type\":\"token_usage_record\",\"payload\":{{\"thread_id\":\"{s}\",\"turn_id\":\"{s}-turn\",\"session_id\":\"{s}\",\"usage\":{{\"input_tokens\":1000,\"cached_input_tokens\":400,\"cache_write_input_tokens\":100,\"output_tokens\":50,\"reasoning_output_tokens\":20,\"total_tokens\":1050}}}}}}\n",
        ),
        s = session,
    );
    let path = dir.join(format!("rollout-2026-09-11T11-31-45-{session}.jsonl"));
    std::fs::write(&path, body).unwrap();
    session.to_owned()
}

/// A `boop` call against the scratch reader home and store. The startup sync
/// runs unless `no_sync` asks for the read-only path.
fn boop(home: &Path, args: &[&str], no_sync: bool) -> std::process::Output {
    let mut command = Command::new(BOOP);
    command
        .args(args)
        .boop_test_root(home)
        .env("BOOP_DB", home.join("boop.db"));
    if no_sync {
        command.env("BOOP_NO_SYNC", "1");
    }
    command.output().expect("run boop")
}

fn ndjson_rows(output: &std::process::Output) -> Vec<serde_json::Value> {
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap_or_else(|e| panic!("parse {line:?}: {e}")))
        .collect()
}

/// RECEIPT. The record's counts land in `agent_usage` and no gap warning fires.
/// Sabotage: dropping the projection arm restores the raw-JSON turn and the
/// `projection_gap="token_usage_record"` warning.
#[test]
fn codex_token_usage_record_projects_without_a_gap_warning() {
    if boop::harness::mock_tui::resolve_llmock().is_none()
        || boop::harness::mock_tui::resolve_executable("codex", "CODEX_BIN").is_none()
    {
        eprintln!("skipping: no codex or llmock (CODEX_BIN / LLMOCK_BIN)");
        return;
    }
    let home = std::env::temp_dir().join(format!("boop-codex-usage-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&home);
    std::fs::create_dir_all(&home).unwrap();
    let _session = write_session(&home);

    let status = boop(&home, &["db", "status"], false);
    let stderr = String::from_utf8_lossy(&status.stderr);
    assert!(status.status.success(), "db status failed: {stderr}");
    assert!(
        !stderr.contains("projection_gap=\"token_usage_record\""),
        "token_usage_record still warns as an unprojected gap:\n{stderr}"
    );

    let usage = boop(
        &home,
        &[
            "db",
            "SELECT input_tokens, output_tokens, cache_create_5m_tokens, cache_read_tokens \
             FROM agent_usage",
            "--format",
            "ndjson",
        ],
        true,
    );
    let rows = ndjson_rows(&usage);
    assert_eq!(rows.len(), 1, "one usage row for the projected record");
    assert_eq!(
        rows[0]["input_tokens"], 500,
        "cached and cache-write excluded"
    );
    assert_eq!(rows[0]["output_tokens"], 50);
    assert_eq!(rows[0]["cache_create_5m_tokens"], 100);
    assert_eq!(rows[0]["cache_read_tokens"], 400);

    let raw = boop(
        &home,
        &[
            "db",
            "SELECT COUNT(*) AS n FROM agent_turn \
             WHERE said LIKE '%token_usage_record (unprojected)%'",
            "--format",
            "ndjson",
        ],
        true,
    );
    let raw = ndjson_rows(&raw);
    assert_eq!(
        raw[0]["n"], 0,
        "the record is never stored as a raw-JSON turn"
    );

    let _ = std::fs::remove_dir_all(&home);
}
