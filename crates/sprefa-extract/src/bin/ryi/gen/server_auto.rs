// Generated for ryii from the Ryi HTTP operations and @daemon options.
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use axum::body::{Body, Bytes};
use axum::extract::State;
use axum::http::{HeaderMap, Request as HttpRequest, StatusCode};
use axum::http::header::CONTENT_TYPE;
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Json;
use base64::Engine as _;
use fs4::fs_std::FileExt as _;
use futures_util::StreamExt as _;
use http_body::Frame;
use http_body_util::{BodyExt as _, StreamBody};
use tokio_util::sync::CancellationToken;
use tokio::io::AsyncWriteExt as _;
use tracing::Instrument as _;

use crate::daemon_auto::Request;
use crate::ops_auto::*;

fn error_status(code: i32) -> StatusCode {
    match code {
        2 => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn error_response(error: OpError) -> Response {
    let code = error.1;
    let mut row = serde_json::to_vec(&serde_json::json!({"error": error.0, "code": error.1})).expect("error row serializes");
    row.push(b'\n');
    let mut response = (error_status(code), [(CONTENT_TYPE, "application/x-ndjson")], row).into_response();
    response.headers_mut().insert(HeaderName::from_static("x-ryi-exit-code"), HeaderValue::from_str(&code.to_string()).expect("exit code header"));
    response
}

fn bad_request(message: String) -> Response { error_response(OpError(message, 2)) }

fn diagnostic_header(diagnostics: &crate::ops::Diagnostics) -> Option<HeaderValue> {
    let bytes = std::mem::take(&mut *diagnostics.lock().unwrap());
    if bytes.is_empty() { return None; }
    HeaderValue::from_str(&base64::engine::general_purpose::STANDARD.encode(bytes)).ok()
}

fn operation_error_response(error: OpError, diagnostics: &crate::ops::Diagnostics) -> Response {
    let mut bytes = std::mem::take(&mut *diagnostics.lock().unwrap());
    if !error.0.is_empty() {
        bytes.extend_from_slice(error.0.as_bytes());
        bytes.push(b'\n');
    }
    let mut response = (error_status(error.1), [(CONTENT_TYPE, "application/x-ndjson")], Bytes::new()).into_response();
    response.headers_mut().insert(HeaderName::from_static("x-ryi-exit-code"), HeaderValue::from_str(&error.1.to_string()).expect("exit code header"));
    if !bytes.is_empty() {
        if let Ok(value) = HeaderValue::from_str(&base64::engine::general_purpose::STANDARD.encode(bytes)) {
            response.headers_mut().insert(HeaderName::from_static("x-ryi-stderr"), value);
        }
    }
    response
}

async fn jsonl_response(items: Box<dyn Iterator<Item = OpResult<Vec<u8>>> + Send>, diagnostics: crate::ops::Diagnostics) -> Response {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(Option<OpError>, Bytes)>(64);
    tokio::task::spawn_blocking(move || {
        let mut batch = Vec::with_capacity(64 * 1024);
        for item in items {
            match item {
                Ok(bytes) => {
                    if batch.len() + bytes.len() > 64 * 1024 && !batch.is_empty() {
                        if tx.blocking_send((None, Bytes::from(std::mem::take(&mut batch)))).is_err() { return; }
                    }
                    batch.extend_from_slice(&bytes);
                }
                Err(error) => {
                    if !batch.is_empty() && tx.blocking_send((None, Bytes::from(std::mem::take(&mut batch)))).is_err() { return; }
                    let mut line = serde_json::to_vec(&serde_json::json!({"error": &error.0, "code": error.1})).expect("error row serializes");
                    line.push(b'\n');
                    let _ = tx.blocking_send((Some(error), Bytes::from(line)));
                    return;
                }
            }
        }
        if !batch.is_empty() { let _ = tx.blocking_send((None, Bytes::from(batch))); }
    });
    let first = rx.recv().await;
    if let Some((Some(error), _)) = first.as_ref() {
        return operation_error_response(OpError(error.0.clone(), error.1), &diagnostics);
    }
    let stream = futures_util::stream::unfold((first, rx, diagnostics, None, false), |(first, mut rx, diagnostics, exit, finished)| async move {
        if finished { return None; }
        if let Some((code, bytes)) = first {
            return Some((Ok::<Frame<Bytes>, std::io::Error>(Frame::data(bytes)), (None, rx, diagnostics, code.map(|error| error.1).or(exit), false)));
        }
        if let Some((code, bytes)) = rx.recv().await {
            return Some((Ok(Frame::data(bytes)), (None, rx, diagnostics, code.map(|error| error.1).or(exit), false)));
        }
        let mut headers = HeaderMap::new();
        if let Some(value) = diagnostic_header(&diagnostics) {
            headers.insert(HeaderName::from_static("x-ryi-stderr"), value);
        }
        if let Some(code) = exit {
            headers.insert(HeaderName::from_static("x-ryi-exit-code"), HeaderValue::from_str(&code.to_string()).expect("exit code trailer"));
        }
        if headers.is_empty() { return None; }
        Some((Ok(Frame::trailers(headers)), (None, rx, diagnostics, exit, true)))
    });
    (StatusCode::OK, [(CONTENT_TYPE, "application/x-ndjson"), (axum::http::header::TRAILER, "x-ryi-stderr, x-ryi-exit-code")], Body::new(StreamBody::new(stream))).into_response()
}

async fn raw_response(out: OpResult<Vec<u8>>, diagnostics: &crate::ops::Diagnostics) -> Response {
    let mut response = match out {
        Ok(bytes) => ([(CONTENT_TYPE, "application/x-ndjson")], bytes).into_response(),
        Err(error) => operation_error_response(error, diagnostics),
    };
    if let Some(value) = diagnostic_header(diagnostics) {
        response.headers_mut().insert(HeaderName::from_static("x-ryi-stderr"), value);
    }
    response
}

macro_rules! stream_handler {
    ($handler:ident, $verb:literal, $args:ty, $op:ident) => {
        async fn $handler(Json(request): Json<Request>) -> Response {
            let root = request.request_root.clone();
            tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
            let args: $args = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };
            let diagnostics = Arc::new(Mutex::new(Vec::new()));
            let items = crate::ops::with_request_context(root, Some(diagnostics.clone()), || crate::ops::$op(&args));
            jsonl_response(items, diagnostics).await
        }
    };
}
macro_rules! raw_handler {
    ($handler:ident, $verb:literal, $args:ty, $op:ident) => {
        async fn $handler(Json(request): Json<Request>) -> Response {
            let root = request.request_root.clone();
            tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
            let args: $args = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };
            let diagnostics = Arc::new(Mutex::new(Vec::new()));
            let captured = diagnostics.clone();
            let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::$op(&args))).await;
            raw_response(out.unwrap_or_else(|error| Err(OpError(error.to_string(), 1))), &diagnostics).await
        }
    };
}

