use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use lsp_server::{Message, Notification, Request, RequestId, Response};
use lsp_types::notification::Notification as NotificationMethod;
use lsp_types::{
    notification::{DidChangeTextDocument, DidOpenTextDocument, Initialized},
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, InitializeParams, InitializedParams,
    TextDocumentContentChangeEvent, TextDocumentItem, Uri, VersionedTextDocumentIdentifier,
    WorkspaceFolder,
};
use serde::Serialize;
use serde_json::{json, Value};

const RESPONSE_TIMEOUT: Duration = Duration::from_secs(60);

pub static TS_SESSIONS: OnceLock<Mutex<HashMap<std::path::PathBuf, TsSession>>> = OnceLock::new();

pub struct TsSession {
    pub lsp: Ts7Lsp,
    pub version: String,
    pub opened: HashMap<String, (String, i32)>,
    pub pending: HashSet<String>,
    pub api: Option<Ts7Rpc>,
}

impl TsSession {
    pub fn open(root: &Path) -> Result<Self, String> {
        let mut lsp = Ts7Lsp::open(root)?;
        let mut initialize = InitializeParams::default();
        initialize.process_id = Some(std::process::id());
        initialize.workspace_folders = Some(vec![WorkspaceFolder {
            uri: file_uri(root)?,
            name: root
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
        }]);
        initialize.initialization_options = Some(json!({}));
        let response = lsp.request("initialize", &initialize)?;
        if response.error.is_some() {
            return Err(format!("initialize: {:?}", response.error));
        }
        lsp.notify::<Initialized>(&InitializedParams {})?;
        let version = response
            .result
            .as_ref()
            .and_then(|value| value["serverInfo"]["version"].as_str())
            .unwrap_or("")
            .to_string();
        Ok(Self {
            lsp,
            version,
            opened: HashMap::new(),
            pending: HashSet::new(),
            api: None,
        })
    }

    /// Bound one checker request across both transports; edit requests retain
    /// the original per-round-trip timeout after the checker clears this.
    pub fn set_deadline(&mut self, deadline: Option<Instant>) {
        self.lsp.rpc.deadline = deadline;
        if let Some(api) = &mut self.api {
            api.deadline = deadline;
        }
    }

    pub fn initialize_api(&mut self) -> Result<&mut Ts7Rpc, String> {
        if self.api.is_none() {
            let response = self
                .lsp
                .request("custom/initializeAPISession", &json!({}))?;
            if let Some(error) = response.error {
                return Err(format!("initializeAPISession: {error:?}"));
            }
            let value = response
                .result
                .ok_or("initializeAPISession: missing result")?;
            let pipe = value["pipe"]
                .as_str()
                .ok_or("initializeAPISession: missing pipe")?;
            let mut api = Ts7Rpc::socket(pipe)?;
            api.deadline = self.lsp.rpc.deadline;
            api.call("initialize", &Value::Null)?;
            self.api = Some(api);
        }
        Ok(self.api.as_mut().unwrap())
    }

    pub fn sync_document(&mut self, uri: &Uri, path: &str, text: &str) -> Result<(), String> {
        let key = uri.as_str().to_string();
        match self.opened.get(&key) {
            Some((previous, _)) if previous == text => return Ok(()),
            Some((_, version)) => {
                let _span =
                    tracing::debug_span!("typescript.lsp", lang = "ts", family = "didChange")
                        .entered();
                let version = version + 1;
                self.lsp
                    .notify::<DidChangeTextDocument>(&DidChangeTextDocumentParams {
                        text_document: VersionedTextDocumentIdentifier {
                            uri: uri.clone(),
                            version,
                        },
                        content_changes: vec![TextDocumentContentChangeEvent {
                            range: None,
                            range_length: None,
                            text: text.into(),
                        }],
                    })?;
                self.opened.insert(key, (text.into(), version));
            }
            None => {
                let _span = tracing::debug_span!("typescript.lsp", lang = "ts", family = "didOpen")
                    .entered();
                self.lsp
                    .notify::<DidOpenTextDocument>(&DidOpenTextDocumentParams {
                        text_document: TextDocumentItem {
                            uri: uri.clone(),
                            language_id: if path.ends_with(".tsx") {
                                "typescriptreact".into()
                            } else {
                                "typescript".into()
                            },
                            version: 1,
                            text: text.into(),
                        },
                    })?;
                self.opened.insert(key, (text.into(), 1));
            }
        }
        Ok(())
    }
}

pub struct Ts7Lsp {
    pub child: Child,
    rpc: Ts7Rpc,
}

/// The framed transport shared by LSP stdio and its hosted API socket.
pub struct Ts7Rpc {
    writer: Box<dyn Write + Send>,
    receiver: Receiver<Result<Option<Message>, String>>,
    next_id: i32,
    deadline: Option<Instant>,
}

