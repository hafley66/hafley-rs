//! Rust receiver syntax rows are produced by hafley_scm; ryi interns them.

use crate::read::shape::{Span, Strings};
use crate::read::types::{CallF, FamilyBundle, ReceiverBinding, ReceiverOutcome};
use hafley_scm::lang::rust::{receiver_rows, RustReceiverOutcome};

/// One impl block's contribution to the corpus (type, method) table.
#[derive(Clone, Debug)]
pub struct ImplEntry {
    pub self_type: String,
    pub trait_name: Option<String>,
    pub methods: Vec<(String, Span)>,
}

pub fn collect_receivers(
    parsed: &syn::File,
    line_starts: &[u32],
    strings: &mut Strings,
    sink: &mut FamilyBundle<CallF>,
) {
    sink.aux.receivers.extend(receiver_rows(parsed, line_starts).into_iter().map(|row| {
        ReceiverBinding {
            call_site: Span { start: row.call_site.start, len: row.call_site.len },
            outcome: match row.outcome {
                RustReceiverOutcome::Named(name) => ReceiverOutcome::Named(strings.intern(&name)),
                RustReceiverOutcome::Inferred => ReceiverOutcome::Inferred,
                RustReceiverOutcome::Shadowed => ReceiverOutcome::Shadowed,
            },
        }
    }));
}
