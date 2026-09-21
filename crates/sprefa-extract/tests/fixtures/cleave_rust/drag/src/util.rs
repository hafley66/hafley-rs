use std::fs::read_to_string;

use crate::log::log_line;

fn pad(raw: &str) -> String {
    let mut out = String::from(" ");
    out.push_str(raw);
    out
}

fn slug(raw: &str) -> String {
    raw.to_lowercase()
}

pub fn label(raw: &str) -> String {
    pad(raw)
}

pub fn load_config(dir: &str) -> String {
    log_line("load");
    pad(&slug(&read_to_string(dir).unwrap_or_default()))
}
