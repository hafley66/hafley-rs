//! The three binding-typed receiver legs, ts: a field read whose type is
//! declared (`this.f.m()` on `class S { f: T }`), a constructor-return
//! binding (`const x = makeFoo(); x.bar()` where `function makeFoo(): Foo`),
//! and a generic bound (`function f<P extends Proj>(p: P) { p.project() }`
//! where the type-parameter constraint names the interface). Each binds the
//! declared member with origin `receiver`, never a corpus name match. The
//! shadow universe (free.ts + shadow.ts) proves a param, a callable `const`,
//! and an arrow param kill the corpus name match while a self-named
//! initializer still denotes the outer fn. One resolve run per universe; the
//! whole edge/drop tables freeze after the asserts in
//! `support/25_ts_binding_legs.rs`. Expected values are hand-derived from the
//! fixtures, never copied from the extractor's output.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_binding_legs_cases", crate::ts_binding_legs_support::evaluate);
}
