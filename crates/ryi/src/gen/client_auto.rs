use std::error::Error;

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

use base64::Engine as _;
use bytes::Bytes;
use clap::Parser as _;
use futures_util::StreamExt as _;
use http_body_util::{BodyExt as _, Full, StreamBody};
use hyper::body::Frame;
use hyper::{Method, Request, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::io::AsyncWriteExt as _;
use tokio_util::io::ReaderStream;

use crate::cli_auto::{Cmd, Ryi};
use crate::daemon_auto;

type ClientBody = http_body_util::combinators::UnsyncBoxBody<Bytes, std::io::Error>;
type ClientError = Box<dyn Error + Send + Sync>;

fn server_binary() -> Result<PathBuf, ClientError> {
    let sibling = std::env::current_exe()?.with_file_name(daemon_auto::SERVER_BIN);
    if sibling.is_file() { return Ok(sibling); }
    for dir in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = dir.join(daemon_auto::SERVER_BIN);
        if candidate.is_file() { return Ok(candidate); }
    }
    Err(std::io::Error::new(std::io::ErrorKind::NotFound, format!("{} was not found", daemon_auto::SERVER_BIN)).into())
}

fn start_daemon(server: &Path) -> Result<std::process::Child, ClientError> {
    // Keep the daemon's stderr: daemonize redirects it to /dev/null once it
    // forks, but a startup failure before that point lands here, and the
    // readiness timeout reports it instead of a bare "did not become ready".
    Ok(Command::new(server).arg("--daemon")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped()).spawn()?)
}

/// The daemon's own words for a failed start, read after readiness times out.
async fn daemon_failure_detail(child: Option<std::process::Child>) -> String {
    let Some(mut child) = child else { return String::new() };
    let mut detail = String::new();
    if let Some(stderr) = child.stderr.take() {
        let reader = tokio::task::spawn_blocking(move || {
            use std::io::Read as _;
            let mut text = String::new();
            let _ = std::io::BufReader::new(stderr).read_to_string(&mut text);
            text
        });
        if let Ok(Ok(text)) = tokio::time::timeout(Duration::from_secs(2), reader).await {
            detail = text.trim().to_string();
        }
    }
    let _ = child.wait();
    detail
}

async fn send(request: Request<ClientBody>, socket: &Path) -> Result<hyper::Response<hyper::body::Incoming>, ClientError> {
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (mut sender, connection) = hyper::client::conn::http1::handshake(TokioIo::new(stream)).await?;
    tokio::spawn(async move { let _ = connection.await; });
    Ok(sender.send_request(request).await?)
}

fn empty_body() -> ClientBody {
    Full::new(Bytes::new()).map_err(|never| match never {}).boxed_unsync()
}

fn append_diagnostics(bytes: &mut Vec<u8>, headers: &hyper::HeaderMap) -> Result<(), ClientError> {
    if let Some(value) = headers.get("x-ryi-stderr") {
        bytes.extend(base64::engine::general_purpose::STANDARD.decode(value.as_bytes())?);
    }
    Ok(())
}

async fn handshake(socket: &Path, stamp: &str) -> Result<StatusCode, ClientError> {
    let request = Request::builder().method(Method::GET).uri("http://ryi/__handshake")
        .header("x-ryi-build", stamp).body(empty_body())?;
    let response = send(request, socket).await?;
    let status = response.status();
    response.into_body().collect().await?;
    Ok(status)
}

