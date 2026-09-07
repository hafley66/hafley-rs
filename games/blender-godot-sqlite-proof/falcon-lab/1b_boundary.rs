//! Engine-neutral numeric presentation rows, exposed through SQLite over recycled Rust slots.
use core_labs::sql::{FrameRing, Generation};
use rusqlite::vtab::{
    sqlite3_vtab, sqlite3_vtab_cursor, Context, Filters, IndexInfo, Module, VTab, VTabConnection,
    VTabCursor,
};
use rusqlite::{Connection, Result};
use serde::{Deserialize, Serialize};
use std::{
    borrow::Cow,
    ffi::CStr,
    os::raw::c_int,
    sync::{Arc, RwLock},
};

pub const WINDOW: i64 = 32;
pub const SLOTS: usize = 3;
pub const ROW_CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub tick: i64,
    pub kind: i64,
    pub entity: i64,
    pub values: [f64; 24],
}
impl Row {
    pub fn new(tick: i64, kind: i64, entity: i64) -> Self {
        Self {
            tick,
            kind,
            entity,
            values: [0.0; 24],
        }
    }
}
pub type Ring = Arc<RwLock<FrameRing<Row>>>;
const MODULE: Module<Table> = Module::eponymous_only_module();
#[repr(C)]
pub struct Table {
    base: sqlite3_vtab,
    ring: Ring,
}
unsafe impl<'vtab> VTab<'vtab> for Table {
    type Aux = Ring;
    type Cursor = Cursor;
    fn connect(
        _: &mut VTabConnection,
        aux: Option<&Ring>,
        _: &[u8],
        _: &[u8],
        _: &[u8],
        _: &[&[u8]],
    ) -> Result<(Cow<'static, CStr>, Self)> {
        Ok((Cow::Borrowed(c"CREATE TABLE x(tick INTEGER,kind INTEGER,entity INTEGER,generation INTEGER,v0 REAL,v1 REAL,v2 REAL,v3 REAL,v4 REAL,v5 REAL,v6 REAL,v7 REAL,v8 REAL,v9 REAL,v10 REAL,v11 REAL,v12 REAL,v13 REAL,v14 REAL,v15 REAL,v16 REAL,v17 REAL,v18 REAL,v19 REAL,v20 REAL,v21 REAL,v22 REAL,v23 REAL)"),Self{base:Default::default(),ring:aux.unwrap().clone()}))
    }
    fn best_index(&self, info: &mut IndexInfo) -> Result<bool> {
        info.set_estimated_cost(1024.0);
        Ok(true)
    }
    fn open(&'vtab mut self) -> Result<Cursor> {
        Ok(Cursor {
            base: Default::default(),
            ring: self.ring.clone(),
            generation: None,
            row: 0,
        })
    }
}
#[repr(C)]
pub struct Cursor {
    base: sqlite3_vtab_cursor,
    ring: Ring,
    generation: Option<Arc<Generation<Row>>>,
    row: usize,
}
unsafe impl VTabCursor for Cursor {
    fn filter(&mut self, _: c_int, _: Option<&str>, _: &Filters<'_>) -> Result<()> {
        self.generation = Some(self.ring.read().unwrap().current());
        self.row = 0;
        Ok(())
    }
    fn next(&mut self) -> Result<()> {
        self.row += 1;
        Ok(())
    }
    fn eof(&self) -> bool {
        self.generation
            .as_ref()
            .is_none_or(|g| self.row >= g.rows.len())
    }
    fn column(&self, ctx: &mut Context, col: c_int) -> Result<()> {
        let g = self.generation.as_ref().unwrap();
        let r = &g.rows[self.row];
        match col {
            0 => ctx.set_result(&r.tick),
            1 => ctx.set_result(&r.kind),
            2 => ctx.set_result(&r.entity),
            3 => ctx.set_result(&(g.id as i64)),
            4..=27 => ctx.set_result(&r.values[col as usize - 4]),
            _ => unreachable!(),
        }
    }
    fn rowid(&self) -> Result<i64> {
        Ok(self.row as i64)
    }
}

