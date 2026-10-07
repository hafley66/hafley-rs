//! `ryi stratify` over the copied `stratify_ts` fixture: determinism and an
//! unchanged tree, record shapes (strata, cycle, localities), entrypoint
//! ranking, full stems with collision omission, and the move-TSV round trip
//! through `ryi move`. Every old assert runs as code over the tables in
//! `support/30_stratify.rs` before the snapshot freezes them; scores stay
//! live relations (internal/touching, the 200-line budget).
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("stratify_cases", crate::stratify_support::evaluate);
}