async fn ready_socket(server: &Path) -> Result<PathBuf, ClientError> {
    let socket = daemon_auto::socket_path()?;
    let stamp = if daemon_auto::handshake_enabled() { Some(daemon_auto::executable_stamp(server)?) } else { None };
    let mut started = false;
    let mut started_child: Option<std::process::Child> = None;
    for _ in 0..100 {
        match tokio::net::UnixStream::connect(&socket).await {
            Ok(stream) => {
                drop(stream);
                if let Some(stamp) = &stamp {
                    match handshake(&socket, stamp).await {
                        Ok(StatusCode::OK) => return Ok(socket),
                        Ok(StatusCode::CONFLICT) => {
                            // The old daemon cancels itself when its executable differs.
                            started = false;
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        Ok(status) => return Err(format!("ryi handshake returned {status}").into()),
                        Err(_) => {}
                    }
                } else {
                    return Ok(socket);
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound || error.kind() == std::io::ErrorKind::ConnectionRefused => {
                if !started {
                    started_child = Some(start_daemon(server)?);
                    started = true;
                }
            }
            Err(error) => return Err(error.into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let detail = daemon_failure_detail(started_child).await;
    if detail.is_empty() {
        Err(format!("{} did not become ready", daemon_auto::SERVER_BIN).into())
    } else {
        Err(format!("{} did not become ready: {detail}", daemon_auto::SERVER_BIN).into())
    }
}

fn command(cli: &Ryi) -> Result<(&'static str, serde_json::Value), ClientError> {
    let pair = match &cli.cmd {
        None => ("extract", serde_json::to_value(&cli.file)?),
        Some(Cmd::Capabilities) => ("capabilities", serde_json::json!({})),
        Some(Cmd::Fast(args)) => ("fast", serde_json::to_value(args)?),
        Some(Cmd::Slow(args)) => ("slow", serde_json::to_value(args)?),
        Some(Cmd::Scip(args)) => ("scip", serde_json::to_value(args)?),
        Some(Cmd::Graph(args)) => ("graph", serde_json::to_value(args)?),
        Some(Cmd::Cleave(args)) => ("cleave", serde_json::to_value(args)?),
        Some(Cmd::Move(args)) => ("move", serde_json::to_value(args)?),
        Some(Cmd::Rename(args)) => ("rename", serde_json::to_value(args)?),
        Some(Cmd::Dismantle(args)) => ("dismantle", serde_json::to_value(args)?),
        Some(Cmd::Query(args)) => ("query", serde_json::to_value(args)?),
        Some(Cmd::Region(args)) => ("region", serde_json::to_value(args)?),
        Some(Cmd::Watch(args)) => ("watch", serde_json::to_value(args)?),
        Some(Cmd::Diff(args)) => ("diff", serde_json::to_value(args)?),
        Some(Cmd::Ingest(args)) => ("ingest", serde_json::to_value(args)?),
        Some(Cmd::Schema) => ("schema", serde_json::json!({})),
        Some(Cmd::Trail(args)) => ("trail", serde_json::to_value(args)?),
        Some(Cmd::Stratify(args)) => ("stratify", serde_json::to_value(args)?),
    };
    Ok(pair)
}

async fn run() -> Result<i32, ClientError> {
    let cli = Ryi::parse();
    let server = server_binary()?;
    let (verb, args) = command(&cli)?;
    let root = std::env::current_dir()?;
    let request = daemon_auto::Request::new(root, &args)?;
    let json = serde_json::to_string(&request)?;
    let socket = ready_socket(&server).await?;
    let method = match verb {
        "capabilities" => Method::GET,
        "schema" => Method::GET,
        _ => Method::POST,
    };
    let path = match verb {
        "capabilities" => "/capabilities",
        "extract" => "/extract",
        "fast" => "/fast",
        "slow" => "/slow",
        "scip" => "/scip",
        "graph" => "/graph",
        "cleave" => "/cleave",
        "move" => "/move",
        "rename" => "/rename",
        "dismantle" => "/dismantle",
        "query" => "/query",
        "region" => "/region",
        "watch" => "/watch",
        "diff" => "/diff",
        "ingest" => "/ingest",
        "schema" => "/schema",
        "trail" => "/trail",
        "stratify" => "/stratify",
        _ => return Err(format!("no HTTP path for {verb}").into()),
    };
    let mut builder = Request::builder().method(method).uri(format!("http://ryi{path}")).header("te", "trailers");
    let body = if matches!(verb, "extract" | "fast" | "slow" | "scip" | "graph" | "query" | "region" | "ingest") && daemon_auto::request_uses_stdin(verb, &request.args) {
        let metadata = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
        builder = builder.header("x-ryi-request", metadata).header("content-type", "application/octet-stream");
        let stream = ReaderStream::new(tokio::io::stdin()).map(|chunk| chunk.map(Frame::data));
        StreamBody::new(stream).boxed_unsync()
    } else {
        builder = builder.header("content-type", "application/json");
        Full::new(Bytes::from(json)).map_err(|never| match never {}).boxed_unsync()
    };
    let response = send(builder.body(body)?, &socket).await?;
    let status = response.status();
    let mut error_code = response.headers().get("x-ryi-exit-code")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<i32>().ok());
    let mut diagnostics = Vec::new();
    append_diagnostics(&mut diagnostics, response.headers())?;
    let mut body = response.into_body();
    if !status.is_success() {
        let mut stderr = tokio::io::stderr();
        while let Some(frame) = body.frame().await {
            match frame?.into_data() {
                Ok(bytes) => stderr.write_all(&bytes).await?,
                Err(frame) => if let Ok(headers) = frame.into_trailers() {
                    append_diagnostics(&mut diagnostics, &headers)?;
                    error_code = headers.get("x-ryi-exit-code").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok()).or(error_code);
                },
            }
        }
        stderr.write_all(&diagnostics).await?;
        stderr.flush().await?;
        return Ok(error_code.unwrap_or(if status == StatusCode::BAD_REQUEST { 2 } else { 1 }));
    }
    let mut stdout = tokio::io::BufWriter::with_capacity(64 * 1024, tokio::io::stdout());
    while let Some(frame) = body.frame().await {
        let frame = frame?;
        match frame.into_data() {
            Ok(bytes) => stdout.write_all(&bytes).await?,
            Err(frame) => if let Ok(headers) = frame.into_trailers() {
                append_diagnostics(&mut diagnostics, &headers)?;
                error_code = headers.get("x-ryi-exit-code").and_then(|value| value.to_str().ok()).and_then(|value| value.parse().ok()).or(error_code);
            },
        }
    }
    tokio::io::stderr().write_all(&diagnostics).await?;
    stdout.flush().await?;
    Ok(error_code.unwrap_or(0))
}

pub async fn main() -> ExitCode {
    match run().await {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => { eprintln!("ryi: {error}"); ExitCode::FAILURE }
    }
}
