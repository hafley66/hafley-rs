//! Defects the rust-analyzer entrypoint crawl measured
//! (`plans/extract-crawl-2026-08-29/rust.REPORT.md` section 7, kinks 3, 4, 5,
//! 6, 7). Fixtures in `tests/fixtures/rust_findings/`.
//!
//! FAIL-FIRST RECEIPT, all five red before the fix (binary at c60e5c4cc):
//! const-block fns minted no call defs ({} vs {inner, outer}); the const-block
//! sibling call did not resolve; initializer calls carried no const/static
//! item as caller; closure callers did not mirror onto the enclosing fn
//! ({closure@1182} vs {closure@1182, entry}); one mirror per closure-caller
//! edge was missing (spawn@1017 among 5 rows, 3 closure-caller).
//!
//! FAIL-FIRST RECEIPT for kinks 4 and 7, all four red at b9b98e3af:
//! module-qualified calls bound in the caller's own module; the type-qualifier
//! leg counted 4 vs 3; dropped sites minted no unresolved rows naming why;
//! rows != sites - edges (0 vs 1, sites 3, edges 2).
//!
//! SABOTAGE RECEIPTS: deleting the `syn::Item::Const` arm from
//! `call_defs_in_items` restores the const rows; returning `false` from
//! `initializer_defs` without minting the item def restores the initializer
//! row; dropping the `enclosing_named_def` push in `Resolve<CallF>` restores
//! the mirror rows; returning `None` from `module_qualifier` restores the
//! qualified rows; clearing `drops` on the rust `ResolveArm` restores the
//! unresolved rows. Each old assert now runs as code over the frozen tables'
//! sources in `support/21_rust_crawl_kinks.rs`, before the snapshot freezes.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("rust_crawl_kinks_cases", crate::rust_crawl_kinks_support::evaluate);
}
