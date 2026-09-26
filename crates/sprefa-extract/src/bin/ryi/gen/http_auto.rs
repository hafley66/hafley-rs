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

use crate::models::call_edge::CallEdge;
use crate::models::edit_plan::EditPlan;
use crate::models::fact_summary::FactSummary;
use crate::models::inputs::Inputs;
use crate::models::type_edge::TypeEdge;
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

impl IntoResponse for OpError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, self.0).into_response()
    }
}

fn jsonl_response(produce: impl FnOnce(&mut dyn FnMut(OpResult<Vec<u8>>) -> bool) + Send + 'static) -> Response {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, std::io::Error>>(64);
    tokio::task::spawn_blocking(move || {
        produce(&mut |line| {
            let chunk = line
                .map(|mut bytes| {
                    bytes.push(b'\n');
                    Bytes::from(bytes)
                })
                .map_err(|e| std::io::Error::other(e.0));
            let failed = chunk.is_err();
            tx.blocking_send(chunk).is_ok() && !failed
        })
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|chunk| (chunk, rx)) });
    ([(CONTENT_TYPE, "application/jsonl")], Body::from_stream(stream)).into_response()
}

fn jsonl_input<T: DeserializeOwned + Send + 'static>(body: Body) -> impl Iterator<Item = OpResult<T>> + Send {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<OpResult<T>>(64);
    tokio::spawn(async move {
        let mut chunks = body.into_data_stream();
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = chunks.next().await {
            match chunk {
                Ok(bytes) => buf.extend_from_slice(&bytes),
                Err(e) => {
                    let _ = tx.send(Err(OpError(e.to_string()))).await;
                    return;
                }
            }
            while let Some(pos) = buf.iter().position(|b| *b == b'\n') {
                let line: Vec<u8> = buf.drain(..=pos).collect();
                if line.iter().all(u8::is_ascii_whitespace) {
                    continue;
                }
                if tx.send(serde_json::from_slice(&line).map_err(OpError::from)).await.is_err() {
                    return;
                }
            }
        }
        if !buf.iter().all(u8::is_ascii_whitespace) {
            let _ = tx.send(serde_json::from_slice(&buf).map_err(OpError::from)).await;
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
  })
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

pub async fn slow(Query(query): Query<SlowQuery>, Json(body): Json<Inputs>) -> OpResult<Json<FactSummary>> {
  let args = SlowArgs {
      paths: query.paths,
      inputs: body,
      sqlite: query.sqlite,
      lines: query.lines.unwrap_or(false),
      scip_index: query.scip_index,
      no_checker: query.no_checker.unwrap_or(false),
      scip_timeout: query.scip_timeout,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::slow(&args)).await??;
  Ok(Json(out))
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

pub async fn scip(Query(query): Query<ScipQuery>, Json(body): Json<Inputs>) -> OpResult<Json<FactSummary>> {
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
  let out = tokio::task::spawn_blocking(move || crate::ops::scip(&args)).await??;
  Ok(Json(out))
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

pub async fn graph(Query(query): Query<GraphQuery>, Json(body): Json<Inputs>) -> OpResult<Json<Vec<CallEdge>>> {
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
  let out = tokio::task::spawn_blocking(move || crate::ops::graph(&args)).await??;
  Ok(Json(out))
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

pub async fn cleave(Query(query): Query<CleaveQuery>) -> OpResult<Json<EditPlan>> {
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

pub async fn r#move(Query(query): Query<MoveQuery>) -> OpResult<Json<EditPlan>> {
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

pub async fn rename(Query(query): Query<RenameQuery>) -> OpResult<Json<EditPlan>> {
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

pub async fn query(Query(query): Query<QueryQuery>, Json(body): Json<Inputs>) -> OpResult<Json<FactSummary>> {
  let args = QueryArgs {
      paths: query.paths,
      inputs: body,
      lang: query.lang,
      query: query.query,
      digest: query.digest,
      sqlite: query.sqlite,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::query(&args)).await??;
  Ok(Json(out))
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

pub async fn region(Path(path): Path<RegionPath>, Query(query): Query<RegionQuery>) -> OpResult<Json<EditPlan>> {
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

pub async fn watch(Query(query): Query<WatchQuery>) -> OpResult<Json<FactSummary>> {
  let args = WatchArgs {
      root: query.root,
      patterns: query.patterns,
      kinds: query.kinds,
      receipts: query.receipts,
      once: query.once.unwrap_or(false),
      poll_ms: query.poll_ms.unwrap_or(500),
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::watch(&args)).await??;
  Ok(Json(out))
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

pub async fn diff(Query(query): Query<DiffQuery>) -> OpResult<Json<FactSummary>> {
  let args = DiffArgs {
      root: query.root,
      from: query.from,
      to: query.to,
      patterns: query.patterns,
      arms: query.arms,
      sqlite: query.sqlite,
  };
  let out = tokio::task::spawn_blocking(move || crate::ops::diff(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Debug)]
pub struct IngestQuery {
  #[serde(default)]
  pub paths: Vec<PathBuf>,
  pub sqlite: Option<PathBuf>,
}

pub async fn ingest(Query(query): Query<IngestQuery>, headers: HeaderMap, body: Body) -> OpResult<Json<FactSummary>> {
  let args = IngestArgs {
      paths: query.paths,
      trace: headers.get("X-Trace").and_then(|v| v.to_str().ok()).map(|v| v.parse::<String>()).transpose().map_err(|e| OpError(e.to_string()))?,
      sqlite: query.sqlite,
  };
  let input = jsonl_input::<TypeEdge>(body);
  let out = tokio::task::spawn_blocking(move || crate::ops::ingest(&args, input)).await??;
  Ok(Json(out))
}

pub async fn schema() -> OpResult<Json<FactSummary>> {
  let args = SchemaArgs {

  };
  let out = tokio::task::spawn_blocking(move || crate::ops::schema(&args)).await??;
  Ok(Json(out))
}

#[derive(Deserialize, Default, Debug)]
pub struct TrailPath {
  pub runs: usize,
}

pub async fn trail(Path(path): Path<TrailPath>) -> OpResult<Json<FactSummary>> {
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
