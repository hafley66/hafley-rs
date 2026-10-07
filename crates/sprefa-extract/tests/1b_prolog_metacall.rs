//! Prolog meta-call closure and goal arguments: SWI `meta_predicate` knowledge
//! drives both the call sites and the `reference` positions inside meta-predicate
//! arguments. The fixture `corpus_1_meta_closures.pl` is the corpus repro: all
//! four meta slots (`maplist/3`, `call/3`, `forall/2`, `findall/3`) must reach
//! `double/2`, and a `--resolve` run over the split def/use pair must mint the
//! four go/1 -> double/2 edges. Every old assert is a step in
//! `tests/fixtures/prolog_metacall_cases/`; the full (name, position, span)
//! tables freeze in the snapshot.
#![cfg(feature = "cli")]

#[test]
fn whole_output() {
    crate::fixture_runner::run("prolog_metacall_cases", |case| {
        crate::fixture_runner::commands(case, |_| serde_json::Value::Null)
    });
}