impl Ts7Lsp {
    fn open(root: &Path) -> Result<Self, String> {
        let executable = std::env::var_os("SPREFA_TSGO")
            .map(std::path::PathBuf::from)
            .or_else(|| crate::read::lang::ts_lib::typescript_executable(Some(root)))
            .ok_or_else(|| {
                "stock tsgo executable missing from the TypeScript install".to_string()
            })?;
        let mut child = Command::new(&executable)
            .args(["--lsp", "--stdio"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("start tsgo {}: {error}", executable.display()))?;
        let stdin = child.stdin.take().ok_or("tsgo stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("tsgo stdout unavailable")?;
        Ok(Self {
            child,
            rpc: Ts7Rpc::connect(stdout, stdin),
        })
    }

    fn notify<N: NotificationMethod>(&mut self, params: &N::Params) -> Result<(), String> {
        self.rpc.notify::<N>(params)
    }

    pub fn request(&mut self, method: &str, params: &impl Serialize) -> Result<Response, String> {
        self.rpc.request(method, params)
    }
}

impl Ts7Rpc {
    fn connect(
        reader: impl std::io::Read + Send + 'static,
        writer: impl Write + Send + 'static,
    ) -> Self {
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            loop {
                let message = Message::read(&mut reader).map_err(|error| error.to_string());
                let done = !matches!(message, Ok(Some(_)));
                if sender.send(message).is_err() || done {
                    break;
                }
            }
        });
        Self {
            writer: Box::new(writer),
            receiver,
            next_id: 1,
            deadline: None,
        }
    }

    #[cfg(unix)]
    fn socket(pipe: &str) -> Result<Self, String> {
        let socket =
            std::os::unix::net::UnixStream::connect(pipe).map_err(|error| error.to_string())?;
        let reader = socket.try_clone().map_err(|error| error.to_string())?;
        Ok(Self::connect(reader, socket))
    }

    #[cfg(not(unix))]
    fn socket(_pipe: &str) -> Result<Self, String> {
        Err("stock tsgo API socket currently requires Unix".into())
    }

    pub fn call(&mut self, method: &str, params: &impl Serialize) -> Result<Value, String> {
        let response = self.request(method, params)?;
        if let Some(error) = response.error {
            return Err(format!("{method}: {error:?}"));
        }
        response
            .result
            .ok_or_else(|| format!("{method}: missing result"))
    }

    fn send(&mut self, message: Message) -> Result<(), String> {
        message
            .write(&mut self.writer)
            .map_err(|error| error.to_string())?;
        self.writer.flush().map_err(|error| error.to_string())
    }

    fn notify<N: NotificationMethod>(&mut self, params: &N::Params) -> Result<(), String> {
        self.send(Message::Notification(Notification {
            method: N::METHOD.into(),
            params: serde_json::to_value(params).map_err(|error| error.to_string())?,
        }))
    }

    pub fn request(&mut self, method: &str, params: &impl Serialize) -> Result<Response, String> {
        // One span per round trip: the summary's `files` column counts requests per method.
        let _span = tracing::debug_span!("typescript.lsp", lang = "ts", family = method).entered();
        let id = RequestId::from(self.next_id);
        self.next_id += 1;
        self.send(Message::Request(Request {
            id: id.clone(),
            method: method.into(),
            params: serde_json::to_value(params).map_err(|error| error.to_string())?,
        }))?;
        loop {
            let message = self
                .receiver
                .recv_timeout(
                    self.deadline
                        .map(|deadline| {
                            deadline
                                .saturating_duration_since(Instant::now())
                                .min(RESPONSE_TIMEOUT)
                        })
                        .unwrap_or(RESPONSE_TIMEOUT),
                )
                .map_err(|error| format!("{method}: {error}"))??;
            match message.ok_or_else(|| format!("{method}: server closed stdout"))? {
                Message::Response(response) if response.id == id => return Ok(response),
                Message::Request(request) => {
                    let result = match request.method.as_str() {
                        "workspace/configuration" => json!(request.params["items"]
                            .as_array()
                            .map(|items| vec![Value::Null; items.len()])
                            .unwrap_or_default()),
                        "workspace/workspaceFolders" => json!([]),
                        _ => Value::Null,
                    };
                    self.send(Message::Response(Response {
                        id: request.id,
                        result: Some(result),
                        error: None,
                    }))?;
                }
                _ => {}
            }
        }
    }
}

impl Drop for Ts7Lsp {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn file_uri(path: &Path) -> Result<Uri, String> {
    let url =
        url::Url::from_file_path(path).map_err(|_| format!("file URI for {}", path.display()))?;
    Uri::from_str(url.as_str()).map_err(|error| error.to_string())
}
