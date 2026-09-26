#![cfg(feature = "cli")]
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request};
use http_body_util::BodyExt;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde_json::json;
use sha2::{Digest, Sha256};

struct Case {
    op: &'static str,
    argv: Vec<String>,
    uri: String,
    body: String,
}

fn query(path: &str, pairs: &[(&str, &str)]) -> String {
    if pairs.is_empty() { return path.to_string(); }
    let encoded = url::form_urlencoded::Serializer::new(String::new()).extend_pairs(pairs.iter().copied()).finish();
    format!("{path}?{encoded}")
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
    let input = |paths: Vec<&str>| json!({"paths": paths, "patterns": [], "entry": []}).to_string();
    let region = format!("{root}/region.rs");
    let generated = format!("{root}/region.txt");
    let mut url = url::Url::parse("http://localhost/region").unwrap();
    url.path_segments_mut().unwrap().push(&region).push("demo");
    let region_path = url.path().to_string();
    let tsi = "tests/fixtures/tsi/foreign_probe.jsonl";
    vec![
        Case { op: "fast", argv: vec!["fast".into(), src.clone()], uri: "/fast".into(), body: input(vec![&src]) },
        Case { op: "slow", argv: vec!["slow".into(), src.clone(), "--root".into(), root.clone(), "--scip-index".into(), index.clone(), "--no-checker".into()], uri: query("/slow", &[("scip_index", &index), ("no_checker", "true")]), body: json!({"paths": [src], "patterns": [], "entry": [], "root": root}).to_string() },
        Case { op: "scip", argv: vec!["scip".into(), root.clone(), "--scip-index".into(), index.clone()], uri: query("/scip", &[("scip_index", &index)]), body: input(vec![&root]) },
        Case { op: "graph", argv: vec!["graph".into(), "--uses".into(), "A".into(), "--root".into(), root.clone(), src.clone()], uri: query("/graph", &[("uses", "A")]), body: json!({"paths": [src], "patterns": [], "entry": [], "root": root}).to_string() },
        Case { op: "cleave", argv: vec!["cleave".into(), format!("{src}/_2_one.rs#OneField"), format!("{src}/_6_cleave.rs"), "--root".into(), root.clone(), "--state".into(), state.clone()], uri: query("/cleave", &[("target", &format!("{src}/_2_one.rs#OneField")), ("dest", &format!("{src}/_6_cleave.rs")), ("root", &root), ("state", &state)]), body: String::new() },
        Case { op: "move", argv: vec!["move".into(), one.clone(), format!("{src}/_5_moved.rs"), "--root".into(), root.clone(), "--state".into(), state.clone()], uri: query("/move", &[("old", &one), ("new", &format!("{src}/_5_moved.rs")), ("root", &root), ("state", &state)]), body: String::new() },
        Case { op: "rename", argv: vec!["rename".into(), format!("{src}/_0_types.rs#A"), "AZ".into(), "--root".into(), root.clone(), "--state".into(), state.clone()], uri: query("/rename", &[("target", &format!("{src}/_0_types.rs#A")), ("new", "AZ"), ("root", &root), ("state", &state)]), body: String::new() },
        Case { op: "query", argv: vec!["query".into(), one.clone(), "--query".into(), "(function_item name: (identifier) @name)".into()], uri: query("/query", &[("query", "(function_item name: (identifier) @name)")]), body: input(vec![&one]) },
        Case { op: "region", argv: vec!["region".into(), region.clone(), "demo".into(), "--generated".into(), generated.clone()], uri: query(&region_path, &[("generated", &generated)]), body: String::new() },
        Case { op: "watch", argv: vec!["watch".into(), "--root".into(), root.clone(), "--once".into(), "--receipts".into(), receipts.clone()], uri: query("/watch", &[("root", &root), ("once", "true"), ("receipts", &receipts)]), body: String::new() },
        Case { op: "diff", argv: vec!["diff".into(), "--root".into(), root.clone(), "--from".into(), "HEAD".into(), "--to".into(), "HEAD".into()], uri: query("/diff", &[("root", &root), ("from", "HEAD"), ("to", "HEAD")]), body: String::new() },
        Case { op: "ingest", argv: vec!["ingest".into(), tsi.into()], uri: query("/ingest", &[("paths", tsi)]), body: String::new() },
        Case { op: "schema", argv: vec!["schema".into()], uri: "/schema".into(), body: String::new() },
        Case { op: "trail", argv: vec!["trail".into()], uri: "/trail/5".into(), body: String::new() },
    ]
}

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

