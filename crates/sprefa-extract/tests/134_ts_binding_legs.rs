//! The three binding-typed receiver legs, ts: a field read whose type is
//! declared (`this.f.m()` on `class S { f: T }`), a constructor-return
//! binding (`const x = makeFoo(); x.bar()` where `function makeFoo(): Foo`),
//! and a generic bound (`function f<P extends Proj>(p: P) { p.project() }`
//! where the type-parameter constraint names the interface). Each binds the
//! declared member with origin `receiver`, never a corpus name match. The
//! shadow universe (free.ts + shadow.ts) proves a param, a callable `const`,
//! and an arrow param kill the corpus name match while a self-named
//! initializer still denotes the outer fn. Every old assert is a step in
//! `tests/fixtures/ts_binding_legs_cases/`; the whole edge/drop tables freeze
//! in the snapshot.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("ts_binding_legs_cases", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
