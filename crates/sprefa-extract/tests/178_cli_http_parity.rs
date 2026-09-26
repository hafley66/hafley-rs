#![cfg(feature = "cli")]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[path = "support/0_daemon_guard.rs"]
mod daemon_guard;
use daemon_guard::DaemonGuard;

use axum::body::Body;
use base64::Engine as _;
use axum::http::{header, Request};
use http_body_util::BodyExt;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde_json::json;
use sha2::{Digest, Sha256};

struct Case {
    op: &'static str,
    argv: Vec<String>,
    args: serde_json::Value,
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("fixture directory");
    for entry in std::fs::read_dir(source).expect("fixture entries") {
        let entry = entry.expect("fixture entry");
        let dest = target.join(entry.file_name());
        if entry.file_type().expect("fixture type").is_dir() {
            copy_tree(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), dest).expect("copy fixture file");
        }
    }
}

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_DATE", "2000-01-01T00:00:00Z")
        .env("GIT_COMMITTER_DATE", "2000-01-01T00:00:00Z")
        .output()
        .expect("git fixture setup");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

fn stable(op: &str, body: &[u8], root: &Path, scratch: &Path) -> Vec<u8> {
    // Path-derived worktree IDs, index mtimes, and dry-run stage IDs vary across
    // temporary copies; normalize those fields before the fixed SHA table.
    let mut text = String::from_utf8(body.to_vec()).expect("UTF-8 transport body");
    for (path, label) in [(root, "<ROOT>"), (scratch, "<TMP>")] {
        let canonical = path.canonicalize().expect("canonical fixture path");
        text = text.replace(&canonical.to_string_lossy().to_string(), label);
        text = text.replace(&path.to_string_lossy().to_string(), label);
    }
    let mtime = regex::Regex::new(r#""index_mtime_unix_ms":\d+"#).unwrap();
    text = mtime.replace_all(&text, "\"index_mtime_unix_ms\":0").into_owned();
    let stage = regex::Regex::new(r"stage [0-9a-f]{64}").unwrap();
    text = stage.replace_all(&text, "stage <STAGE>").into_owned();
    for field in ["repository", "worktree"] {
        let identity = regex::Regex::new(&format!(r#""{field}":"[0-9a-f]{{64}}""#)).unwrap();
        text = identity.replace_all(&text, format!(r#""{field}":"<REPO>""#)).into_owned();
    }
    if matches!(op, "cleave" | "move" | "rename" | "region" | "ingest" | "schema" | "trail") {
        text.truncate(text.trim_end_matches('\n').len());
    }
    text.into_bytes()
}

fn row(op: &str, transport: &str, body: &[u8], root: &Path, scratch: &Path) -> String {
    let body = stable(op, body, root, scratch);
    let sha = format!("{:x}", Sha256::digest(&body));
    format!("{op:<7} {transport:<6} {:>4} {}", body.split(|byte| *byte == b'\n').filter(|line| !line.is_empty()).count(), &sha[..12])
}

fn cases(root: &Path, scratch: &Path) -> Vec<Case> {
    let root = root.to_string_lossy().to_string();
    let src = format!("{root}/src");
    let index = format!("{root}/index.scip");
    let one = format!("{src}/_1_none.rs");
    let state = format!("{}/state", scratch.display());
    let receipts = format!("{}/receipts.db", scratch.display());
    let region = format!("{root}/region.rs");
    let generated = format!("{root}/region.txt");
    let tsi = format!("{}/tests/fixtures/tsi/foreign_probe.jsonl", env!("CARGO_MANIFEST_DIR"));
    vec![
        Case { op: "fast", argv: vec!["fast".into(), src.clone()], args: json!({"paths": [src]}) },
        Case { op: "slow", argv: vec!["slow".into(), src.clone(), "--root".into(), root.clone(), "--scip-index".into(), index.clone(), "--no-checker".into()], args: json!({"paths": [src], "root": root, "scip_index": index, "no_checker": true}) },
        Case { op: "scip", argv: vec!["scip".into(), root.clone(), "--scip-index".into(), index.clone()], args: json!({"paths": [root], "scip_index": index}) },
        Case { op: "graph", argv: vec!["graph".into(), "--uses".into(), "A".into(), "--root".into(), root.clone(), src.clone()], args: json!({"paths": [src], "root": root, "uses": "A"}) },
        Case { op: "cleave", argv: vec!["cleave".into(), format!("{src}/_2_one.rs#OneField"), format!("{src}/_6_cleave.rs"), "--root".into(), root.clone(), "--state".into(), state.clone()], args: json!({"target": format!("{src}/_2_one.rs#OneField"), "dest": format!("{src}/_6_cleave.rs"), "root": root, "state": state}) },
        Case { op: "move", argv: vec!["move".into(), one.clone(), format!("{src}/_5_moved.rs"), "--root".into(), root.clone(), "--state".into(), state.clone()], args: json!({"old": one, "new": format!("{src}/_5_moved.rs"), "root": [root], "state": state}) },
        Case { op: "rename", argv: vec!["rename".into(), format!("{src}/_0_types.rs#A"), "AZ".into(), "--root".into(), root.clone(), "--state".into(), state.clone()], args: json!({"target": format!("{src}/_0_types.rs#A"), "new": "AZ", "root": root, "state": state}) },
        Case { op: "query", argv: vec!["query".into(), one.clone(), "--query".into(), "(function_item name: (identifier) @name)".into()], args: json!({"paths": [one], "query": "(function_item name: (identifier) @name)"}) },
        Case { op: "region", argv: vec!["region".into(), region.clone(), "demo".into(), "--generated".into(), generated.clone()], args: json!({"target": region, "id": "demo", "generated": generated}) },
        Case { op: "watch", argv: vec!["watch".into(), "--root".into(), root.clone(), "--once".into(), "--receipts".into(), receipts.clone()], args: json!({"root": root, "once": true, "receipts": receipts, "poll_ms": 500}) },
        Case { op: "diff", argv: vec!["diff".into(), "--root".into(), root.clone(), "--from".into(), "HEAD".into(), "--to".into(), "HEAD".into()], args: json!({"root": root, "from": "HEAD", "to": "HEAD"}) },
        Case { op: "ingest", argv: vec!["ingest".into(), tsi.clone()], args: json!({"paths": [tsi]}) },
        Case { op: "schema", argv: vec!["schema".into()], args: json!({}) },
        Case { op: "trail", argv: vec!["trail".into()], args: json!({"runs": 5}) },
    ]
}

fn start_server(binary: &Path, cache: &Path) -> (DaemonGuard, PathBuf) {
    let guard = DaemonGuard::new(cache);
    let socket = guard.socket().to_path_buf();
    let status = Command::new(binary).arg("--daemon").env("XDG_CACHE_HOME", cache)
        .env("RYI_IDLE_SECS", "30")
        .env("HOME", cache.parent().expect("cache parent").join("home"))
        .status().expect("start daemon");
    assert!(status.success(), "daemonize");
    for _ in 0..200 {
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() && guard.pid().is_some() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(socket.exists(), "unix socket bound");
    assert!(guard.pid().is_some(), "daemon PID recorded");
    (guard, socket)
}

async fn socket_response(socket: &Path, op: &str, args: &serde_json::Value, root: &Path) -> (axum::http::StatusCode, axum::body::Bytes) {
    let stream = tokio::net::UnixStream::connect(socket).await.expect("connect unix socket");
    let (mut client, connection) = http1::handshake(TokioIo::new(stream)).await.expect("HTTP handshake");
    tokio::spawn(async move { let _ = connection.await; });
    let envelope = json!({"request_root": root, "args": args}).to_string();
    let mut request = if op == "schema" { Request::get(format!("http://localhost/{op}")) } else { Request::post(format!("http://localhost/{op}")) };
    let body = if op == "ingest" {
        request = request.header("x-ryi-request", base64::engine::general_purpose::STANDARD.encode(envelope));
        Body::empty()
    } else {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(envelope)
    };
    let request = request.body(body).expect("socket request");
    let response = client.send_request(request).await.expect("socket response");
    let status = response.status();
    let body = response.into_body().collect().await.expect("socket body").to_bytes();
    (status, body)
}

async fn socket_request(socket: &Path, op: &str, args: &serde_json::Value, root: &Path) -> axum::body::Bytes {
    let (status, body) = socket_response(socket, op, args, root).await;
    assert!(status.is_success(), "{op} socket HTTP: {status}: {}", String::from_utf8_lossy(&body));
    body
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cli_router_and_unix_socket_share_the_contract() {
    let scratch = tempfile::tempdir().expect("parity scratch");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/type_ladder");
    let root = scratch.path().join("type_ladder");
    copy_tree(&fixture, &root);
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(&root, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "base"]);
    std::fs::write(root.join("region.rs"), "// sprefa:auto-begin demo\nold\n// sprefa:auto-end demo\n").unwrap();
    std::fs::write(root.join("region.txt"), "old\n").unwrap();
    let original_bin = PathBuf::from(env!("CARGO_BIN_EXE_ryi-server"));
    let binary = scratch.path().join("ryi-server");
    std::fs::copy(original_bin, &binary).expect("pin ryi for parity run");

    let mut table = Vec::new();
    let mut fast_cli = Vec::new();
    let cases = cases(&root, scratch.path());
    let (_server, socket) = start_server(&binary, &scratch.path().join("cache"));
    for case in &cases {
        let cli = Command::new(&binary)
            .args(&case.argv)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .env("RUST_LOG", "off").env("DL_TRAIL", "0")
            .env("HOME", scratch.path().join("home"))
            .output().expect("CLI transport");
        assert!(cli.status.success(), "{} CLI: {}", case.op, String::from_utf8_lossy(&cli.stderr));
        if case.op == "fast" { fast_cli = cli.stdout.clone(); }
        table.push(row(case.op, "cli", &cli.stdout, &root, scratch.path()));
        if matches!(case.op, "cleave" | "move" | "rename") {
            let state = scratch.path().join("state");
            if state.exists() {
                std::fs::remove_dir_all(state).expect("reset dry-run stage for HTTP transport");
            }
        }
        let body = socket_request(&socket, case.op, &case.args, Path::new(env!("CARGO_MANIFEST_DIR"))).await;
        table.push(row(case.op, "router", &body, &root, scratch.path()));
    }

    let fast = &cases[0];
    let socket_body = socket_request(&socket, fast.op, &fast.args, Path::new(env!("CARGO_MANIFEST_DIR"))).await;
    table.push(row("fast", "socket", &socket_body, &root, scratch.path()));

    let proof_cache = scratch.path().join("proof-cache");
    let system_path = "/usr/bin:/bin:/opt/homebrew/bin";
    assert!(system_path.split(':').all(|dir| !Path::new(dir).join("ryi-server").exists()));
    let (_proof_server, proof_socket) = start_server(&binary, &proof_cache);
    std::fs::remove_file(&binary).expect("remove server executable after launch");
    let proof_body = socket_request(&proof_socket, fast.op, &fast.args, Path::new(env!("CARGO_MANIFEST_DIR"))).await;

    let sqlite = scratch.path().join("fast-http.db");
    let sqlite_arg = sqlite.to_string_lossy().to_string();
    let (sqlite_status, sqlite_body) = socket_response(
        &socket,
        "fast",
        &json!({"paths": fast.args["paths"], "sqlite": sqlite_arg}),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    ).await;
    assert_eq!(sqlite_status, axum::http::StatusCode::OK);
    assert!(sqlite.exists(), "HTTP fast published SQLite");
    assert!(sqlite_body.starts_with(b"Wrote "));

    let rendered = table.join("\n");
    println!("{rendered}");
    assert_eq!(rendered, include_str!("fixtures/ryi_http_parity.tsv").trim_end());
    let no_child = row("fast", "nochild", &proof_body, &root, scratch.path());
    println!("{no_child}");
    assert_eq!(no_child, row("fast", "nochild", &fast_cli, &root, scratch.path()), "proof body: {}", String::from_utf8_lossy(&proof_body));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_error_has_http_status_and_server_accepts_next_request() {
    let cli = Command::new(env!("CARGO_BIN_EXE_ryi-server"))
        .args(["scip", "--indexer", "not-a-language"])
        .env("DL_TRAIL", "0")
        .output().expect("CLI error");
    assert_eq!(cli.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&cli.stderr).contains("unknown language"));

    let scratch = tempfile::tempdir().expect("error scratch");
    let (_server, socket) = start_server(Path::new(env!("CARGO_BIN_EXE_ryi-server")), &scratch.path().join("cache"));

    let (status, body) = socket_response(
        &socket,
        "scip",
        &json!({"indexer": "not-a-language"}),
        Path::new(env!("CARGO_MANIFEST_DIR")),
    ).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    let rows: Vec<serde_json::Value> = body.split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("JSON error row"))
        .collect();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["code"], 2);
    assert!(rows[0]["error"].as_str().unwrap().contains("unknown language"));

    let first = scratch.path().join("first.rs");
    let unsupported = scratch.path().join("second.txt");
    std::fs::write(&first, "fn first() {}\n").expect("first query input");
    std::fs::write(&unsupported, "second\n").expect("unsupported query input");
    let query_args = json!({"paths": [first, unsupported], "query": "(function_item name: (identifier) @name)"});
    let (late_status, late_body) = socket_response(&socket, "query", &query_args, Path::new(env!("CARGO_MANIFEST_DIR"))).await;
    assert_eq!(late_status, axum::http::StatusCode::OK);
    let late_rows: Vec<serde_json::Value> = late_body.split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("late JSONL row"))
        .collect();
    assert_eq!(late_rows.len(), 2);
    assert_eq!(late_rows[0]["name"], "first");
    assert_eq!(late_rows[1]["code"], 2);
    assert!(late_rows[1]["error"].as_str().unwrap().contains("no language"));

    let (next_status, next_body) = socket_response(&socket, "schema", &json!({}), Path::new(env!("CARGO_MANIFEST_DIR"))).await;
    assert_eq!(next_status, axum::http::StatusCode::OK);
    assert!(next_body.starts_with(b"sprefa-extract JSONL contract:"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persistent_watch_allows_other_ops_and_disconnects() {
    let scratch = tempfile::tempdir().expect("watch scratch");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/type_ladder");
    let root = scratch.path().join("type_ladder");
    copy_tree(&fixture, &root);
    git(&root, &["init", "-q", "."]);
    git(&root, &["add", "-A"]);
    git(&root, &["-c", "user.email=t@t", "-c", "user.name=t", "commit", "-qm", "base"]);
    let receipts = scratch.path().join("watch.db");
    let (_server, socket) = start_server(Path::new(env!("CARGO_BIN_EXE_ryi-server")), &scratch.path().join("cache"));

    let stream = tokio::net::UnixStream::connect(&socket).await.expect("watch connect");
    let (mut client, connection) = http1::handshake(TokioIo::new(stream)).await.expect("watch handshake");
    tokio::spawn(async move { let _ = connection.await; });
    let request = Request::post("http://localhost/watch")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(json!({"request_root": env!("CARGO_MANIFEST_DIR"), "args": {"root": root, "receipts": receipts, "poll_ms": 50, "once": false}}).to_string())).expect("watch request");
    let mut watch = tokio::time::timeout(Duration::from_secs(15), client.send_request(request))
        .await.expect("watch response arrived").expect("watch response");
    assert_eq!(watch.status(), axum::http::StatusCode::OK);
    let first = tokio::time::timeout(Duration::from_secs(15), watch.body_mut().frame())
        .await.expect("watch first row arrived").expect("watch first frame").expect("watch frame");
    assert!(!first.into_data().expect("watch data").is_empty());

    let one = root.join("src/_1_none.rs");
    let query_args = json!({"paths": [one], "query": "(function_item name: (identifier) @name)"});
    let (status, body) = tokio::time::timeout(Duration::from_secs(15), socket_response(&socket, "query", &query_args, Path::new(env!("CARGO_MANIFEST_DIR"))))
        .await.expect("query while watch is open");
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(!body.is_empty());

    let old = one.to_string_lossy().to_string();
    let new = root.join("src/_5_moved.rs").to_string_lossy().to_string();
    let state = scratch.path().join("state").to_string_lossy().to_string();
    let move_args = json!({"old": old, "new": new, "root": [root], "state": state});
    let (move_status, move_body) = tokio::time::timeout(Duration::from_secs(15), socket_response(&socket, "move", &move_args, Path::new(env!("CARGO_MANIFEST_DIR"))))
        .await.expect("move while watch is open");
    assert_eq!(move_status, axum::http::StatusCode::OK);
    let move_text = String::from_utf8(move_body.to_vec()).expect("move response text");
    assert!(move_text.contains("plan "));

    drop(watch);
    drop(client);
    let (next_status, _) = tokio::time::timeout(Duration::from_secs(15), socket_response(&socket, "schema", &json!({}), Path::new(env!("CARGO_MANIFEST_DIR"))))
        .await.expect("request after watch disconnect");
    assert_eq!(next_status, axum::http::StatusCode::OK);
}

#[test]
fn fast_jsonl_uses_bounded_sorted_path_over_large_roster() {
    let scratch = tempfile::tempdir().expect("fast scratch");
    for index in 0..4097 {
        std::fs::write(scratch.path().join(format!("{index:04}_fact.rs")), format!("pub fn f_{index}() {{}}\n"))
            .expect("fast fixture file");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_ryi-server"))
        .arg("fast").arg(scratch.path())
        .env("DL_TRACE_SUMMARY", "1")
        .env("DL_TRAIL", "0")
        .env("RYI_MAX_MEM_MB", "1024")
        .output().expect("bounded fast run");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<&[u8]> = output.stdout.split(|byte| *byte == b'\n').filter(|row| !row.is_empty()).collect();
    assert!(!rows.is_empty());
    assert!(rows.windows(2).all(|pair| pair[0] <= pair[1]), "fast JSONL order");
    assert!(!String::from_utf8_lossy(&output.stderr).contains("sorted_lines"), "fast retained corpus sort");
}
