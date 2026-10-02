use std::ops::Range;

use tree_sitter::Query;

use crate::scmpp::Compiled;

/// ts: `start_byte_for_pattern` places each user pattern inside the routed text range it came from.
pub fn pair_routed_patterns(
    user: &Query,
    items: Vec<(Range<usize>, Compiled)>,
) -> Vec<(u16, Compiled)> {
    let starts: Vec<usize> = (0..user.pattern_count())
        .map(|pattern| user.start_byte_for_pattern(pattern))
        .collect();
    items
        .into_iter()
        .map(|(range, compiled)| {
            let pattern = starts
                .iter()
                .position(|start| range.contains(start))
                .expect("a routed range holds one user pattern");
            (pattern as u16, compiled)
        })
        .collect()
}
