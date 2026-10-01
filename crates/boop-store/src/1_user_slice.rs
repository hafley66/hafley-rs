//! User-authored favorite references and the version 39 migration.
use crate::ident::{Row, Store, MOOD_SEEDS};
use anyhow::{bail, Result};
use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoodName {
    Plain,
    Unga,
    Board,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FavoriteSourceKind {
    Empty,
    Text,
    Session,
    AgentSession,
    Codex,
    Turn,
    TurnRange,
    MissingTurn,
}
impl FavoriteSourceKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Text => "text",
            Self::Session => "session",
            Self::AgentSession => "agent_session",
            Self::Codex => "codex",
            Self::Turn => "turn",
            Self::TurnRange => "turn_range",
            Self::MissingTurn => "missing_turn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FavoriteSource {
    pub kind: FavoriteSourceKind,
    pub session: Option<String>,
    pub turn: Option<i64>,
    pub turn_end: Option<i64>,
    pub harness: Option<String>,
    pub role: Option<String>,
    pub codex_ref: Option<String>,
    /// Exact user-entered provenance, including prose and legacy spelling.
    pub text: String,
}
impl FavoriteSource {
    pub fn parse(text: &str) -> Self {
        let mut source = Self {
            kind: if text.is_empty() {
                FavoriteSourceKind::Empty
            } else {
                FavoriteSourceKind::Text
            },
            session: None,
            turn: None,
            turn_end: None,
            harness: None,
            role: None,
            codex_ref: None,
            text: text.to_owned(),
        };
        let parts: Vec<_> = text.split(':').collect();
        match parts.as_slice() {
            ["turn", session, turn] if turn.parse::<i64>().is_ok() => {
                source.kind = FavoriteSourceKind::Turn;
                source.session = Some((*session).into());
                source.turn = turn.parse().ok();
            }
            ["turn-range", session, range] => {
                if let Some((start, end)) = range.split_once('-') {
                    if let (Ok(start), Ok(end)) = (start.parse::<i64>(), end.parse::<i64>()) {
                        source.kind = FavoriteSourceKind::TurnRange;
                        source.session = Some((*session).into());
                        source.turn = Some(start);
                        source.turn_end = Some(end);
                    }
                }
            }
            [harness, session, role, turn]
                if matches!(*harness, "codex" | "claude" | "kimi" | "opencode" | "omp") =>
            {
                source.kind = if *harness == "codex" {
                    FavoriteSourceKind::Codex
                } else {
                    FavoriteSourceKind::Turn
                };
                source.session = Some((*session).into());
                source.turn = turn.parse().ok();
                source.harness = Some((*harness).into());
                source.role = Some((*role).into());
                if let Some((start, end)) = turn.split_once('-') {
                    if let (Ok(start), Ok(end)) = (start.parse::<i64>(), end.parse::<i64>()) {
                        source.kind = FavoriteSourceKind::TurnRange;
                        source.turn = Some(start);
                        source.turn_end = Some(end);
                    }
                }
            }
            ["codex", reference] => {
                source.kind = FavoriteSourceKind::Codex;
                source.codex_ref = Some((*reference).into());
                source.harness = Some("codex".into());
            }
            ["session", session] if !session.contains(' ') => {
                source.kind = FavoriteSourceKind::Session;
                source.session = Some((*session).into());
            }
            [prefix, session_and_turn, turn] if matches!(*prefix, "session" | "agent_session") => {
                if let Some(session) = session_and_turn.strip_suffix(" turn") {
                    if let Ok(turn) = turn.parse::<i64>() {
                        source.kind = if *prefix == "session" {
                            FavoriteSourceKind::Session
                        } else {
                            FavoriteSourceKind::AgentSession
                        };
                        source.session = Some(session.into());
                        source.turn = Some(turn);
                    }
                }
            }
            _ => {
                let words: Vec<_> = text.split_whitespace().collect();
                let (session, rest, harness) = match words.as_slice() {
                    ["codex", "session", session, rest @ ..] => {
                        (Some(*session), rest, Some("codex"))
                    }
                    [session, rest @ ..] if uuid::Uuid::parse_str(session).is_ok() => {
                        (Some(*session), rest, None)
                    }
                    _ => (None, &[][..], None),
                };
                if let Some(session) = session {
                    source.kind = if harness.is_some() {
                        FavoriteSourceKind::Codex
                    } else {
                        FavoriteSourceKind::Session
                    };
                    source.session = Some(session.into());
                    source.harness = harness.map(str::to_owned);
                    match rest {
                        ["turn", turn] => source.turn = turn.parse().ok(),
                        ["turns", range] => {
                            if let Some((start, end)) = range.split_once('-') {
                                source.turn = start.parse().ok();
                                source.turn_end = end.parse().ok();
                            }
                        }
                        [] => (),
                        _ => {
                            source.kind = FavoriteSourceKind::Text;
                            source.session = None;
                            source.harness = None;
                        }
                    }
                }
            }
        }
        source
    }
    pub fn assistant(harness: &str, session: &str, turn: i64) -> Self {
        Self::parse(&format!("{harness}:{session}:assistant:{turn}"))
    }
}

