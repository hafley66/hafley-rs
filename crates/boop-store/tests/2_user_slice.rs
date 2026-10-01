use boop_store::{
    user_slice::{FavoriteSource, FavoriteSourceKind},
    Store,
};
use rusqlite::{params, Connection};
use std::path::PathBuf;

fn fixture_path(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "boop-user-slice-{label}-{}-{}.db",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}
fn favorite_bodies(connection: &Connection) -> Vec<(i64, Vec<u8>)> {
    let mut statement = connection.prepare("SELECT f.favorite_id, CAST(m.body AS BLOB) FROM agent_favorite f JOIN markdown_cache m USING(markdown_id) ORDER BY f.favorite_id").unwrap();
    statement
        .query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
        .unwrap()
        .collect::<rusqlite::Result<_>>()
        .unwrap()
}
#[test]
fn v38_migration_preserves_every_kind_body_and_user_row() {
    let path = fixture_path("migration");
    let store = Store::open(path.clone()).unwrap();
    store.connection().execute_batch(include_str!("../sql/39_interned_fixture.sql")).unwrap();
    store.connection().execute_batch("DROP VIEW v_favorite; DROP TABLE agent_favorite;
      CREATE TABLE agent_favorite(favorite_id INTEGER PRIMARY KEY, markdown_id INTEGER NOT NULL, note TEXT, source TEXT NOT NULL DEFAULT '', created_ts INTEGER NOT NULL);
      DROP TABLE mood;
      CREATE TABLE dict_mood_name(id INTEGER PRIMARY KEY, value TEXT NOT NULL UNIQUE);
      CREATE TABLE mood(id INTEGER PRIMARY KEY, name_id INTEGER NOT NULL UNIQUE, template TEXT NOT NULL);
      INSERT INTO dict_mood_name VALUES(1,'plain'),(2,'unga'),(3,'board');
      INSERT INTO mood VALUES(10,1,'plain custom'),(20,2,'unga custom'),(30,3,'board custom');
      INSERT INTO agent_tag VALUES('kept',1,2,1);
      INSERT INTO agent_tag_link VALUES('kept','favorite:1',2);
      INSERT INTO dict_session VALUES(1,'session-a'),(3009,'legacy-session');
      INSERT INTO dict_role VALUES(1,'assistant');
      INSERT INTO agent_turn(session_id,turn,role_id,said) VALUES(1,7,1,'turn body');
      PRAGMA user_version=38;").unwrap();
    let sources = [
        "",
        "turn:session-a:7",
        "turn:session-a:99",
        "turn-range:session-a:7-9",
        "codex:last-assistant",
        "codex:session-a:turn:7",
        "codex:session-a:assistant:7",
        "session:3009 turn:4195",
        "agent_session:3009 turn:5606",
        "session:session-a",
        "codex session session-a turn 7",
        "codex session session-a turns 7-9",
        "codex session session-a",
        "a87db96b-81bb-4b55-9375-27b34f4af177 turn 147",
        "a87db96b-81bb-4b55-9375-27b34f4af177",
        "free provenance",
        "claude:session-a:assistant:7",
    ];
    for (index, source) in sources.iter().enumerate() {
        let body = format!("# Body {index}\nUnicode: λ\r\n\0end");
        let markdown = store.intern_markdown(&body, index as u64).unwrap();
        store
            .connection()
            .execute(
                "INSERT INTO agent_favorite VALUES(?1,?2,?3,?4,?5)",
                params![
                    index as i64 + 1,
                    markdown,
                    if index == 0 { None } else { Some("") },
                    source,
                    index as i64
                ],
            )
            .unwrap();
    }
    let before = favorite_bodies(store.connection());
    drop(store);
    let store = Store::open(path.clone()).unwrap();
    assert_eq!(store.schema_version().unwrap(), 40);
    assert_eq!(favorite_bodies(store.connection()), before);
    let rows=store.rows("SELECT source_kind,source_session,source_turn,source_turn_end,source_harness,source_role,source_codex_ref,source_text FROM agent_favorite ORDER BY favorite_id",vec![]).unwrap();
    let kinds: Vec<_> = rows
        .iter()
        .map(|row| row["source_kind"].as_str().unwrap())
        .collect();
    assert_eq!(
        kinds,
        [
            "empty",
            "turn",
            "missing_turn",
            "turn_range",
            "codex",
            "codex",
            "codex",
            "session",
            "agent_session",
            "session",
            "codex",
            "codex",
            "codex",
            "session",
            "session",
            "text",
            "turn"
        ]
    );
    for (row, source) in rows.iter().zip(sources) {
        assert_eq!(row["source_text"], source);
    }
    assert_eq!(rows[2]["source_session"], "session-a");
    assert_eq!(rows[2]["source_turn"], 99);
    assert_eq!(rows[3]["source_turn_end"], 9);
    assert_eq!(rows[4]["source_codex_ref"], "last-assistant");
    assert_eq!(rows[8]["source_session"], "legacy-session");
    assert_eq!(rows[11]["source_turn_end"], 9);
    let mood_before = store
        .rows("SELECT * FROM mood ORDER BY id", vec![])
        .unwrap();
    assert_eq!(
        mood_before,
        vec![
            serde_json::json!({"id":10,"name":"plain","template":"plain custom"}),
            serde_json::json!({"id":20,"name":"unga","template":"unga custom"}),
            serde_json::json!({"id":30,"name":"board","template":"board custom"})
        ]
    );
    assert_eq!(
        store.rows("SELECT * FROM agent_tag_link", vec![]).unwrap(),
        vec![serde_json::json!({"tag":"kept","source":"favorite:1","ts":2})]
    );
    assert!(store
        .connection()
        .execute("UPDATE mood SET name='unknown' WHERE id=10", [])
        .is_err());
    // Reopen and rebuild preserve migrated references, including the orphan marker.
    drop(store);
    let store = Store::open(path.clone()).unwrap();
    store.rebuild().unwrap();
    assert_eq!(favorite_bodies(store.connection()), before);
    assert_eq!(store.rows("SELECT source_kind,source_session,source_turn,source_turn_end,source_harness,source_role,source_codex_ref,source_text FROM agent_favorite ORDER BY favorite_id",vec![]).unwrap(),rows);
    assert_eq!(
        store
            .rows("SELECT * FROM mood ORDER BY id", vec![])
            .unwrap(),
        mood_before
    );
    drop(store);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn favorite_edits_replace_references_and_preserve_omitted_metadata() {
    let path = fixture_path("edit");
    let store = Store::open(path.clone()).unwrap();
    let id = store
        .favorite_add_typed(
            "body",
            Some("note"),
            &FavoriteSource::assistant("claude", "session-a", 7),
            1,
        )
        .unwrap();
    store.favorite_edit(id, Some("edited"), None).unwrap();
    let row = &store.query_favorite(id).unwrap()[0];
    assert_eq!(row["source_kind"], FavoriteSourceKind::Turn.as_str());
    assert_eq!(row["source_turn"], 7);
    store.favorite_edit(id, None, Some("")).unwrap();
    let row = &store.query_favorite(id).unwrap()[0];
    assert_eq!(row["source_kind"], "empty");
    assert_eq!(row["source_session"], serde_json::Value::Null);
    assert_eq!(row["source_turn"], serde_json::Value::Null);
    assert_eq!(row["body"], "body");
    assert_eq!(row["note"], "edited");
    assert!(!store
        .favorite_edit(999, None, Some("codex:last-assistant"))
        .unwrap());
    drop(store);
    std::fs::remove_file(path).unwrap();
}
