use rusqlite::{Connection, Result};
use rusqlite::vtab::{sqlite3_vtab, sqlite3_vtab_cursor, Context, Filters, IndexInfo, Module, VTab, VTabConnection, VTabCursor};
use std::{borrow::Cow, ffi::CStr, os::raw::c_int, sync::{Arc, RwLock}};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRow { pub frame: i64, pub entity: i64, pub value: i64 }

#[derive(Debug)]
pub struct Generation<R = EntityRow> { pub id: u64, pub rows: Vec<R> }

#[derive(Debug)]
pub struct FrameRing<R = EntityRow> {
    slots: Vec<Arc<Generation<R>>>,
    next: usize,
    published: Option<usize>,
}

impl<R> FrameRing<R> {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self { slots: (0..capacity).map(|_| Arc::new(Generation { id: 0, rows: Vec::new() })).collect(), next: 0, published: None }
    }

    pub fn publish(&mut self, generation: Generation<R>) -> std::result::Result<Arc<Generation<R>>, Generation<R>> {
        for offset in 0..self.slots.len() {
            let index = (self.next + offset) % self.slots.len();
            if Arc::strong_count(&self.slots[index]) == 1 {
                let slot = Arc::get_mut(&mut self.slots[index]).unwrap();
                slot.id = generation.id;
                slot.rows.clear();
                slot.rows.extend(generation.rows);
                self.published = Some(index);
                self.next = (index + 1) % self.slots.len();
                return Ok(self.slots[index].clone());
            }
        }
        Err(generation)
    }

    pub fn current(&self) -> Arc<Generation<R>> { self.slots[self.published.unwrap()].clone() }

    pub fn with_row_capacity(capacity: usize, rows: usize) -> Self {
        let mut ring = Self::new(capacity);
        for slot in &mut ring.slots { Arc::get_mut(slot).unwrap().rows.reserve_exact(rows); }
        ring
    }

    pub fn slot_layout(&self) -> Vec<(usize, usize)> {
        self.slots.iter().map(|slot| (slot.rows.as_ptr() as usize, slot.rows.capacity())).collect()
    }

    /// Publish into preallocated storage; pinned slots and oversized rows refuse publication.
    pub fn publish_rows(&mut self, id: u64, rows: &[R]) -> Option<Arc<Generation<R>>> where R: Clone {
        for offset in 0..self.slots.len() {
            let index = (self.next + offset) % self.slots.len();
            if Arc::strong_count(&self.slots[index]) == 1 && rows.len() <= self.slots[index].rows.capacity() {
                let slot = Arc::get_mut(&mut self.slots[index]).unwrap();
                slot.id = id; slot.rows.clear(); slot.rows.extend_from_slice(rows);
                self.published = Some(index); self.next = (index + 1) % self.slots.len();
                return Some(self.slots[index].clone());
            }
        }
        None
    }
}

pub type SharedRing = Arc<RwLock<FrameRing>>;
const MODULE: Module<RingTab> = Module::eponymous_only_module();

#[repr(C)]
pub struct RingTab { base: sqlite3_vtab, ring: SharedRing }

unsafe impl<'vtab> VTab<'vtab> for RingTab {
    type Aux = SharedRing;
    type Cursor = RingCursor;

    fn connect(_: &mut VTabConnection, aux: Option<&SharedRing>, _: &[u8], _: &[u8], _: &[u8], _: &[&[u8]]) -> Result<(Cow<'static, CStr>, Self)> {
        Ok((Cow::Borrowed(c"CREATE TABLE x(frame INTEGER, entity INTEGER, value INTEGER, generation INTEGER)"), Self { base: sqlite3_vtab::default(), ring: aux.unwrap().clone() }))
    }

    fn best_index(&self, info: &mut IndexInfo) -> Result<bool> {
        info.set_estimated_cost(10.0);
        Ok(true)
    }

    fn open(&'vtab mut self) -> Result<RingCursor> {
        Ok(RingCursor { base: sqlite3_vtab_cursor::default(), generation: None, row: 0, ring: self.ring.clone() })
    }
}

#[repr(C)]
pub struct RingCursor {
    base: sqlite3_vtab_cursor,
    generation: Option<Arc<Generation>>,
    row: usize,
    ring: SharedRing,
}

unsafe impl VTabCursor for RingCursor {
    fn filter(&mut self, _: c_int, _: Option<&str>, _: &Filters<'_>) -> Result<()> {
        self.generation = Some(self.ring.read().unwrap().current());
        self.row = 0;
        Ok(())
    }
    fn next(&mut self) -> Result<()> { self.row += 1; Ok(()) }
    fn eof(&self) -> bool { self.generation.as_ref().is_none_or(|g| self.row >= g.rows.len()) }
    fn column(&self, ctx: &mut Context, column: c_int) -> Result<()> {
        let generation = self.generation.as_ref().unwrap();
        let row = &generation.rows[self.row];
        match column { 0 => ctx.set_result(&row.frame), 1 => ctx.set_result(&row.entity), 2 => ctx.set_result(&row.value), 3 => ctx.set_result(&(generation.id as i64)), _ => unreachable!() }
    }
    fn rowid(&self) -> Result<i64> { Ok(self.row as i64) }
}

