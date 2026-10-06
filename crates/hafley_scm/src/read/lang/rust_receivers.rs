//! Rust receiver syntax rows are produced by hafley_scm; ryi interns them.

use crate::read::shape::{Span, Strings};
use crate::read::types::{CallF, FamilyBundle, ReceiverBinding, ReceiverOutcome};

pub fn collect_receivers_from_tree(
    tree: &tree_sitter::Tree,
    source: &[u8],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    sink.aux.receivers.extend(
        hafley_scm::lang::rust::receiver_rows_from_tree(tree, source)
            .into_iter()
            .map(|row| ReceiverBinding {
                call_site: Span {
                    start: row.call_site.start,
                    len: row.call_site.len,
                },
                outcome: match row.outcome {
                    hafley_scm::lang::rust::RustReceiverOutcome::Named(name) => {
                        ReceiverOutcome::Named(strings.intern(&name))
                    }
                    hafley_scm::lang::rust::RustReceiverOutcome::Inferred => {
                        ReceiverOutcome::Inferred
                    }
                    hafley_scm::lang::rust::RustReceiverOutcome::Shadowed => {
                        ReceiverOutcome::Shadowed
                    }
                },
            }),
    );
}
