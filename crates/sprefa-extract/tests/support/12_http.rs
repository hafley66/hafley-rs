use std::{cell::RefCell, collections::BTreeMap, path::Path, time::Duration};
use axum::{body::Body, http::{header, Request}};
use base64::Engine as _;
use http_body_util::BodyExt;
use hyper::client::conn::http1;
use hyper_util::rt::TokioIo;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use crate::daemon_guard::DaemonGuard;

pub fn evaluate(case: &Value) -> Value {
    let runtime = tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap();
    let guards = RefCell::new(BTreeMap::<String, DaemonGuard>::new());
    let saved = RefCell::new(BTreeMap::<String, Vec<u8>>::new());
    crate::fixture_runner::commands(case, |step| {
        let step = crate::fixture_runner::replace_strings(step, &[ ("$fixtures", &crate::fixture_runner::expand_text("$fixtures", "")), ("$manifest", env!("CARGO_MANIFEST_DIR")), ("$ryii", env!("CARGO_BIN_EXE_ryii")) ]);
        let slot = step["slot"].as_str().unwrap();
        if step["api"] == "server" {
            let guard = DaemonGuard::new(Path::new(step["cache"].as_str().unwrap()));
            let (output, _) = crate::command_support::execute(&step["bootstrap"], &|text| crate::fixture_runner::expand_text(text, ""));
            assert!(output.status.success());
            for _ in 0..200 {
                if std::os::unix::net::UnixStream::connect(guard.socket()).is_ok() && guard.pid().is_some() { break; }
                std::thread::sleep(Duration::from_millis(10));
            }
            assert!(guard.socket().exists()); assert!(guard.pid().is_some());
            guards.borrow_mut().insert(slot.to_string(), guard);
            return json!({"bound":true,"pid_recorded":true});
        }
        let socket = guards.borrow()[slot].socket().to_path_buf();
        if step["api"] == "watch" { return runtime.block_on(watch(&socket, &step)); }
        let op = step["op"].as_str().unwrap();
        let root = Path::new(step["root"].as_str().unwrap());
        let scratch = Path::new(step["scratch"].as_str().unwrap());
        if step["api"] == "parity" {
            let (cli, _) = crate::command_support::execute(&step["cli"], &|text| crate::fixture_runner::expand_text(text, ""));
            assert!(cli.status.success(), "{op}: {}", String::from_utf8_lossy(&cli.stderr));
            if op == "fast" { saved.borrow_mut().insert(op.to_string(), cli.stdout.clone()); }
            if let Some(state) = step["reset_state"].as_str() { if Path::new(state).exists() { std::fs::remove_dir_all(state).unwrap(); } }
            let (status, _, body) = runtime.block_on(socket_response_with_headers(&socket, op, &step["args"], Path::new(env!("CARGO_MANIFEST_DIR"))));
            assert!(status.is_success(), "{op}: {}", String::from_utf8_lossy(&body));
            let table = json!([row(op,"cli",&cli.stdout,root,scratch),row(op,"router",&body,root,scratch)]);
            assert_eq!(table, step["expect_table"]);
            return json!({"table":table,"cli_success":true,"http_success":true});
        }
        if let Some(path) = step["system_path"].as_str() { assert!(path.split(':').all(|dir| !Path::new(dir).join("ryii").exists())); }
        let mut result = runtime.block_on(request(&socket, &step));
        let body = result["body"].as_str().unwrap().as_bytes();
        if let Some(transport) = step["transport"].as_str() { let rendered = row(op,transport,body,root,scratch); assert_eq!(rendered, step["expect_table"].as_str().unwrap()); result["table"] = json!(rendered); }
        if let Some(cell) = step["equals_saved"].as_str() { let rendered = row(op,"nochild",result["body"].as_str().unwrap().as_bytes(),root,scratch); assert_eq!(rendered,row(op,"nochild",&saved.borrow()[cell],root,scratch)); result["table"] = json!(rendered); }
        project(&step, result)
    })
}

async fn request(socket: &Path, step: &Value) -> Value {
    let op = step["op"].as_str().unwrap();
    let (status, headers, body) = socket_response_with_headers(socket,op,&step["args"],Path::new(env!("CARGO_MANIFEST_DIR"))).await;
    if let Some(expected) = step["expect_status"].as_u64() { assert_eq!(status.as_u16() as u64, expected); } else { assert!(status.is_success()); }
    let text = String::from_utf8(body.to_vec()).unwrap();
    if let Some(expected) = step["expect_body"].as_str() { assert_eq!(text, expected); }
    if let Some(expected) = step["expect_prefix"].as_str() { assert!(text.starts_with(expected)); }
    if let Some(expected) = step["expect_contains"].as_str() { assert!(text.contains(expected)); }
    if step["expect_nonempty"] == true { assert!(!text.is_empty()); }
    let exit = headers.get("x-ryi-exit-code").map(|header| header.to_str().unwrap());
    if let Some(expected) = step["expect_exit_header"].as_str() { assert_eq!(exit, Some(expected)); }
    let stderr = headers.get("x-ryi-stderr").map(|header| String::from_utf8(base64::engine::general_purpose::STANDARD.decode(header.as_bytes()).unwrap()).unwrap()).unwrap_or_default();
    if let Some(expected) = step["expect_stderr_contains"].as_str() { assert!(stderr.contains(expected)); }
    if let Some(expected) = step["expect_rows"].as_array() {
        let rows: Vec<Value> = text.lines().map(|line| serde_json::from_str(line).unwrap()).collect();
        assert_eq!(rows.len(), expected.len());
        for (row, expected) in rows.iter().zip(expected) { for (field,value) in expected.as_object().unwrap() {
            if let Some(field) = field.strip_suffix("_contains") { assert!(row[field].as_str().unwrap().contains(value.as_str().unwrap())); }
            else { assert_eq!(&row[field], value); }
        } }
    }
    json!({"status":status.as_u16(),"exit_header":exit,"stderr":stderr,"body":String::from_utf8(stable(op,&body,Path::new(step["root"].as_str().unwrap()),Path::new(step["scratch"].as_str().unwrap()))).unwrap()})
}

