#![cfg(feature = "cli")]
#![allow(dead_code)]

#[path = "../src/bin/ryi/gen/models/mod.rs"]
mod models;
#[path = "../src/bin/ryi/gen/ops_auto.rs"]
mod ops_auto;
#[path = "../src/bin/ryi/ops.rs"]
mod ops;
#[path = "../src/bin/ryi/gen/http_auto.rs"]
mod http_auto;

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use axum::body::Body;
use axum::http::{header, Request};
use http_body_util::BodyExt;
use serde_json::json;
use sha2::{Digest, Sha256};
use tower::ServiceExt;

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
    let original_bin = std::env::var_os("RYI_BIN").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_ryi")));
    let binary = scratch.path().join("ryi");
    std::fs::copy(original_bin, &binary).expect("pin ryi for parity run");
    std::env::set_var("RYI_BIN", &binary);
    std::env::set_var("HOME", &home);
    std::env::set_var("RUST_LOG", "off");
    std::env::set_var("DL_TRAIL", "0");
    std::env::set_var("RYI_MAX_MEM_MB", "2048");

    let app = http_auto::router();
    let mut table = Vec::new();
    let cases = cases(&root, scratch.path());
    for case in &cases {
        let cli = Command::new(&binary)
            .arg(&case.argv[0]).args(["--format", "jsonl"]).args(&case.argv[1..])
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .output().expect("CLI transport");
        assert!(cli.status.success(), "{} CLI: {}", case.op, String::from_utf8_lossy(&cli.stderr));
        table.push(row(case.op, "cli", &cli.stdout, &root, scratch.path()));
        if matches!(case.op, "cleave" | "move" | "rename") {
            let state = scratch.path().join("state");
            if state.exists() {
                std::fs::remove_dir_all(state).expect("reset dry-run stage for HTTP transport");
            }
        }
        let request = Request::post(&case.uri).header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(case.body.clone())).expect("HTTP request");
        let response = app.clone().oneshot(request).await.expect("Router oneshot");
        assert!(response.status().is_success(), "{} HTTP: {}", case.op, response.status());
        let body = response.into_body().collect().await.expect("HTTP body").to_bytes();
        table.push(row(case.op, "router", &body, &root, scratch.path()));
    }

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
    let fast = &cases[0];
    let socket_response = Command::new("curl")
        .args(["--silent", "--show-error", "--fail-with-body", "--unix-socket"])
        .arg(&socket)
        .args(["-X", "POST", "-H", "content-type: application/json", "--data-binary"])
        .arg(&fast.body)
        .arg("http://localhost/fast")
        .output().expect("curl unix socket");
    assert!(socket_response.status.success(), "{}", String::from_utf8_lossy(&socket_response.stderr));
    table.push(row("fast", "socket", &socket_response.stdout, &root, scratch.path()));

    let rendered = table.join("\n");
    println!("{rendered}");
    assert_eq!(rendered, include_str!("fixtures/ryi_http_parity.tsv").trim_end());
}
