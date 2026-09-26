use std::error::Error;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::os::unix::process::CommandExt as _;
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
    let sibling = std::env::current_exe()?.with_file_name("ryi-server");
    Ok(if sibling.exists() { sibling } else { PathBuf::from("ryi-server") })
}

fn server_stamp(server: &Path) -> Result<String, ClientError> {
    let output = Command::new(server).arg("--stamp").output()?;
    if !output.status.success() { return Err("ryi-server --stamp failed".into()); }
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

fn start_daemon(server: &Path) -> Result<(), ClientError> {
    Command::new(server).arg("--daemon")
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn()?;
    Ok(())
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

async fn handshake(socket: &Path, stamp: &str) -> Result<StatusCode, ClientError> {
    let request = Request::builder().method(Method::GET).uri("http://ryi/__handshake")
        .header("x-ryi-build", stamp).body(empty_body())?;
    Ok(send(request, socket).await?.status())
}

async fn ready_socket(server: &Path) -> Result<PathBuf, ClientError> {
    let socket = daemon_auto::socket_path()?;
    let stamp = if daemon_auto::HANDSHAKE { Some(server_stamp(server)?) } else { None };
    let mut started = false;
    for _ in 0..100 {
        match tokio::net::UnixStream::connect(&socket).await {
            Ok(stream) => {
                drop(stream);
                if let Some(stamp) = &stamp {
                    match handshake(&socket, stamp).await {
                        Ok(StatusCode::OK) => return Ok(socket),
                        Ok(StatusCode::CONFLICT) => {
                            // The server cancels itself on a mismatched build stamp.
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
                    start_daemon(server)?;
                    started = true;
                }
            }
            Err(error) => return Err(error.into()),
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        // A mismatched instance may still hold the lock. Retry spawn after it exits.
        if started && !socket.exists() { start_daemon(server)?; }
    }
    Err("ryi-server did not become ready".into())
}

fn command(cli: &Ryi) -> Result<(&'static str, serde_json::Value), ClientError> {
    let pair = match &cli.cmd {
        None => ("extract", serde_json::to_value(&cli.file)?),
        Some(Cmd::Extract(args)) => ("extract", serde_json::to_value(args)?),
        Some(Cmd::Fast(args)) => ("fast", serde_json::to_value(args)?),
        Some(Cmd::Slow(args)) => ("slow", serde_json::to_value(args)?),
        Some(Cmd::Scip(args)) => ("scip", serde_json::to_value(args)?),
        Some(Cmd::Graph(args)) => ("graph", serde_json::to_value(args)?),
        Some(Cmd::Cleave(args)) => ("cleave", serde_json::to_value(args)?),
        Some(Cmd::Move(args)) => ("move", serde_json::to_value(args)?),
        Some(Cmd::Rename(args)) => ("rename", serde_json::to_value(args)?),
        Some(Cmd::Query(args)) => ("query", serde_json::to_value(args)?),
        Some(Cmd::Region(args)) => ("region", serde_json::to_value(args)?),
        Some(Cmd::Watch(args)) => ("watch", serde_json::to_value(args)?),
        Some(Cmd::Diff(args)) => ("diff", serde_json::to_value(args)?),
        Some(Cmd::Ingest(args)) => ("ingest", serde_json::to_value(args)?),
        Some(Cmd::Schema) => ("schema", serde_json::json!({})),
        Some(Cmd::Trail(args)) => ("trail", serde_json::to_value(args)?),
    };
    Ok(pair)
}

async fn run() -> Result<i32, ClientError> {
    let mut argv: Vec<std::ffi::OsString> = std::env::args_os().collect();
    if argv.get(1).is_some_and(|arg| arg == "--fresh") && argv.len() > 2 {
        let fresh = argv.remove(1);
        argv.insert(2, fresh);
    }
    let cli = Ryi::parse_from(argv);
    let (verb, args) = command(&cli)?;
    let root = std::env::current_dir()?;
    let request = daemon_auto::Request::new(verb, root, &args)?;
    let json = serde_json::to_string(&request)?;
    let server = server_binary()?;
    if cli.fresh {
        let error = Command::new(server).args(["--oneshot", verb, &json])
            .stdin(Stdio::inherit()).stdout(Stdio::inherit()).stderr(Stdio::inherit()).exec();
        return Err(error.into());
    }
    let socket = ready_socket(&server).await?;
    let method = match verb {
        "schema" => Method::GET,
        _ => Method::POST,
    };
    let mut builder = Request::builder().method(method).uri(format!("http://ryi/{verb}"));
    let body = if verb == "ingest" {
        let metadata = base64::engine::general_purpose::STANDARD.encode(json.as_bytes());
        builder = builder.header("x-ryi-request", metadata).header("content-type", "application/x-ndjson");
        let stream = ReaderStream::new(tokio::io::stdin()).map(|chunk| chunk.map(Frame::data));
        StreamBody::new(stream).boxed_unsync()
    } else {
        builder = builder.header("content-type", "application/json");
        Full::new(Bytes::from(json)).map_err(|never| match never {}).boxed_unsync()
    };
    let response = send(builder.body(body)?, &socket).await?;
    let status = response.status();
    let mut body = response.into_body();
    let mut stdout = tokio::io::stdout();
    let mut exit = if status.is_success() { 0 } else if status == StatusCode::BAD_REQUEST { 2 } else { 1 };
    let mut pending = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = frame?;
        if let Ok(bytes) = frame.into_data() {
            stdout.write_all(&bytes).await?;
            for byte in &bytes {
                pending.push(*byte);
                if *byte == b'\n' {
                    if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&pending) {
                        if value.get("error").is_some() { exit = value.get("code").and_then(serde_json::Value::as_i64).unwrap_or(1) as i32; }
                    }
                    pending.clear();
                }
            }
        }
    }
    stdout.flush().await?;
    Ok(exit)
}

pub async fn main() -> ExitCode {
    match run().await {
        Ok(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        Err(error) => { eprintln!("ryi: {error}"); ExitCode::FAILURE }
    }
}