pub const FAVORITE_SCHEMA: &str = "CREATE TABLE agent_favorite_v39 (
 favorite_id INTEGER PRIMARY KEY, markdown_id INTEGER NOT NULL, note TEXT,
 source_kind TEXT NOT NULL DEFAULT 'empty' CHECK (source_kind IN ('empty','text','session','agent_session','codex','turn','turn_range','missing_turn')),
 source_session TEXT, source_turn INTEGER, source_turn_end INTEGER,
 source_harness TEXT, source_role TEXT, source_codex_ref TEXT,
 source_text TEXT NOT NULL DEFAULT '', created_ts INTEGER NOT NULL);";

fn has_column(connection: &Connection, table: &str, column: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name=?2)",
        params![table, column],
        |row| row.get(0),
    )?)
}

impl Store {
    /// Called inside the schema owner's transaction. No markdown or tag rows are rewritten.
    pub(crate) fn migrate_user_slice(&self) -> Result<()> {
        let connection = self.connection();
        if has_column(connection, "agent_favorite", "source")? {
            connection.execute_batch("DROP VIEW IF EXISTS v_favorite;")?;
            connection.execute_batch(FAVORITE_SCHEMA)?;
            let rows = {
                let mut statement = connection.prepare("SELECT favorite_id, markdown_id, note, source, created_ts FROM agent_favorite ORDER BY favorite_id")?;
                let rows = statement.query_map([], |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, Option<String>>(2)?,
                        row.get::<_, String>(3)?,
                        row.get::<_, i64>(4)?,
                    ))
                })?;
                rows.collect::<rusqlite::Result<Vec<_>>>()?
            };
            for (id, markdown, note, text, ts) in rows {
                let mut source = self.resolve_favorite_source(&text)?;
                if source.kind == FavoriteSourceKind::Turn {
                    let exists: bool = connection.query_row("SELECT EXISTS(SELECT 1 FROM agent_turn t JOIN dict_session s ON s.id=t.session_id WHERE s.value=?1 AND t.turn=?2)", params![source.session, source.turn], |row| row.get(0))?;
                    if !exists {
                        source.kind = FavoriteSourceKind::MissingTurn;
                    }
                }
                connection.execute("INSERT INTO agent_favorite_v39 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)", params![id,markdown,note,source.kind.as_str(),source.session,source.turn,source.turn_end,source.harness,source.role,source.codex_ref,text,ts])?;
            }
            connection.execute_batch("DROP TABLE agent_favorite; ALTER TABLE agent_favorite_v39 RENAME TO agent_favorite;")?;
        }
        if has_column(connection, "mood", "name_id")? {
            connection.execute_batch("CREATE TABLE mood_v39 (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE CHECK (name IN ('plain','unga','board')), template TEXT NOT NULL);
              INSERT INTO mood_v39 SELECT mood.id, name.value, mood.template FROM mood JOIN dict_mood_name name ON name.id=mood.name_id;")?;
            let before: i64 =
                connection.query_row("SELECT count(*) FROM mood", [], |r| r.get(0))?;
            let after: i64 =
                connection.query_row("SELECT count(*) FROM mood_v39", [], |r| r.get(0))?;
            if before != after {
                bail!("mood migration would lose rows: {before} -> {after}");
            }
            connection.execute_batch(
                "DROP TABLE mood; ALTER TABLE mood_v39 RENAME TO mood; DROP TABLE dict_mood_name;",
            )?;
        }
        connection.execute_batch(crate::ident::CONTEXT_VIEW_SCHEMA)?;
        Ok(())
    }

    pub fn favorite_add(
        &self,
        body: &str,
        note: Option<&str>,
        source: &str,
        ts: u64,
    ) -> Result<i64> {
        self.favorite_add_typed(body, note, &self.resolve_favorite_source(source)?, ts)
    }
    pub fn favorite_add_typed(
        &self,
        body: &str,
        note: Option<&str>,
        source: &FavoriteSource,
        ts: u64,
    ) -> Result<i64> {
        let transaction = self.connection().unchecked_transaction()?;
        let markdown_id = self.intern_markdown(body, ts)?;
        self.connection().execute(
            "INSERT INTO agent_favorite (markdown_id,note,created_ts) VALUES (?1,?2,?3)",
            params![markdown_id, note, ts as i64],
        )?;
        let id = self.connection().last_insert_rowid();
        self.write_favorite_source(id, source)?;
        transaction.commit()?;
        Ok(id)
    }
    fn resolve_favorite_source(&self, text: &str) -> Result<FavoriteSource> {
        let mut source = FavoriteSource::parse(text);
        if matches!(
            source.kind,
            FavoriteSourceKind::Session | FavoriteSourceKind::AgentSession
        ) {
            if let Some(id) = source
                .session
                .as_deref()
                .and_then(|session| session.parse::<i64>().ok())
            {
                let session: Option<String> = self
                    .connection()
                    .query_row("SELECT value FROM dict_session WHERE id=?1", [id], |row| {
                        row.get(0)
                    })
                    .optional()?;
                if let Some(session) = session {
                    source.session = Some(session);
                }
            }
        }
        Ok(source)
    }

    pub(crate) fn set_favorite_source(&self, id: i64, source: &str) -> Result<()> {
        self.write_favorite_source(id, &self.resolve_favorite_source(source)?)?;
        Ok(())
    }
    fn write_favorite_source(&self, id: i64, source: &FavoriteSource) -> Result<usize> {
        Ok(self.connection().execute("UPDATE agent_favorite SET source_kind=?2, source_session=?3, source_turn=?4, source_turn_end=?5, source_harness=?6, source_role=?7, source_codex_ref=?8, source_text=?9 WHERE favorite_id=?1", params![id,source.kind.as_str(),source.session,source.turn,source.turn_end,source.harness,source.role,source.codex_ref,source.text])?)
    }
    pub fn favorite_edit(&self, id: i64, note: Option<&str>, source: Option<&str>) -> Result<bool> {
        let transaction = self.connection().unchecked_transaction()?;
        let changed = self.connection().execute(
            "UPDATE agent_favorite SET note=COALESCE(?2,note) WHERE favorite_id=?1",
            params![id, note],
        )?;
        if let Some(source) = source {
            self.set_favorite_source(id, source)?;
        }
        transaction.commit()?;
        Ok(changed == 1)
    }
    pub(crate) fn seed_moods(&self) -> Result<()> {
        for (name, template) in MOOD_SEEDS {
            self.connection().execute(
                "INSERT OR IGNORE INTO mood (name,template) VALUES (?1,?2)",
                params![name, template],
            )?;
        }
        Ok(())
    }
    pub fn mood_names(&self) -> Result<Vec<String>> {
        let mut statement = self
            .connection()
            .prepare("SELECT name FROM mood ORDER BY name")?;
        let names = statement
            .query_map([], |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(names)
    }
}

