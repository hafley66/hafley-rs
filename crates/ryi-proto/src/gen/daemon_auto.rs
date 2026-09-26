// Generated from Ryi's @daemon service and path-valued operation parameters.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub const IDLE_SECS: u64 = 600;
pub const HANDSHAKE: bool = true;

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

    pub fn decode<T: serde::de::DeserializeOwned>(mut self, verb: &str) -> Result<T, String> {
        if !self.request_root.is_absolute() {
            return Err("request_root must be absolute".into());
        }
        resolve_paths(verb, &self.request_root, &mut self.args);
        serde_json::from_value(self.args).map_err(|error| error.to_string())
    }
}

fn resolve_paths(verb: &str, root: &Path, args: &mut serde_json::Value) {
    let names: &[&str] = match verb {
        "extract" => &["paths", "entry", "root", "sqlite", "scip_index"],
        "fast" => &["paths", "entry", "root", "sqlite"],
        "slow" => &["paths", "entry", "root", "sqlite", "scip_index"],
        "scip" => &["paths", "entry", "root", "sqlite", "scip_index", "scip_cache"],
        "graph" => &["paths", "entry", "root", "sqlite", "scip_index"],
        "cleave" => &["dest", "list", "root", "state"],
        "move" => &["old", "new", "list", "root", "verify_cwd", "state"],
        "rename" => &["list", "root", "state", "verify_scip"],
        "query" => &["paths", "entry", "root", "sqlite"],
        "region" => &["target", "generated", "state"],
        "watch" => &["root", "receipts"],
        "diff" => &["root", "sqlite"],
        "ingest" => &["paths", "sqlite"],
        "schema" => &[],
        "trail" => &[],
        _ => &[],
    };
    let Some(object) = args.as_object_mut() else { return };
    for name in names {
        let Some(value) = object.get_mut(*name) else { continue };
        match value {
            serde_json::Value::String(path) => resolve_one(root, path),
            serde_json::Value::Array(paths) => {
                for value in paths {
                    if let Some(path) = value.as_str().map(str::to_owned) {
                        let mut path = path;
                        resolve_one(root, &mut path);
                        *value = serde_json::Value::String(path);
                    }
                }
            }
            _ => {}
        }
    }
}

fn resolve_one(root: &Path, path: &mut String) {
    if path == "-" || path.is_empty() || Path::new(path).is_absolute() { return; }
    *path = root.join(&*path).to_string_lossy().into_owned();
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
    Ok(cache_dir()?.join("ryi.sock"))
}