fn project(step: &Value, result: Value) -> Value {
    let mut output = serde_json::Map::new();
    if step["expect_success"] == true { output.insert("http_success".into(), json!(true)); } else { output.insert("status".into(), result["status"].clone()); }
    if result.get("table").is_some() { output.insert("table".into(), result["table"].clone()); }
    if step.get("expect_exit_header").is_some() { output.insert("exit_header".into(), result["exit_header"].clone()); }
    for (expect,field) in [("expect_body","body"),("expect_prefix","body_prefix"),("expect_contains","body_contains"),("expect_stderr_contains","stderr_contains"),("expect_rows","rows")] {
        if let Some(value) = step.get(expect) { output.insert(field.into(), value.clone()); }
    }
    if step["expect_nonempty"] == true { output.insert("body_nonempty".into(), json!(true)); }
    Value::Object(output)
}

async fn watch(socket: &Path, step: &Value) -> Value {
    let stream = tokio::net::UnixStream::connect(socket).await.unwrap();
    let (mut client, connection) = http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(async move { let _ = connection.await; });
    let message = Request::post("http://localhost/watch").header(header::CONTENT_TYPE,"application/json")
        .body(Body::from(json!({"request_root":env!("CARGO_MANIFEST_DIR"),"args":step["args"]}).to_string())).unwrap();
    let mut response = crate::wall_bench::bounded(Duration::from_secs(15),client.send_request(message)).await.unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let first = crate::wall_bench::bounded(Duration::from_secs(15),response.body_mut().frame()).await.unwrap().unwrap();
    assert!(!first.into_data().unwrap().is_empty());
    let mut observed = BTreeMap::new();
    for request_step in step["requests"].as_array().unwrap() {
        observed.insert(request_step["name"].as_str().unwrap().to_string(),project(request_step,crate::wall_bench::bounded(Duration::from_secs(15),request(socket,request_step)).await));
    }
    drop(response); drop(client);
    observed.insert("after_disconnect".to_string(),project(&step["after"],crate::wall_bench::bounded(Duration::from_secs(15),request(socket,&step["after"])).await));
    json!({"watch_status":200,"first_frame_nonempty":true,"requests":observed})
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
    text = mtime
        .replace_all(&text, "\"index_mtime_unix_ms\":0")
        .into_owned();
    let stage = regex::Regex::new(r"stage [0-9a-f]{64}").unwrap();
    text = stage.replace_all(&text, "stage <STAGE>").into_owned();
    for field in ["repository", "worktree"] {
        let identity = regex::Regex::new(&format!(r#""{field}":"[0-9a-f]{{64}}""#)).unwrap();
        text = identity
            .replace_all(&text, format!(r#""{field}":"<REPO>""#))
            .into_owned();
    }
    if matches!(
        op,
        "cleave" | "move" | "rename" | "region" | "ingest" | "schema" | "trail"
    ) {
        text.truncate(text.trim_end_matches('\n').len());
    }
    text.into_bytes()
}

fn row(op: &str, transport: &str, body: &[u8], root: &Path, scratch: &Path) -> String {
    let body = stable(op, body, root, scratch);
    let sha = format!("{:x}", Sha256::digest(&body));
    format!(
        "{op:<7} {transport:<6} {:>4} {}",
        body.split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .count(),
        &sha[..12]
    )
}

async fn socket_response_with_headers(
    socket: &Path,
    op: &str,
    args: &serde_json::Value,
    root: &Path,
) -> (
    axum::http::StatusCode,
    axum::http::HeaderMap,
    axum::body::Bytes,
) {
    let stream = tokio::net::UnixStream::connect(socket)
        .await
        .expect("connect unix socket");
    let (mut client, connection) = http1::handshake(TokioIo::new(stream))
        .await
        .expect("HTTP handshake");
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let envelope = json!({"request_root": root, "args": args}).to_string();
    let mut request = if op == "schema" {
        Request::get(format!("http://localhost/{op}"))
    } else {
        Request::post(format!("http://localhost/{op}"))
    };
    let body = if op == "ingest" {
        request = request.header(
            "x-ryi-request",
            base64::engine::general_purpose::STANDARD.encode(envelope),
        );
        Body::empty()
    } else {
        request = request.header(header::CONTENT_TYPE, "application/json");
        Body::from(envelope)
    };
    let request = request.body(body).expect("socket request");
    let response = client.send_request(request).await.expect("socket response");
    let status = response.status();
    let headers = response.headers().clone();
    let body = response
        .into_body()
        .collect()
        .await
        .expect("socket body")
        .to_bytes();
    (status, headers, body)
}

