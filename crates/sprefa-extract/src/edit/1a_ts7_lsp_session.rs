use std::collections::{HashMap, HashSet};
use std::io::{BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::str::FromStr;
use std::sync::mpsc::{self, Receiver};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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

pub(super) static TS_SESSIONS: OnceLock<Mutex<HashMap<std::path::PathBuf, TsSession>>> =
    OnceLock::new();

pub(super) struct TsSession {
    pub(super) lsp: Ts7Lsp,
    opened: HashMap<String, (String, i32)>,
    pub(super) pending: HashSet<String>,
}

impl TsSession {
    pub(super) fn open(root: &Path) -> Result<Self, String> {
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
        Ok(Self {
            lsp,
            opened: HashMap::new(),
            pending: HashSet::new(),
        })
    }

    pub(super) fn sync_document(
        &mut self,
        uri: &Uri,
        path: &str,
        text: &str,
    ) -> Result<(), String> {
        let key = uri.as_str().to_string();
        match self.opened.get(&key) {
            Some((previous, _)) if previous == text => return Ok(()),
            Some((_, version)) => {
                let _span = tracing::debug_span!("typescript.lsp", lang = "ts", family = "didChange").entered();
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
                let _span = tracing::debug_span!("typescript.lsp", lang = "ts", family = "didOpen").entered();
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

pub(super) struct Ts7Lsp {
    child: Child,
    stdin: ChildStdin,
    receiver: Receiver<Result<Option<Message>, String>>,
    next_id: i32,
}

impl Ts7Lsp {
    fn open(root: &Path) -> Result<Self, String> {
        // The fast tier reads its lib globals from this same install.
        let Some(package) = crate::lang::ts_lib::typescript_package(Some(root)) else {
            return Err(format!(
                "TypeScript LSP executable missing: {}",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("ts7/node_modules/typescript/bin/tsc").display()
            ));
        };
        let tsc = package.join("bin/tsc");
        let mut child = Command::new(&tsc)
            .args(["--lsp", "--stdio"])
            .current_dir(root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("start {}: {error}", tsc.display()))?;
        let stdin = child.stdin.take().ok_or("tsc stdin unavailable")?;
        let stdout = child.stdout.take().ok_or("tsc stdout unavailable")?;
        let (sender, receiver) = mpsc::channel();
        std::thread::spawn(move || {
            let mut stdout = BufReader::new(stdout);
            loop {
                let message = Message::read(&mut stdout).map_err(|error| error.to_string());
                let done = !matches!(message, Ok(Some(_)));
                if sender.send(message).is_err() || done {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            receiver,
            next_id: 1,
        })
    }

    fn send(&mut self, message: Message) -> Result<(), String> {
        message
            .write(&mut self.stdin)
            .map_err(|error| error.to_string())?;
        self.stdin.flush().map_err(|error| error.to_string())
    }

    fn notify<N: NotificationMethod>(&mut self, params: &N::Params) -> Result<(), String> {
        self.send(Message::Notification(Notification {
            method: N::METHOD.into(),
            params: serde_json::to_value(params).map_err(|error| error.to_string())?,
        }))
    }

    pub(super) fn request(
        &mut self,
        method: &str,
        params: &impl Serialize,
    ) -> Result<Response, String> {
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
                .recv_timeout(RESPONSE_TIMEOUT)
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

pub(super) fn file_uri(path: &Path) -> Result<Uri, String> {
    let url =
        url::Url::from_file_path(path).map_err(|_| format!("file URI for {}", path.display()))?;
    Uri::from_str(url.as_str()).map_err(|error| error.to_string())
}
