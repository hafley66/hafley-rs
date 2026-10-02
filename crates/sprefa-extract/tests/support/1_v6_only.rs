//! v6-only written-syntax rows: reported, never asserted by ported goldens.

pub const WRITTEN_SYNTAX: [&str; 3] = ["call_site", "jsx_element", "jsx_attribute"];

pub fn is_written_syntax_row(line: &str) -> bool {
    WRITTEN_SYNTAX
        .iter()
        .any(|record| line.contains(&format!("\"record\":\"{record}\"")))
}

/// The stream without the written-syntax rows, line endings kept.
pub fn ported(stream: &str) -> String {
    stream
        .lines()
        .filter(|line| !is_written_syntax_row(line))
        .map(|line| format!("{line}\n"))
        .collect()
}
