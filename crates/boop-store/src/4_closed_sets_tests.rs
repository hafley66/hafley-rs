use crate::{closed_sets::REFERENCES, Store};
use rusqlite::Connection;

fn database(label: &str) -> std::path::PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("boop-v40-{label}-{suffix}.db"))
}

#[test]
fn enum_checks_and_open_dictionaries_survive_reopen_and_rebuild() {
    let path = database("constraints");
    let store = Store::open(path.clone()).unwrap();
    for &(table, old, dict) in REFERENCES {
        let column = old.strip_suffix("_id").unwrap();
        let sql: String = store
            .connection()
            .query_row(
                "SELECT sql FROM sqlite_master WHERE name=?1",
                [table],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            sql.contains(&format!("{column} TEXT")),
            "{table}.{column}: {sql}"
        );
        assert!(
            sql.contains(&format!("CHECK ({column} IN (")),
            "{table}.{column}: {sql}"
        );
        let remaining: i64 = store
            .connection()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name=?1",
                [dict],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(remaining, 0, "{dict}");
    }
    store.connection().execute_batch("INSERT INTO dict_session(value) VALUES ('enum-test'); INSERT INTO agent_session(session_id,harness) VALUES (1,'codex'); INSERT INTO agent_turn(session_id,turn,ts,role,said) VALUES (1,1,10,'developer','fixture');").unwrap();
    assert!(store
        .connection()
        .execute(
            "INSERT INTO agent_turn(session_id,turn,role) VALUES (1,2,'unknown-role')",
            []
        )
        .is_err());
    let open = store
        .intern_public("dict_path", "/new/path/outside/a/closed/set")
        .unwrap();
    assert_eq!(
        store
            .intern_public("dict_path", "/new/path/outside/a/closed/set")
            .unwrap(),
        open
    );
    drop(store);
    let store = Store::open(path.clone()).unwrap();
    assert_eq!(
        store.turn_rows(&Default::default()).unwrap()[0].role,
        "developer"
    );
    store.rebuild().unwrap();
    assert_eq!(store.schema_version().unwrap(), 40);
    let closed: i64 = store
        .connection()
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE name='dict_role'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(closed, 0);
    drop(store);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn failed_migration_rolls_back_nullable_orphans_and_invalid_values() {
    for (label, corruption) in [
        (
            "orphan",
            "UPDATE agent_session_observation SET harness_id=987654",
        ),
        (
            "unknown",
            "UPDATE dict_role SET value='unknown-role' WHERE value='assistant'",
        ),
    ] {
        let path = database(label);
        let store = Store::open(path.clone()).unwrap();
        store.connection().execute_batch("INSERT INTO dict_session(value) VALUES ('enum-test'); INSERT INTO agent_turn(session_id,turn,role) VALUES (1,1,'assistant'); INSERT INTO agent_session_observation(observation_key,session_id,observed_ts,harness,source) VALUES ('observation',1,10,'codex','trace-event');").unwrap();
        crate::legacy_tests::restore(store.connection());
        store
            .connection()
            .execute_batch("PRAGMA foreign_keys=OFF; PRAGMA user_version=39;")
            .unwrap();
        store.connection().execute_batch(corruption).unwrap();
        drop(store);
        assert!(Store::open(path.clone()).is_err(), "{label}");
        let raw = Connection::open(&path).unwrap();
        let state: (i64,i64,i64) = raw.query_row("SELECT (SELECT user_version FROM pragma_user_version), (SELECT count(*) FROM agent_turn), (SELECT count(*) FROM sqlite_master WHERE name LIKE '%_v40')", [], |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?))).unwrap();
        assert_eq!(state, (39, 1, 0), "{label}");
        assert_eq!(
            raw.query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='dict_role'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            1
        );
        drop(raw);
        std::fs::remove_file(path).unwrap();
    }
}

/// Explicit scratch-only stage 39 driver used by plans/dict-closed-sets/0_rehearse.sh.
#[test]
#[ignore = "requires an explicit backup copy under this lane's scratch directory"]
fn rehearsal_schema39() {
    let path = std::path::PathBuf::from(
        std::env::var("BOOP_REHEARSAL_DB").expect("BOOP_REHEARSAL_DB required"),
    );
    let scratch = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scratch")
        .canonicalize()
        .unwrap();
    let path = path.canonicalize().unwrap();
    assert!(path.starts_with(scratch));
    let store = Store::open_bounded_write(path, std::time::Duration::from_secs(5)).unwrap();
    assert_eq!(store.schema_version().unwrap(), 38);
    store.connection().execute_batch("BEGIN IMMEDIATE").unwrap();
    store.migrate_user_slice().unwrap();
    store
        .connection()
        .execute_batch("PRAGMA user_version=39; COMMIT;")
        .unwrap();
}
