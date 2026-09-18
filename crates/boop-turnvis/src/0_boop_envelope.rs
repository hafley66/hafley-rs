//! Presentation-only removal of the default Boop delivery envelope, emitted as
//! `[boop {id} from {from}] {body}`. Other bracket text and XML remain literal.
pub fn boop_content(text: &str) -> &str {
    let trimmed = text.trim_start();
    let Some(header) = trimmed.strip_prefix("[boop ") else {
        return text;
    };
    let Some((header, body)) = header.split_once(']') else {
        return text;
    };
    if body.chars().next().is_some_and(|c| !c.is_whitespace()) {
        return text;
    }
    let Some((id, sender)) = header.split_once(" from ") else {
        return text;
    };
    let atom = |value: &str| {
        !value.is_empty()
            && !value
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '[' | ']'))
    };
    if !atom(id) || !atom(sender) {
        return text;
    }
    body.trim_start()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_the_verified_default_envelope_is_removed() {
        let cases = [
            (
                "[boop m1 from coordinator] actual user text",
                "actual user text",
            ),
            (
                "[boop m-4f2 from feature/tls]\nline one\nline two",
                "line one\nline two",
            ),
            ("[boop m1 from coordinator]", ""),
            ("[boop m1 from coordinator]   ", ""),
            (
                "[ordinary brackets] user text",
                "[ordinary brackets] user text",
            ),
            ("[boop idea] user text", "[boop idea] user text"),
            (
                "[BOOP m1 from coordinator] literal",
                "[BOOP m1 from coordinator] literal",
            ),
            (
                "[boop m1 from coordinator]literal",
                "[boop m1 from coordinator]literal",
            ),
            ("[boop m1 from ] user text", "[boop m1 from ] user text"),
            (
                "quote [boop m1 from coordinator]",
                "quote [boop m1 from coordinator]",
            ),
            ("<example>user XML</example>", "<example>user XML</example>"),
            (
                "<system-reminder>injected</system-reminder>",
                "<system-reminder>injected</system-reminder>",
            ),
        ];
        for (raw, expected) in cases {
            assert_eq!(boop_content(raw), expected, "{raw}");
        }
        assert_eq!(
            crate::normalize_turn_line("❯ [BOOP m1 from coordinator] Literal"),
            "[boop m1 from coordinator] literal"
        );
        assert_eq!(
            crate::normalize_turn_line("❯ [boop m1 from coordinator] Actual text"),
            "actual text"
        );
    }
}