stream_handler!(capabilities, "capabilities", CapabilitiesArgs, capabilities);
raw_handler!(cleave, "cleave", CleaveArgs, cleave);
raw_handler!(r#move, "move", MoveArgs, r#move);
raw_handler!(rename, "rename", RenameArgs, rename);
raw_handler!(dismantle, "dismantle", DismantleArgs, dismantle);
stream_handler!(watch, "watch", WatchArgs, watch);
stream_handler!(diff, "diff", DiffArgs, diff);
raw_handler!(schema, "schema", SchemaArgs, schema);
raw_handler!(trail, "trail", TrailArgs, trail);
stream_handler!(stratify, "stratify", StratifyArgs, stratify);


async fn extract(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: ExtractArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::extract(&args)))).await;
    match out { Ok(out) => jsonl_response(out, diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn fast(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: FastArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::fast(&args)))).await;
    match out { Ok(out) => jsonl_response(out, diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn slow(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: SlowArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::slow(&args)))).await;
    match out { Ok(out) => jsonl_response(out, diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn scip(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: ScipArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::scip(&args)))).await;
    match out { Ok(out) => jsonl_response(out, diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn graph(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: GraphArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::graph(&args)))).await;
    match out { Ok(out) => jsonl_response(out, diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn query(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: QueryArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::query(&args)))).await;
    match out { Ok(out) => jsonl_response(out, diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn region(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: RegionArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::region(&args)))).await;
    match out { Ok(out) => raw_response(out, &diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

async fn ingest(headers: HeaderMap, body: Body) -> Response {
    let (json, input) = if let Some(encoded) = headers.get("x-ryi-request") {
        let json = match base64::engine::general_purpose::STANDARD.decode(encoded.as_bytes()) {
            Ok(json) => json,
            Err(error) => return bad_request(error.to_string()),
        };
        let staged = match tempfile::NamedTempFile::new() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let file = match staged.reopen() { Ok(file) => file, Err(error) => return bad_request(error.to_string()) };
        let mut file = tokio::fs::File::from_std(file);
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let mut reader = tokio_util::io::StreamReader::new(chunks);
        if let Err(error) = tokio::io::copy(&mut reader, &mut file).await { return bad_request(error.to_string()); }
        if let Err(error) = file.flush().await { return bad_request(error.to_string()); }
        (json, Some(Arc::new(staged)))
    } else {
        let json = match axum::body::to_bytes(body, 1024 * 1024).await {
            Ok(json) => json.to_vec(),
            Err(error) => return bad_request(error.to_string()),
        };
        (json, None)
    };
    let request: Request = match serde_json::from_slice(&json) {
        Ok(request) => request,
        Err(error) => return bad_request(error.to_string()),
    };
    let root = request.request_root.clone();
    tracing::Span::current().record("request_root", &tracing::field::display(root.display()));
    let args: IngestArgs = match request.decode() { Ok(args) => args, Err(error) => return bad_request(error) };

    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostics.clone();
    let out = tokio::task::spawn_blocking(move || crate::ops::with_request_context(root, Some(captured), || crate::ops::with_request_input(input, || crate::ops::ingest(&args)))).await;
    match out { Ok(out) => raw_response(out, &diagnostics).await, Err(error) => error_response(OpError(error.to_string(), 1)) }
}

#[derive(Clone)]
struct DaemonState { last: Arc<Mutex<Instant>>, active: Arc<AtomicUsize>, next_request: Arc<AtomicU64>, shutdown: CancellationToken, stamp: Arc<str>, flush_observe: fn() }

struct RequestGuard { state: DaemonState, span: tracing::Span }

impl Drop for RequestGuard {
    fn drop(&mut self) {
        let _span = &self.span;
        *self.state.last.lock().unwrap() = Instant::now();
        self.state.active.fetch_sub(1, Ordering::AcqRel);
        (self.state.flush_observe)();
    }
}

fn request_verb(path: &str) -> &'static str {
    match path {
        "/capabilities" => "capabilities",
        "/extract" => "extract",
        "/fast" => "fast",
        "/slow" => "slow",
        "/scip" => "scip",
        "/graph" => "graph",
        "/cleave" => "cleave",
        "/move" => "move",
        "/rename" => "rename",
        "/dismantle" => "dismantle",
        "/query" => "query",
        "/region" => "region",
        "/watch" => "watch",
        "/diff" => "diff",
        "/ingest" => "ingest",
        "/schema" => "schema",
        "/trail" => "trail",
        "/stratify" => "stratify",
        _ => "internal",
    }
}

async fn touch(State(state): State<DaemonState>, request: HttpRequest<Body>, next: Next) -> Response {
    state.active.fetch_add(1, Ordering::AcqRel);
    *state.last.lock().unwrap() = Instant::now();
    let request_id = state.next_request.fetch_add(1, Ordering::Relaxed);
    let span = tracing::info_span!("daemon_request", request_id, verb = %request_verb(request.uri().path()), request_root = tracing::field::Empty);
    let guard = RequestGuard { state, span: span.clone() };
    let response = next.run(request).instrument(span).await;
    response.map(|body| Body::new(StreamBody::new(body.into_stream().map(move |chunk| {
        let _keep_alive = &guard;
        chunk
    }))))
}

async fn handshake(State(state): State<DaemonState>, headers: HeaderMap) -> StatusCode {
    if crate::daemon_auto::handshake_enabled() && headers.get("x-ryi-build").and_then(|value| value.to_str().ok()) != Some(state.stamp.as_ref()) {
        state.shutdown.cancel();
        return StatusCode::CONFLICT;
    }
    StatusCode::OK
}

async fn shutdown(State(state): State<DaemonState>) -> StatusCode {
    state.shutdown.cancel();
    StatusCode::OK
}

fn router(state: DaemonState) -> axum::Router {
    axum::Router::new()
        .route("/__handshake", get(handshake))
        .route("/__shutdown", post(shutdown))
        .route("/capabilities", get(capabilities))
        .route("/extract", post(extract))
        .route("/fast", post(fast))
        .route("/slow", post(slow))
        .route("/scip", post(scip))
        .route("/graph", post(graph))
        .route("/cleave", post(cleave))
        .route("/move", post(r#move))
        .route("/rename", post(rename))
        .route("/dismantle", post(dismantle))
        .route("/query", post(query))
        .route("/region", post(region))
        .route("/watch", post(watch))
        .route("/diff", post(diff))
        .route("/ingest", post(ingest))
        .route("/schema", get(schema))
        .route("/trail", post(trail))
        .route("/stratify", post(stratify))
        .layer(axum::middleware::from_fn_with_state(state.clone(), touch))
        .with_state(state)
}

pub fn daemon(install_observe: impl FnOnce(), flush_observe: fn(), finish_observe: fn()) -> Result<(), Box<dyn std::error::Error>> {
    // Prepare the state directory and socket parent before daemonizing.
    // `daemonize` redirects stderr to /dev/null, so a failure after the fork
    // reaches the client as silence; here it still reaches the pipe the client
    // reads when readiness times out.
    let cache = crate::daemon_auto::cache_dir()?;
    std::fs::create_dir_all(&cache)?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(&cache, std::fs::Permissions::from_mode(0o700))?;
    }
    let socket = crate::daemon_auto::socket_path()?;
    if let Some(parent) = socket.parent() {
        std::fs::create_dir_all(parent)?;
        #[cfg(unix)] {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700))?;
        }
    }
    daemonize::Daemonize::new().start()?;
    install_observe();
    let lock = std::fs::OpenOptions::new().create(true).read(true).write(true).open(cache.join("ryi.lock"))?;
    match lock.try_lock_exclusive() {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
        Err(error) => return Err(error.into()),
    }
    if socket.exists() { std::fs::remove_file(&socket)?; }
    let pid_file = cache.join("ryi.pid");
    let stamp: Arc<str> = crate::daemon_auto::executable_stamp(&std::env::current_exe()?)?.into();
    let runtime = tokio::runtime::Builder::new_multi_thread().enable_all().build()?;
    let result = runtime.block_on(async {
        let listener = tokio::net::UnixListener::bind(&socket)?;
        std::fs::write(&pid_file, std::process::id().to_string())?;
        let state = DaemonState { last: Arc::new(Mutex::new(Instant::now())), active: Arc::new(AtomicUsize::new(0)), next_request: Arc::new(AtomicU64::new(0)), shutdown: CancellationToken::new(), stamp, flush_observe };
        let idle_state = state.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(1)).await;
                if idle_state.active.load(Ordering::Acquire) == 0 && idle_state.last.lock().unwrap().elapsed() >= Duration::from_secs(crate::daemon_auto::idle_secs()) {
                    idle_state.shutdown.cancel();
                    break;
                }
            }
        });
        let shutdown = state.shutdown.clone();
        axum::serve(listener, router(state)).with_graceful_shutdown(async move { shutdown.cancelled().await }).await?;
        Ok::<(), Box<dyn std::error::Error>>(())
    });
    let _ = std::fs::remove_file(&socket);
    if socket.parent() != Some(cache.as_path()) {
        if let Some(parent) = socket.parent() { let _ = std::fs::remove_dir(parent); }
    }
    let _ = std::fs::remove_file(pid_file);
    finish_observe();
    result
}
