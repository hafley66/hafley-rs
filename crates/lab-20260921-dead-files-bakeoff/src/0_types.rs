use serde::Deserialize;
use std::path::PathBuf;

#[derive(Debug, Deserialize)]
pub struct FileEdge {
    pub record: String,
    pub src_path: String,
    #[serde(default)]
    pub dst_path: String,
}

#[derive(Debug, PartialEq, Eq)]
pub struct DeadFile {
    pub path: PathBuf,
}
