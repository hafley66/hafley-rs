//! `--witness` over `--resolve`: every leg that named a target is a witness on
//! the fact, and a leg that named another def is a fact of its own. The
//! syntax-tier stream freezes whole; the checker tier's normalized tables
//! freeze under the case (same normalization the retired
//! `all__t_98_resolve_witness__stock_*.snap` files pinned); every old assert
//! runs as code in `support/27_resolve_witness.rs` before the freeze.
//! SABOTAGE RECEIPT (base sha 8e050ed82): `--witness --resolve` was a clap
//! conflict, rc=2, and `ProjectEdge` carried no `witnesses` field.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("resolve_witness_cases", crate::resolve_witness_support::evaluate);
}
