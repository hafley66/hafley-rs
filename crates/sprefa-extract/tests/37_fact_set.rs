//! dl6 fact reads: a stored (rel, column) value set loaded once per run, the
//! set membership a rule asks instead of a query.
//!
//! @comment-ok: sabotage receipt, repo law keeps these in TEST headers.
//! SABOTAGE: `FactSet::load` rewritten to `SELECT DISTINCT t."<column>"` (the
//! `__str` join dropped) measured `load` return raw surrogate integers, caught
//! by `every_value_comes_back_through_the_str_join` below: dictionary-joined
//! text is the whole contract, and the refusal tests pin the identifier law
//! that keeps the SQL unreachable.
//! FAIL-FIRST: `a_name_that_is_not_an_identifier_is_refused` with the rejection
//! deleted measured the injection string straight through `load`, so it fails
//! only on the missing identifier check.

use std::sync::Arc;

use rusqlite::Connection;
use sprefa_extract::{dl6_db_path, open_readonly, FactError, FactSet};

const REL: &str = "callee";
const COLUMN: &str = "name";

fn seeded(values: &[&str]) -> Connection {
    let store = Connection::open_in_memory().expect("in-memory store");
    store
        .execute_batch(
            "CREATE TABLE \"__str\" (\"__id\" INTEGER PRIMARY KEY, \"content\" TEXT NOT NULL UNIQUE);
             CREATE TABLE \"callee\" (\"__id\" INTEGER PRIMARY KEY, \"name\" INTEGER NOT NULL,
                UNIQUE (\"name\"));",
        )
        .expect("schema");
    for value in values {
        store
            .execute(
                "INSERT OR IGNORE INTO \"__str\" (\"content\") VALUES (?1)",
                [value],
            )
            .expect("intern");
        store
            .execute(
                "INSERT OR IGNORE INTO \"callee\" (\"name\")
                 SELECT \"__id\" FROM \"__str\" WHERE \"content\" = ?1",
                [value],
            )
            .expect("row");
    }
    store
}

#[test]
fn load_joins_the_stored_text_once() {
    let facts = Arc::new(FactSet::load(&seeded(&["beta", "gamma"]), REL, COLUMN).expect("load"));
    assert_eq!(facts.rel(), REL);
    assert_eq!(facts.column(), COLUMN);
    assert!(facts.contains("beta"));
    assert!(facts.contains("gamma"));
    assert!(!facts.contains("alpha"), "only stored values are members");
    let mut values: Vec<&str> = facts.values().collect();
    values.sort_unstable();
    assert_eq!(values, vec!["beta", "gamma"]);
}

#[test]
fn every_value_comes_back_through_the_str_join() {
    let facts = FactSet::load(&seeded(&["beta()", "gamma()", "delta()"]), REL, COLUMN)
        .expect("preload");
    let mut values: Vec<&str> = facts.values().collect();
    values.sort_unstable();
    assert_eq!(
        values,
        vec!["beta()", "delta()", "gamma()"],
        "the __str join turns surrogate ids back into the stored text"
    );
}

#[test]
fn from_values_builds_the_same_set_without_a_store() {
    let facts = FactSet::from_values(REL, COLUMN, ["beta", "gamma"]).expect("from_values");
    assert!(facts.contains("beta"));
    assert!(!facts.contains("alpha"));
}

#[test]
fn a_name_that_is_not_an_identifier_is_refused() {
    let store = seeded(&["beta"]);
    assert_eq!(
        FactSet::load(&store, "callee\"; DROP TABLE \"__str", COLUMN),
        Err(FactError::Name("callee\"; DROP TABLE \"__str".into())),
    );
    assert_eq!(
        FactSet::load(&store, REL, ""),
        Err(FactError::Name(String::new())),
    );
}

#[test]
fn the_live_store_opens_read_only_and_preloads_once() {
    let path = match dl6_db_path() {
        Ok(path) if path.is_file() => path,
        _ => return,
    };
    let store = open_readonly(&path).expect("the live store opens read-only");
    let refused = store
        .execute(
            "CREATE TABLE \"__arc_c_probe\" (\"__id\" INTEGER PRIMARY KEY)",
            [],
        )
        .expect_err("a read-only connection cannot write the one server's db");
    assert!(
        refused.to_string().contains("readonly"),
        "{refused} names the read-only refusal"
    );

    let facts = FactSet::load(&store, "import_graph_candidate", "raw").expect("preload");
    assert_eq!(facts.rel(), "import_graph_candidate");
    assert!(
        facts.values().all(|value| !value.is_empty()),
        "every spec text came back through the __str join"
    );
}
