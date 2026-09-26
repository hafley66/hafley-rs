use axum::Json;
use axum::body::Body;
use axum::body::Bytes;
use axum::extract::Path;
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::http::header::CONTENT_TYPE;
use axum::response::IntoResponse;
use axum::response::Response;
use axum::routing::post;
use axum_extra::extract::Query;
use futures_util::StreamExt;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::path::PathBuf;
use std::io::{Seek, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::models::inputs::Inputs;
use crate::ops_auto::CleaveArgs;
use crate::ops_auto::DiffArgs;
use crate::ops_auto::FastArgs;
use crate::ops_auto::GraphArgs;
use crate::ops_auto::IngestArgs;
use crate::ops_auto::MoveArgs;
use crate::ops_auto::OpError;
use crate::ops_auto::OpResult;
use crate::ops_auto::QueryArgs;
use crate::ops_auto::RegionArgs;
use crate::ops_auto::RenameArgs;
use crate::ops_auto::SchemaArgs;
use crate::ops_auto::ScipArgs;
use crate::ops_auto::SlowArgs;
use crate::ops_auto::TrailArgs;
use crate::ops_auto::WatchArgs;

fn error_status(code: i32) -> StatusCode {
    match code {
        2 => StatusCode::BAD_REQUEST,
        3 | 5..=7 => StatusCode::UNPROCESSABLE_ENTITY,
        4 => StatusCode::NOT_FOUND,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

impl IntoResponse for OpError {
    fn into_response(self) -> Response {
        (error_status(self.1), Json(serde_json::json!({"error": self.0, "code": self.1}))).into_response()
    }
}

async fn jsonl_response(produce: impl FnOnce(&mut dyn FnMut(OpResult<Vec<u8>>) -> bool) + Send + 'static) -> Response {
    let result = tokio::task::spawn_blocking(move || -> OpResult<(std::fs::File, Option<i32>)> {
        let mut spool = tempfile::tempfile()?;
        let mut rows = 0u64;
        let mut failed = None;
        let mut write_error = None;
        produce(&mut |line| {
            let mut bytes = match line {
                Ok(bytes) => { rows += 1; bytes }
                Err(error) => {
                    failed = Some(error.1);
                    serde_json::to_vec(&serde_json::json!({"error": error.0, "code": error.1})).expect("error row serializes")
                }
            };
            bytes.push(b'\n');
            if let Err(error) = spool.write_all(&bytes) {
                write_error = Some(error);
                return false;
            }
            failed.is_none()
        });
        if let Some(error) = write_error { return Err(OpError::from(error)); }
        let mut complete = serde_json::to_vec(&serde_json::json!({"complete": failed.is_none(), "rows": rows})).expect("completion row serializes");
        complete.push(b'\n');
        spool.write_all(&complete)?;
        spool.rewind()?;
        Ok((spool, failed))
    }).await;
    match result {
        Ok(Ok((spool, failed))) => {
            let status = failed.map_or(StatusCode::OK, error_status);
            let stream = tokio_util::io::ReaderStream::new(tokio::fs::File::from_std(spool));
            (status, [(CONTENT_TYPE, "application/x-ndjson")], Body::from_stream(stream)).into_response()
        }
        Ok(Err(error)) => error.into_response(),
        Err(error) => OpError::from(error).into_response(),
    }
}

struct CancelOnDrop(Arc<AtomicBool>);
impl Drop for CancelOnDrop {
    fn drop(&mut self) { self.0.store(true, Ordering::Release); }
}

async fn live_jsonl_response(
    items: Box<dyn Iterator<Item = OpResult<serde_json::Value>> + Send>,
    cancelled: Arc<AtomicBool>,
) -> Response {
    let guard = CancelOnDrop(cancelled);
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(Option<i32>, Bytes)>(64);
    tokio::task::spawn_blocking(move || {
        let mut rows = 0u64;
        let mut failed = false;
        for item in items {
            let (mut bytes, code) = match item.and_then(|value| Ok(serde_json::to_vec(&value)?)) {
                Ok(bytes) => { rows += 1; (bytes, None) }
                Err(error) => {
                    failed = true;
                    (serde_json::to_vec(&serde_json::json!({"error": error.0, "code": error.1})).expect("error row serializes"), Some(error.1))
                }
            };
            bytes.push(b'\n');
            if tx.blocking_send((code, Bytes::from(bytes))).is_err() { return; }
            if failed { break; }
        }
        let mut complete = serde_json::to_vec(&serde_json::json!({"complete": !failed, "rows": rows})).expect("completion row serializes");
        complete.push(b'\n');
        let _ = tx.blocking_send((None, Bytes::from(complete)));
    });
    let Some(first) = rx.recv().await else { return OpError("watch produced no response".into(), 1).into_response(); };
    let status = first.0.map_or(StatusCode::OK, error_status);
    let stream = futures_util::stream::once(async move { Ok::<Bytes, std::io::Error>(first.1) })
        .chain(futures_util::stream::unfold((rx, guard), |(mut rx, guard)| async move {
            rx.recv().await.map(|(_, bytes)| (Ok::<Bytes, std::io::Error>(bytes), (rx, guard)))
        }));
    (status, [(CONTENT_TYPE, "application/x-ndjson")], Body::from_stream(stream)).into_response()
}

fn jsonl_input<T: DeserializeOwned + Send + 'static>(body: Body) -> impl Iterator<Item = OpResult<T>> + Send {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<OpResult<T>>(64);
    tokio::spawn(async move {
        let chunks = body.into_data_stream().map(|chunk| chunk.map_err(std::io::Error::other));
        let reader = tokio_util::io::StreamReader::new(chunks);
        let mut lines = tokio_util::codec::FramedRead::new(reader, tokio_util::codec::LinesCodec::new());
        while let Some(line) = lines.next().await {
            let value = line.map_err(OpError::from).and_then(|line| serde_json::from_str(&line).map_err(OpError::from));
            if tx.send(value).await.is_err() {
                return;
            }
        }
    });
    std::iter::from_fn(move || rx.blocking_recv())
}

#[derive(Deserialize, Debug)]
pub struct FastQuery {
  #[serde(default)]
  pub paths: Vec<String>,
  pub sqlite: Option<PathBuf>,
  pub lines: Option<bool>,
}

pub async fn fast(Query(query): Query<FastQuery>, Json(body): Json<Inputs>) -> Response {
  let args = FastArgs {
      paths: query.paths,
      inputs: body,
      sqlite: query.sqlite,
      lines: query.lines.unwrap_or(false),
  };
  jsonl_response(move |emit| {
      for item in crate::ops::fast(&args) {
          if !emit(item.and_then(|v| Ok(serde_json::to_vec(&v)?))) {
              break;
          }
      }
  }).await
}

#[derive(Deserialize, Debug)]
pub struct SlowQuery {
  #[serde(default)]
  pub paths: Vec<String>,
  pub sqlite: Option<PathBuf>,
  pub lines: Option<bool>,
  pub scip_index: Option<PathBuf>,
  pub no_checker: Option<bool>,
  pub scip_timeout: Option<u64>,
}

pub async fn slow(Query(query): Query<SlowQuery>, Json(body): Json<Inputs>) -> Response {
  let args = SlowArgs {
      paths: query.paths,
      inputs: body,
      sqlite: query.sqlite,
      lines: query.lines.unwrap_or(false),
      scip_index: query.scip_index,
      no_checker: query.no_checker.unwrap_or(false),
      scip_timeout: query.scip_timeout,
  };
  jsonl_response(move |emit| {
      for item in crate::ops::slow(&args) {
          if !emit(item.and_then(|v| Ok(serde_json::to_vec(&v)?))) {
              break;
          }
      }
  }).await
}

#[derive(Deserialize, Debug)]
pub struct ScipQuery {
  #[serde(default)]
  pub paths: Vec<String>,
  pub sqlite: Option<PathBuf>,
  pub lines: Option<bool>,
  pub scip_index: Option<PathBuf>,
  pub scip_cache: Option<PathBuf>,
  pub scip_timeout: Option<u64>,
  pub indexer: Option<String>,
  pub raw: Option<bool>,
  pub records: Option<String>,
  pub occurrence_text: Option<bool>,
  pub scip_build: Option<bool>,
}

pub async fn scip(Query(query): Query<ScipQuery>, Json(body): Json<Inputs>) -> Response {
  let args = ScipArgs {
      paths: query.paths,
      inputs: body,
      sqlite: query.sqlite,
      lines: query.lines.unwrap_or(false),
      scip_index: query.scip_index,
      scip_cache: query.scip_cache,
      scip_timeout: query.scip_timeout,
      indexer: query.indexer,
      raw: query.raw.unwrap_or(false),
      records: query.records,
      occurrence_text: query.occurrence_text.unwrap_or(false),
      scip_build: query.scip_build.unwrap_or(false),
  };
  jsonl_response(move |emit| {
      for item in crate::ops::scip(&args) {
          if !emit(item.and_then(|v| Ok(serde_json::to_vec(&v)?))) {
              break;
          }
      }
  }).await
}

#[derive(Deserialize, Debug)]
pub struct GraphQuery {
  #[serde(default)]
  pub paths: Vec<String>,
  pub callers: Option<String>,
  pub uses: Option<String>,
  pub from: Option<String>,
  pub call_path: Option<String>,
  pub type_path: Option<String>,
  pub flow_path: Option<String>,
  pub sqlite: Option<PathBuf>,
  pub slow: Option<bool>,
  pub timeout: Option<u64>,
  pub at: Option<String>,
  pub compare: Option<String>,
  pub scip_index: Option<PathBuf>,
  pub rust_checker: Option<bool>,
  pub ts_checker: Option<bool>,
  pub go_checker: Option<bool>,
}

pub async fn graph(Query(query): Query<GraphQuery>, Json(body): Json<Inputs>) -> Response {
  let args = GraphArgs {
      paths: query.paths,
      inputs: body,
      callers: query.callers,
      uses: query.uses,
      from: query.from,
      call_path: query.call_path,
      type_path: query.type_path,
      flow_path: query.flow_path,
      sqlite: query.sqlite,
      slow: query.slow.unwrap_or(false),
      timeout: query.timeout.unwrap_or(30),
      at: query.at,
      compare: query.compare,
      scip_index: query.scip_index,
      rust_checker: query.rust_checker.unwrap_or(false),
      ts_checker: query.ts_checker.unwrap_or(false),
      go_checker: query.go_checker.unwrap_or(false),
  };
  jsonl_response(move |emit| {
      for item in crate::ops::graph(&args) {
          if !emit(item.and_then(|v| Ok(serde_json::to_vec(&v)?))) {
              break;
          }
      }
  }).await
}

#[derive(Deserialize, Debug)]
pub struct CleaveQuery {
  pub target: Option<String>,
  pub dest: Option<PathBuf>,
  pub list: Option<PathBuf>,
  pub root: Option<PathBuf>,
  pub state: Option<PathBuf>,
  pub drag: Option<bool>,
  pub commit: Option<bool>,
  pub verify: Option<String>,
  pub text_refs: Option<bool>,
  pub json: Option<bool>,
}

pub async fn cleave(Query(query): Query<CleaveQuery>) -> OpResult<Json<serde_json::Value>> {
  let args = CleaveArgs {
      target: query.target,
      dest: query.dest,
      list: query.list,
      root: query.root,
      state: query.state,
      drag: query.drag.unwrap_or(false),
      commit: query.commit.unwrap_or(false),
      verify: query.verify,
      text_refs: query.text_refs.unwrap_or(false),
      json: query.json.unwrap_or(false),
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::cleave(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Debug)]
pub struct MoveQuery {
  pub old: Option<PathBuf>,
  pub new: Option<PathBuf>,
  pub list: Option<PathBuf>,
  #[serde(default)]
  pub root: Vec<PathBuf>,
  pub verify_cwd: Option<PathBuf>,
  pub state: Option<PathBuf>,
  pub commit: Option<bool>,
  pub shim: Option<bool>,
  pub relocate_mod: Option<bool>,
  pub verify: Option<String>,
  pub text_refs: Option<bool>,
}

pub async fn r#move(Query(query): Query<MoveQuery>) -> OpResult<Json<serde_json::Value>> {
  let args = MoveArgs {
      old: query.old,
      new: query.new,
      list: query.list,
      root: query.root,
      verify_cwd: query.verify_cwd,
      state: query.state,
      commit: query.commit.unwrap_or(false),
      shim: query.shim.unwrap_or(false),
      relocate_mod: query.relocate_mod.unwrap_or(false),
      verify: query.verify,
      text_refs: query.text_refs.unwrap_or(false),
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::r#move(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Debug)]
pub struct RenameQuery {
  pub target: Option<String>,
  pub new: Option<String>,
  pub list: Option<PathBuf>,
  pub root: Option<PathBuf>,
  pub state: Option<PathBuf>,
  pub at: Option<u32>,
  pub commit: Option<bool>,
  pub text_refs: Option<bool>,
  pub verify_scip: Option<PathBuf>,
  pub no_scip_merge: Option<bool>,
  pub json: Option<bool>,
}

pub async fn rename(Query(query): Query<RenameQuery>) -> OpResult<Json<serde_json::Value>> {
  let args = RenameArgs {
      target: query.target,
      new: query.new,
      list: query.list,
      root: query.root,
      state: query.state,
      at: query.at,
      commit: query.commit.unwrap_or(false),
      text_refs: query.text_refs.unwrap_or(false),
      verify_scip: query.verify_scip,
      no_scip_merge: query.no_scip_merge.unwrap_or(false),
      json: query.json.unwrap_or(false),
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::rename(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Debug)]
pub struct QueryQuery {
  #[serde(default)]
  pub paths: Vec<String>,
  pub lang: Option<String>,
  pub query: String,
  pub digest: Option<String>,
  pub sqlite: Option<PathBuf>,
}

pub async fn query(Query(query): Query<QueryQuery>, Json(body): Json<Inputs>) -> Response {
  let args = QueryArgs {
      paths: query.paths,
      inputs: body,
      lang: query.lang,
      query: query.query,
      digest: query.digest,
      sqlite: query.sqlite,
  };
  jsonl_response(move |emit| {
      for item in crate::ops::query(&args) {
          if !emit(item.and_then(|v| Ok(serde_json::to_vec(&v)?))) {
              break;
          }
      }
  }).await
}

#[derive(Deserialize, Default, Debug)]
pub struct RegionPath {
  pub target: PathBuf,
  pub id: String,
}

#[derive(Deserialize, Debug)]
pub struct RegionQuery {
  pub generated: Option<PathBuf>,
  pub apply: Option<bool>,
  pub state: Option<PathBuf>,
}

pub async fn region(Path(path): Path<RegionPath>, Query(query): Query<RegionQuery>) -> OpResult<Json<serde_json::Value>> {
  let args = RegionArgs {
      target: path.target,
      id: path.id,
      generated: query.generated.unwrap_or(PathBuf::from("-")),
      apply: query.apply.unwrap_or(false),
      state: query.state,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::region(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Debug)]
pub struct WatchQuery {
  pub root: Option<PathBuf>,
  #[serde(default)]
  pub patterns: Vec<String>,
  #[serde(default)]
  pub kinds: Vec<String>,
  pub receipts: Option<PathBuf>,
  pub once: Option<bool>,
  pub poll_ms: Option<u64>,
}

pub async fn watch(Query(query): Query<WatchQuery>) -> Response {
  let args = WatchArgs {
      root: query.root,
      patterns: query.patterns,
      kinds: query.kinds,
      receipts: query.receipts,
      once: query.once.unwrap_or(false),
      poll_ms: query.poll_ms.unwrap_or(500),
  };
  let (items, cancelled) = crate::ops::watch_live(&args);
  live_jsonl_response(items, cancelled).await
}

#[derive(Deserialize, Debug)]
pub struct DiffQuery {
  pub root: Option<PathBuf>,
  pub from: String,
  pub to: String,
  #[serde(default)]
  pub patterns: Vec<String>,
  #[serde(default)]
  pub arms: Vec<String>,
  pub sqlite: Option<PathBuf>,
}

pub async fn diff(Query(query): Query<DiffQuery>) -> Response {
  let args = DiffArgs {
      root: query.root,
      from: query.from,
      to: query.to,
      patterns: query.patterns,
      arms: query.arms,
      sqlite: query.sqlite,
  };
  jsonl_response(move |emit| {
      for item in crate::ops::diff(&args) {
          if !emit(item.and_then(|v| Ok(serde_json::to_vec(&v)?))) {
              break;
          }
      }
  }).await
}

#[derive(Deserialize, Debug)]
pub struct IngestQuery {
  #[serde(default)]
  pub paths: Vec<PathBuf>,
  pub sqlite: Option<PathBuf>,
}

pub async fn ingest(Query(query): Query<IngestQuery>, headers: HeaderMap, body: Body) -> OpResult<Json<serde_json::Value>> {
  let args = IngestArgs {
      paths: query.paths,
      trace: headers.get("X-Trace").and_then(|v| v.to_str().ok()).map(|v| v.parse::<String>()).transpose().map_err(|e| OpError(e.to_string(), 1))?,
      sqlite: query.sqlite,
  };
  let input = jsonl_input::<serde_json::Value>(body);
  let out = tokio::task::spawn_blocking(move || crate::ops::ingest(&args, input)).await??;
  Ok(Json(out))
}

pub async fn schema() -> OpResult<Json<serde_json::Value>> {
  let args = SchemaArgs {

  };
  let out = tokio::task::spawn_blocking(move || crate::ops::schema(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Default, Debug)]
pub struct TrailPath {
  pub runs: usize,
}

pub async fn trail(Path(path): Path<TrailPath>) -> OpResult<Json<serde_json::Value>> {
  let args = TrailArgs {
      runs: path.runs,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::trail(&args)).await??;
  Ok(Json(out))
}

pub fn router() -> axum::Router {
  axum::Router::new()
      .route("/fast", post(fast))
      .route("/slow", post(slow))
      .route("/scip", post(scip))
      .route("/graph", post(graph))
      .route("/cleave", post(cleave))
      .route("/move", post(r#move))
      .route("/rename", post(rename))
      .route("/query", post(query))
      .route("/region/{target}/{id}", post(region))
      .route("/watch", post(watch))
      .route("/diff", post(diff))
      .route("/ingest", post(ingest))
      .route("/schema", post(schema))
      .route("/trail/{runs}", post(trail))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt as _;

    #[tokio::test]
    async fn error_after_a_data_row_still_sets_http_status() {
        let response = jsonl_response(|emit| {
            assert!(emit(Ok(br#"{"record":"first"}"#.to_vec())));
            assert!(!emit(Err(OpError("late failure".into(), 3))));
        }).await;
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let body = response.into_body().collect().await.expect("response body").to_bytes();
        let rows: Vec<serde_json::Value> = body.split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| serde_json::from_slice(line).expect("JSONL row"))
            .collect();
        assert_eq!(rows, vec![
            serde_json::json!({"record": "first"}),
            serde_json::json!({"error": "late failure", "code": 3}),
            serde_json::json!({"complete": false, "rows": 1}),
        ]);
    }
}