#[cfg(feature = "agent-read")]
impl Store {
    /// Favorites newest-first, body included.
    pub fn query_favorites(&self, limit: Option<u64>) -> Result<Vec<Row>> {
        let mut sql = String::from(
            "SELECT f.favorite_id, f.note, f.source_text AS source, f.source_kind, f.source_session, f.source_turn, f.source_turn_end, f.source_harness, f.source_role, f.source_codex_ref, f.created_ts, m.bytes, m.body
               FROM agent_favorite f
               JOIN markdown_cache m ON m.markdown_id = f.markdown_id
              ORDER BY f.favorite_id DESC",
        );
        if let Some(limit) = limit {
            sql.push_str(&format!(" LIMIT {limit}"));
        }
        self.rows(&sql, Vec::new())
    }

    /// One favorite by id, body included; empty when the id names none.
    pub fn query_favorite(&self, id: i64) -> Result<Vec<Row>> {
        self.rows(
            "SELECT f.favorite_id, f.note, f.source_text AS source, f.source_kind, f.source_session, f.source_turn, f.source_turn_end, f.source_harness, f.source_role, f.source_codex_ref, f.created_ts, m.bytes, m.body
               FROM agent_favorite f
               JOIN markdown_cache m ON m.markdown_id = f.markdown_id
              WHERE f.favorite_id = ?1",
            vec![rusqlite::types::Value::Integer(id)],
        )
    }
}
