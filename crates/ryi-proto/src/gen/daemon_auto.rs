// Generated from Ryi's @daemon service.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

pub const IDLE_SECS: u64 = 600;
pub const HANDSHAKE: bool = true;
pub const SERVER_BIN: &str = "ryii";

pub fn idle_secs() -> u64 {
    std::env::var("RYI_IDLE_SECS").ok().and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0).unwrap_or(IDLE_SECS)
}

pub fn handshake_enabled() -> bool {
    match std::env::var("RYI_HANDSHAKE").as_deref() {
        Ok("1" | "true") => true,
        Ok("0" | "false") => false,
        _ => HANDSHAKE,
    }
}

pub fn executable_stamp(path: &Path) -> Result<String, std::io::Error> {
    let metadata = std::fs::metadata(path)?;
    let modified = metadata.modified()?.duration_since(std::time::UNIX_EPOCH)
        .map_err(std::io::Error::other)?;
    Ok(format!("{}:{}:{}", metadata.len(), modified.as_secs(), modified.subsec_nanos()))
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub request_root: PathBuf,
    pub args: serde_json::Value,
}

impl Request {
    pub fn new<T: Serialize>(request_root: PathBuf, args: &T) -> Result<Self, serde_json::Error> {
        Ok(Self { request_root, args: serde_json::to_value(args)? })
    }

    pub fn decode<T: serde::de::DeserializeOwned>(self) -> Result<T, String> {
        if !self.request_root.is_absolute() {
            return Err("request_root must be absolute".into());
        }
        serde_json::from_value(self.args).map_err(|error| error.to_string())
    }
}

pub fn request_uses_stdin(verb: &str, args: &serde_json::Value) -> bool {
    let names: &[&str] = match verb {
        "extract" => &["paths"],
        "fast" => &["paths"],
        "slow" => &["paths"],
        "scip" => &["paths"],
        "graph" => &["paths"],
        "cleave" => &[],
        "move" => &[],
        "rename" => &[],
        "query" => &["paths"],
        "region" => &["generated"],
        "watch" => &[],
        "diff" => &[],
        "ingest" => &["paths"],
        "schema" => &[],
        "trail" => &[],
        "stratify" => &[],
        _ => &[],
    };
    fn has_stdin(value: &serde_json::Value, names: &[&str]) -> bool {
        match value {
            serde_json::Value::Object(object) => object.iter().any(|(key, value)| {
                if names.contains(&key.as_str()) {
                    match value {
                        serde_json::Value::String(path) => path == "-" || path == "/dev/stdin",
                        serde_json::Value::Array(paths) => paths.iter().any(|path| path.as_str().is_some_and(|path| path == "-" || path == "/dev/stdin")),
                        _ => false,
                    }
                } else { has_stdin(value, names) }
            }),
            _ => false,
        }
    }
    has_stdin(args, names)
}

pub fn cache_dir() -> Result<PathBuf, std::io::Error> {
    let base = std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache"))
    }).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "HOME is unset"))?;
    if !base.is_absolute() {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "cache home must be absolute"));
    }
    Ok(base.join("ryi"))
}

pub fn socket_path() -> Result<PathBuf, std::io::Error> {
    Ok(socket_path_at(&cache_dir()?))
}

pub fn socket_path_at(cache: &Path) -> PathBuf {
    use std::os::unix::ffi::OsStrExt as _;
    let socket = cache.join("ryi.sock");
    if socket.as_os_str().as_bytes().len() <= 100 { return socket; }
    let digest = Sha256::digest(cache.as_os_str().as_bytes());
    let digest = digest[..10].iter().fold(String::new(), |mut out, byte| {
        use std::fmt::Write as _;
        write!(out, "{byte:02x}").expect("hex digest");
        out
    });
    let name = format!("ryi-{digest}");
    let short = std::env::temp_dir().join(&name).join("ryi.sock");
    if short.as_os_str().as_bytes().len() <= 100 { return short; }
    PathBuf::from("/tmp").join(name).join("ryi.sock")
}