pub fn register_ring(db: &Connection, ring: SharedRing) -> Result<()> {
    db.create_module(c"ring_rows", &MODULE, Some(ring))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(frame: i64, value_base: i64) -> Vec<EntityRow> {
        (0..3).map(|entity| EntityRow { frame, entity, value: value_base + entity }).collect()
    }

    #[test]
    fn ordinary_table_retains_bounded_complete_frames() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE frames(frame INTEGER, entity INTEGER, value INTEGER, PRIMARY KEY(frame, entity)) WITHOUT ROWID")?;
        for frame in 0..10_i64 {
            let tx = db.transaction()?;
            for row in rows(frame, frame * 10) { tx.execute("INSERT INTO frames VALUES (?1, ?2, ?3)", (row.frame, row.entity, row.value))?; }
            tx.execute("DELETE FROM frames WHERE frame < ?1", [frame - 3])?;
            tx.commit()?;
        }
        assert_eq!(db.query_row("SELECT COUNT(DISTINCT frame) FROM frames", [], |r| r.get::<_, i64>(0))?, 4);
        assert_eq!(db.query_row("SELECT COUNT(*) FROM frames", [], |r| r.get::<_, i64>(0))?, 12);
        Ok(())
    }

    #[test]
    fn preallocated_rowid_slots_stay_bounded_have_no_indexes_and_rollback() -> Result<()> {
        let mut db = Connection::open_in_memory()?;
        db.execute_batch("CREATE TABLE hot(frame INTEGER, entity INTEGER, value INTEGER); INSERT INTO hot(rowid, frame, entity, value) VALUES (1,0,0,0),(2,0,1,0),(3,0,2,0),(4,0,3,0)")?;
        assert_eq!(db.query_row("SELECT count(*) FROM sqlite_schema WHERE type='index' AND tbl_name='hot'", [], |r| r.get::<_, i64>(0))?, 0);
        for frame in 1..20_i64 {
            db.execute("UPDATE hot SET frame=?1, entity=rowid-1, value=?1*100+rowid", [frame])?;
            assert_eq!(db.query_row("SELECT count(*) FROM hot", [], |r| r.get::<_, i64>(0))?, 4);
        }
        let before: i64 = db.query_row("SELECT sum(value) FROM hot", [], |r| r.get(0))?;
        let tx = db.transaction()?;
        tx.execute("UPDATE hot SET value=-1", [])?;
        tx.rollback()?;
        assert_eq!(db.query_row("SELECT sum(value) FROM hot", [], |r| r.get::<_, i64>(0))?, before);
        Ok(())
    }

    #[test]
    fn held_vtab_cursor_pins_immutable_generation_across_slot_reuse() -> Result<()> {
        let ring = Arc::new(RwLock::new(FrameRing::new(2)));
        let first = ring.write().unwrap().publish(Generation { id: 1, rows: rows(1, 100) }).unwrap();
        let first_ptr = first.rows.as_ptr();
        let first_capacity = first.rows.capacity();
        let db = Connection::open_in_memory()?;
        register_ring(&db, ring.clone())?;
        let mut statement = db.prepare("SELECT generation, frame, entity, value FROM ring_rows()")?;
        let mut cursor = statement.query([])?;
        let row = cursor.next()?.unwrap();
        let first_row = (row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?, row.get::<_, i64>(3)?);
        assert_eq!(first_row, (1, 1, 0, 100));
        ring.write().unwrap().publish(Generation { id: 2, rows: rows(2, 200) }).unwrap();
        ring.write().unwrap().publish(Generation { id: 3, rows: rows(3, 300) }).unwrap();
        assert_eq!(Arc::strong_count(&first), 3);
        let remaining = cursor.mapped(|r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))).collect::<Result<Vec<_>>>()?;
        assert_eq!(remaining, vec![(1, 1, 1, 101), (1, 1, 2, 102)]);
        assert_eq!(Arc::strong_count(&first), 2);
        drop(first);
        drop(statement);
        assert_eq!(db.query_row("SELECT generation FROM ring_rows() LIMIT 1", [], |r| r.get::<_, i64>(0))?, 3);
        let reused = ring.write().unwrap().publish(Generation { id: 4, rows: rows(4, 400) }).unwrap();
        assert_eq!(reused.rows.as_ptr(), first_ptr);
        assert_eq!(reused.rows.capacity(), first_capacity);
        Ok(())
    }
}
