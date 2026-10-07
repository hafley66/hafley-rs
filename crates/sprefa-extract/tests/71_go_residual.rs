//! The five residual go classes of the codeql-agreed set: a `type A = B` alias
//! receiver, a multi-hop chain whose hops are fields or an import-qualified
//! root, a one-hop receiver our scope never typed (range var, field read, index
//! read, multi-value define, type switch, conversion), a bare in-package call
//! whose name is not corpus-unique, and an import-qualified call a same-named
//! METHOD in the target directory was shadowing.
//!
//! Expected values are hand-derived from the fixtures, never copied from the
//! extractor's output. Every original `one_edge`/absence claim is a `claims`
//! row in `0_claims.json`; the whole resolved-edge table is one snapshot.
//!
//! TEST HEADER, HEAD failure before the fix
//! (`cargo test --release --features cli --test 71_go_residual`, 13 of 17):
//!   alias_receiver                    FAILED  one BasePing edge: []
//!   alias_chain                       FAILED  one BasePing edge: []
//!   field_hop_cross_package           FAILED  one BasePing edge: []
//!   field_hop_then_call               FAILED  one Make edge: []
//!   import_root_chain                 FAILED  one BasePing edge: []
//!   range_over_cross_package_field    FAILED  one BasePing edge: []
//!   range_over_channel                FAILED  one BasePing edge: []
//!   type_switch_case                  FAILED  one BasePing edge: []
//!   field_read_define                 FAILED  one BasePing edge: []
//!   index_read_define                 FAILED  one BasePing edge: []
//!   type_assertion_define             FAILED  one BasePing edge: []
//!   pointer_conversion                FAILED  one BasePing edge: []
//!   own_package_shadows_corpus_name   FAILED  one NewThing edge: []
//!   import_qualified_skips_method     FAILED  one IsThing edge: []
//!   multi_value_define                ok
//!   local_shadows_package_name        ok, by the import leg taking a METHOD
//!   range_index_only_declines         ok

#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("go_residual", crate::go_residual_support::evaluate);
}
