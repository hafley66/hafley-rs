/// An exact session's current window, or an explicit window/pane target.
/// Pane-target commands interpret a bare `=name` as a window name; the colon
/// selects the session and prevents fallback to an unrelated current window.
pub fn exact_pane_target(target: &str) -> String {
    if target.starts_with(['%', '@']) { return target.to_owned(); }
    let exact = if target.starts_with(['=', '$']) { target.to_owned() } else { format!("={target}") };
    if exact.contains(':') { exact } else { format!("{exact}:") }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn names_and_ids_keep_their_target_kind() {
        for (input, expected) in [("work", "=work:"), ("work:2.1", "=work:2.1"),
            ("=work:", "=work:"), ("%7", "%7"), ("@3", "@3"), ("$2", "$2:")] {
            assert_eq!(exact_pane_target(input), expected);
        }
    }
}