pub struct Boundary {
    pub ring: Ring,
    pub db: Connection,
    pub generation: u64,
    history: Vec<Row>,
    scratch: Vec<Row>,
    pub layout: Vec<(usize, usize)>,
}
impl Boundary {
    pub fn new() -> Result<Self> {
        let ring = Arc::new(RwLock::new(FrameRing::with_row_capacity(
            SLOTS,
            ROW_CAPACITY,
        )));
        ring.write().unwrap().publish_rows(0, &[]).unwrap();
        let db = Connection::open_in_memory()?;
        db.create_module(c"presentation", &MODULE, Some(ring.clone()))?;
        db.execute_batch("CREATE VIEW frame_state AS SELECT generation,tick,v0 AS action,v1 AS pose,v5 AS damage,v6 AS hits,v9 AS predicted FROM presentation WHERE kind=0;
            CREATE VIEW target_state AS SELECT generation,tick,v0 AS x,v1 AS y,v2 AS z,v3 AS vx,v4 AS vy,v5 AS vz,v6 AS hitstun,v7 AS phase,v8 AS grounded FROM presentation WHERE kind=1;
            CREATE VIEW hurtboxes AS SELECT * FROM presentation WHERE kind=2;
            CREATE VIEW attacks AS SELECT * FROM presentation WHERE kind=3;")?;
        let layout = ring.read().unwrap().slot_layout();
        Ok(Self {
            ring,
            db,
            generation: 0,
            history: Vec::with_capacity(ROW_CAPACITY),
            scratch: Vec::with_capacity(ROW_CAPACITY),
            layout,
        })
    }
    pub fn reader(&self) -> Result<Connection> {
        let db = Connection::open_in_memory()?;
        db.create_module(c"presentation", &MODULE, Some(self.ring.clone()))?;
        Ok(db)
    }
    /// Replace all replayed ticks in one publication, never exposing an intermediate replay state.
    pub fn publish(&mut self, frames: &[Vec<Row>]) -> bool {
        let first = frames.first().unwrap()[0].tick;
        let last = frames.last().unwrap()[0].tick;
        let total = self
            .history
            .iter()
            .filter(|r| r.tick < first && r.tick > last - WINDOW)
            .count()
            + frames.iter().map(Vec::len).sum::<usize>();
        if total > ROW_CAPACITY {
            return false;
        }
        self.scratch.clear();
        self.scratch.extend(
            self.history
                .iter()
                .copied()
                .filter(|r| r.tick < first && r.tick > last - WINDOW),
        );
        for frame in frames {
            self.scratch.extend_from_slice(frame);
        }
        let next = self.generation + 1;
        if self
            .ring
            .write()
            .unwrap()
            .publish_rows(next, &self.scratch)
            .is_none()
        {
            return false;
        }
        std::mem::swap(&mut self.history, &mut self.scratch);
        self.generation = next;
        assert_eq!(self.layout, self.ring.read().unwrap().slot_layout());
        true
    }
}

pub fn read_row(row: &rusqlite::Row<'_>) -> Result<(u64, Row)> {
    let mut r = Row::new(row.get(0)?, row.get(1)?, row.get(2)?);
    for (i, v) in r.values.iter_mut().enumerate() {
        *v = row.get(i + 4)?;
    }
    Ok((row.get::<_, i64>(3)? as u64, r))
}
pub fn read_frame(db: &Connection, tick: i64) -> Result<(u64, Vec<Row>)> {
    let mut statement = db.prepare("SELECT * FROM presentation WHERE tick=?1")?;
    let records = statement
        .query_map([tick], read_row)?
        .collect::<Result<Vec<_>>>()?;
    let generation = records.first().expect("frame missing").0;
    assert!(records.iter().all(|r| r.0 == generation));
    Ok((generation, records.into_iter().map(|r| r.1).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn oversized_publication_preserves_generation() {
        let mut b = Boundary::new().unwrap();
        assert!(b.publish(&[vec![Row::new(0, 0, 0)]]));
        assert!(!b.publish(&[vec![Row::new(1, 0, 0); ROW_CAPACITY + 1]]));
        assert_eq!(b.generation, 1);
        assert_eq!(read_frame(&b.db, 0).unwrap().1, vec![Row::new(0, 0, 0)]);
        assert_eq!(b.layout, b.ring.read().unwrap().slot_layout());
    }
    #[test]
    fn all_slots_pinned_refuses_then_recycles_without_reallocation() {
        let mut b = Boundary::new().unwrap();
        let mut pins = Vec::new();
        for tick in 0..3 {
            assert!(b.publish(&[vec![Row::new(tick, 0, 0)]]));
            pins.push(b.ring.read().unwrap().current());
        }
        let before = b.generation;
        assert!(!b.publish(&[vec![Row::new(3, 0, 0)]]));
        assert_eq!(b.generation, before);
        drop(pins.remove(0));
        assert!(b.publish(&[vec![Row::new(3, 0, 0)]]));
        assert_eq!(b.layout, b.ring.read().unwrap().slot_layout());
        assert_eq!(pins[0].rows.last().unwrap().tick, 1);
    }
}