async fn socket_response(socket: &Path, uri: &str, body: &str) -> (axum::http::StatusCode, axum::body::Bytes) {
    let stream = tokio::net::UnixStream::connect(socket).await.expect("connect unix socket");
    let (mut client, connection) = http1::handshake(TokioIo::new(stream)).await.expect("HTTP handshake");
    tokio::spawn(async move { let _ = connection.await; });
    let request = Request::post(format!("http://localhost{uri}"))
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string())).expect("socket request");
    let response = client.send_request(request).await.expect("socket response");
    let status = response.status();
    let body = response.into_body().collect().await.expect("socket body").to_bytes();
    (status, body)
}

async fn socket_request(socket: &Path, uri: &str, body: &str) -> axum::body::Bytes {
    let (status, body) = socket_response(socket, uri, body).await;
    assert!(status.is_success(), "socket HTTP: {status}");
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
    let home = scratch.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let original_bin = PathBuf::from(env!("CARGO_BIN_EXE_ryi"));
    let binary = scratch.path().join("ryi");
    std::fs::copy(original_bin, &binary).expect("pin ryi for parity run");
    std::env::set_var("HOME", &home);
    std::env::set_var("RUST_LOG", "off");
    std::env::set_var("DL_TRAIL", "0");
    std::env::set_var("RYI_MAX_MEM_MB", "2048");

    let mut table = Vec::new();
    let mut fast_cli = Vec::new();
    let cases = cases(&root, scratch.path());
    let socket = scratch.path().join("ryi.sock");
    let server = Command::new(&binary).args(["serve", "--listen", &format!("unix:{}", socket.display())])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdout(Stdio::null()).stderr(Stdio::piped()).spawn().expect("start unix server");
    let _server = Server(server);
    for _ in 0..200 {
        if socket.exists() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(socket.exists(), "unix socket bound");
    for case in &cases {
        let cli = Command::new(&binary)
            .arg(&case.argv[0]).args(["--format", "jsonl"]).args(&case.argv[1..])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
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
        let body = socket_request(&socket, &case.uri, &case.body).await;
        table.push(row(case.op, "router", &body, &root, scratch.path()));
    }

    let fast = &cases[0];
    let socket_body = socket_request(&socket, &fast.uri, &fast.body).await;
    table.push(row("fast", "socket", &socket_body, &root, scratch.path()));

    let proof_socket = scratch.path().join("nochild.sock");
    let system_path = "/usr/bin:/bin:/opt/homebrew/bin";
    assert!(system_path.split(':').all(|dir| !Path::new(dir).join("ryi").exists()));
    let proof_server = Command::new(&binary).args(["serve", "--listen", &format!("unix:{}", proof_socket.display())])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("PATH", system_path).env_remove("RYI_BIN")
        .stdout(Stdio::null()).stderr(Stdio::piped()).spawn().expect("start unix server");
    let _proof_server = Server(proof_server);
    for _ in 0..200 {
        if proof_socket.exists() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(proof_socket.exists(), "proof socket bound");
    std::fs::remove_file(&binary).expect("remove server executable after launch");
    let proof_body = socket_request(&proof_socket, &fast.uri, &fast.body).await;

    let sqlite = scratch.path().join("fast-http.db");
    let sqlite_arg = sqlite.to_string_lossy().to_string();
    let (sqlite_status, sqlite_body) = socket_response(
        &socket,
        &query("/fast", &[("sqlite", &sqlite_arg)]),
        &fast.body,
    ).await;
    assert_eq!(sqlite_status, axum::http::StatusCode::OK);
    assert!(sqlite.exists(), "HTTP fast published SQLite");
    let sqlite_rows: Vec<serde_json::Value> = sqlite_body.split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("SQLite response JSONL"))
        .collect();
    assert!(sqlite_rows.iter().any(|row| row.as_str().is_some_and(|text| text.starts_with("Wrote "))));
    assert_eq!(sqlite_rows.last(), Some(&json!({"complete": true, "rows": sqlite_rows.len() - 1})));

    let rendered = table.join("\n");
    println!("{rendered}");
    assert_eq!(rendered, include_str!("fixtures/ryi_http_parity.tsv").trim_end());
    let no_child = row("fast", "nochild", &proof_body, &root, scratch.path());
    println!("{no_child}");
    assert_eq!(no_child, row("fast", "nochild", &fast_cli, &root, scratch.path()), "proof body: {}", String::from_utf8_lossy(&proof_body));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn operation_error_has_http_status_and_server_accepts_next_request() {
    let cli = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["scip", "--indexer", "not-a-language"])
        .env("DL_TRAIL", "0")
        .output().expect("CLI error");
    assert_eq!(cli.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&cli.stderr).contains("unknown language"));

    let scratch = tempfile::tempdir().expect("error scratch");
    let socket = scratch.path().join("errors.sock");
    let server = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["serve", "--listen", &format!("unix:{}", socket.display())])
        .env("DL_TRAIL", "0")
        .stdout(Stdio::null()).stderr(Stdio::piped())
        .spawn().expect("start error server");
    let _server = Server(server);
    for _ in 0..200 {
        if socket.exists() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(socket.exists(), "error socket bound");

    let (status, body) = socket_response(
        &socket,
        "/scip?indexer=not-a-language",
        r#"{"paths":[],"patterns":[],"entry":[]}"#,
    ).await;
    assert_eq!(status, axum::http::StatusCode::BAD_REQUEST);
    let rows: Vec<serde_json::Value> = body.split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("JSON error row"))
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["code"], 2);
    assert!(rows[0]["error"].as_str().unwrap().contains("unknown language"));
    assert_eq!(rows[1], json!({"complete": false, "rows": 0}));

    let first = scratch.path().join("first.rs");
    let unsupported = scratch.path().join("second.txt");
    std::fs::write(&first, "fn first() {}\n").expect("first query input");
    std::fs::write(&unsupported, "second\n").expect("unsupported query input");
    let query_uri = query("/query", &[("query", "(function_item name: (identifier) @name)")]);
    let query_body = json!({"paths": [first, unsupported], "patterns": [], "entry": []}).to_string();
    let (late_status, late_body) = socket_response(&socket, &query_uri, &query_body).await;
    assert_eq!(late_status, axum::http::StatusCode::OK);
    let late_rows: Vec<serde_json::Value> = late_body.split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| serde_json::from_slice(line).expect("late JSONL row"))
        .collect();
    assert_eq!(late_rows.len(), 2);
    assert_eq!(late_rows[0]["name"], "first");
    assert_eq!(late_rows[1]["code"], 2);
    assert!(late_rows[1]["error"].as_str().unwrap().contains("no language"));

    let (next_status, next_body) = socket_response(&socket, "/schema", "").await;
    assert_eq!(next_status, axum::http::StatusCode::OK);
    let _: serde_json::Value = serde_json::from_slice(&next_body).expect("next JSON response");
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
    let socket = scratch.path().join("watch.sock");
    let receipts = scratch.path().join("watch.db");
    let server = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["serve", "--listen", &format!("unix:{}", socket.display())])
        .env("DL_TRAIL", "0")
        .stdout(Stdio::null()).stderr(Stdio::piped())
        .spawn().expect("start watch server");
    let _server = Server(server);
    for _ in 0..200 {
        if socket.exists() { break; }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(socket.exists(), "watch socket bound");

    let stream = tokio::net::UnixStream::connect(&socket).await.expect("watch connect");
    let (mut client, connection) = http1::handshake(TokioIo::new(stream)).await.expect("watch handshake");
    tokio::spawn(async move { let _ = connection.await; });
    let root_arg = root.to_string_lossy().to_string();
    let receipts_arg = receipts.to_string_lossy().to_string();
    let uri = query("/watch", &[("root", &root_arg), ("receipts", &receipts_arg), ("poll_ms", "50")]);
    let request = Request::post(format!("http://localhost{uri}"))
        .body(Body::empty()).expect("watch request");
    let mut watch = tokio::time::timeout(Duration::from_secs(15), client.send_request(request))
        .await.expect("watch response arrived").expect("watch response");
    assert_eq!(watch.status(), axum::http::StatusCode::OK);
    let first = tokio::time::timeout(Duration::from_secs(15), watch.body_mut().frame())
        .await.expect("watch first row arrived").expect("watch first frame").expect("watch frame");
    assert!(!first.into_data().expect("watch data").is_empty());

    let one = root.join("src/_1_none.rs");
    let request_body = json!({"paths": [one], "patterns": [], "entry": []}).to_string();
    let query_uri = query("/query", &[("query", "(function_item name: (identifier) @name)")]);
    let (status, body) = tokio::time::timeout(Duration::from_secs(15), socket_response(&socket, &query_uri, &request_body))
        .await.expect("query while watch is open");
    assert_eq!(status, axum::http::StatusCode::OK);
    assert!(body.windows(b"\"complete\":true".len()).any(|part| part == b"\"complete\":true"));

    let old = one.to_string_lossy().to_string();
    let new = root.join("src/_5_moved.rs").to_string_lossy().to_string();
    let state = scratch.path().join("state").to_string_lossy().to_string();
    let move_uri = query("/move", &[("old", &old), ("new", &new), ("root", &root_arg), ("state", &state)]);
    let (move_status, move_body) = tokio::time::timeout(Duration::from_secs(15), socket_response(&socket, &move_uri, ""))
        .await.expect("move while watch is open");
    assert_eq!(move_status, axum::http::StatusCode::OK);
    let move_text: String = serde_json::from_slice(&move_body).expect("move response text");
    assert!(move_text.contains("plan "));

    drop(watch);
    drop(client);
    let (next_status, _) = tokio::time::timeout(Duration::from_secs(15), socket_response(&socket, "/schema", ""))
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
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
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
