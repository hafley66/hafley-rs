use std::fs::read_to_string;
use std::path::Path;

use crate::log::log_line;

pub fn slug(raw: &str) -> String {
    raw.to_lowercase()
}

pub fn load_config(dir: &str) -> String {
    log_line("load");
    read_to_string(Path::new(dir).join("config.json")).unwrap_or_default()
}
