//! Engine-neutral numeric presentation rows, exposed through SQLite over recycled Rust slots.
use core_labs::sql::{FrameRing, Generation};
use rusqlite::vtab::{
    Context, Filters, IndexInfo, Module, VTab, VTabConnection, VTabCursor, sqlite3_vtab,
    sqlite3_vtab_cursor,
};
use rusqlite::{Connection, Result};
use std::{
    borrow::Cow,
    ffi::CStr,
    os::raw::c_int,
    sync::{Arc, RwLock},
};

pub use contracts::WINDOW;
pub const SLOTS: usize = contracts::SLOTS as usize;
pub const ROW_CAPACITY: usize = contracts::ROW_CAPACITY as usize;

#[allow(dead_code)]
#[path = "contracts/2_presentation_auto.rs"]
pub(crate) mod contracts;
pub use contracts::Row;
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
        reader_for(&self.ring)
    }
    /// Replace all replayed ticks in one publication, never exposing an intermediate replay state.
    #[tracing::instrument(target = "falcon::sql", level = "trace", skip_all, fields(generation = self.generation + 1, frames = frames.len()))]
    pub fn publish(&mut self, frames: &[Vec<Row>]) -> bool {
        self.publish_rows(frames.iter().flatten().copied()).is_ok()
    }

    fn publish_rows(&mut self, rows: impl Iterator<Item = Row> + Clone) -> contracts::PublishResult {
        use contracts::BoundaryError;
        let first = rows.clone().next().ok_or(BoundaryError::InvalidPayload)?.tick;
        let mut last = first;
        let mut count = 0;
        for row in rows.clone() {
            if row.tick < last || row.values.iter().any(|v| !v.is_finite()) {
                return Err(BoundaryError::InvalidPayload);
            }
            last = row.tick;
            count += 1;
        }
        let oldest = last.saturating_sub(WINDOW);
        let total = self
            .history
            .iter()
            .filter(|r| r.tick < first && r.tick > oldest)
            .count()
            + count;
        if total > ROW_CAPACITY {
            tracing::warn!(target: "falcon::sql", rows = total, capacity = ROW_CAPACITY, reason = "row_capacity", "publication_refused");
            return Err(BoundaryError::Capacity);
        }
        self.scratch.clear();
        self.scratch.extend(
            self.history
                .iter()
                .copied()
                .filter(|r| r.tick < first && r.tick > oldest),
        );
        self.scratch.extend(rows);
        let next = self.generation.checked_add(1).ok_or(BoundaryError::Capacity)?;
        if self
            .ring
            .write()
            .unwrap()
            .publish_rows(next, &self.scratch)
            .is_none()
        {
            tracing::warn!(target: "falcon::sql", rows = total, reason = "slots_pinned", "publication_refused");
            return Err(BoundaryError::SlotsPinned);
        }
        std::mem::swap(&mut self.history, &mut self.scratch);
        self.generation = next;
        tracing::trace!(target: "falcon::sql", generation = next, rows = total, "published");
        assert_eq!(self.layout, self.ring.read().unwrap().slot_layout());
        Ok(contracts::GenerationId { epoch: 0, generation: next })
    }
}

impl contracts::RowPublisher for Boundary {
    #[tracing::instrument(target = "falcon::sql", level = "trace", skip_all, fields(rows = rows.len()))]
    fn publish(&mut self, rows: &[Row]) -> contracts::PublishResult {
        self.publish_rows(rows.iter().copied())
    }
}

// Initial adapter retains the existing SQL materialization. Epoch zero denotes
// this local Boundary lifetime; cross-process epoch negotiation is not wired yet.
impl contracts::FrameQuery for Boundary {
    fn read_frame(&mut self, tick: i64, output: &mut [Row]) -> contracts::ReadResult {
        let (generation, rows) = read_frame(&self.db, tick).map_err(|error| match error {
            rusqlite::Error::QueryReturnedNoRows => contracts::BoundaryError::MissingFrame,
            _ => contracts::BoundaryError::InvalidPayload,
        })?;
        if rows.len() > output.len() {
            return Err(contracts::BoundaryError::Capacity);
        }
        output[..rows.len()].copy_from_slice(&rows);
        Ok(contracts::FrameRead {
            id: contracts::GenerationId { epoch: 0, generation },
            rows_written: rows.len() as u32,
        })
    }
}

#[tracing::instrument(target = "falcon::sql", level = "trace", skip_all)]
pub fn reader_for(ring: &Ring) -> Result<Connection> {
    let db = Connection::open_in_memory()?;
    db.create_module(c"presentation", &MODULE, Some(ring.clone()))?;
    Ok(db)
}

pub fn read_row(row: &rusqlite::Row<'_>) -> Result<(u64, Row)> {
    let mut r = Row::new(row.get(0)?, row.get(1)?, row.get(2)?);
    for (i, v) in r.values.iter_mut().enumerate() {
        *v = row.get(i + 4)?;
    }
    Ok((row.get::<_, i64>(3)? as u64, r))
}
#[tracing::instrument(target = "falcon::sql", level = "trace", skip_all, fields(tick))]
pub fn read_frame(db: &Connection, tick: i64) -> Result<(u64, Vec<Row>)> {
    let mut statement = db.prepare("SELECT * FROM presentation WHERE tick=?1")?;
    let records = statement
        .query_map([tick], read_row)?
        .collect::<Result<Vec<_>>>()?;
    let generation = records.first().ok_or(rusqlite::Error::QueryReturnedNoRows)?.0;
    assert!(records.iter().all(|r| r.0 == generation));
    Ok((generation, records.into_iter().map(|r| r.1).collect()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn named_row_adapters_preserve_packed_values_and_untouched_tail() {
        use contracts::{FrameValues, TargetValues, HurtValues, AttackValues, pack_rows};
        let source = std::array::from_fn(|i| i as f64 + 0.25);
        let mut rows = Vec::new();
        for kind in 0..4 {
            let input = Row { tick: 91, kind, entity: 7, values: source };
            let mut output = Row { values: [-99.0; 24], ..input };
            let width = match kind {
                0 => {
                    let value = FrameValues::from_row(&input).unwrap();
                    assert_eq!((value.damage, value.confirmed), (5.25, 18.25));
                    value.write_row(&mut output);
                    assert_eq!(value.into_row(91, 7).values[19..], [0.0; 5]);
                    19
                }
                1 => { TargetValues::from_row(&input).unwrap().write_row(&mut output); 9 }
                2 => { HurtValues::from_row(&input).unwrap().write_row(&mut output); 24 }
                3 => { AttackValues::from_row(&input).unwrap().write_row(&mut output); 6 }
                _ => unreachable!(),
            };
            assert_eq!(&output.values[..width], &source[..width]);
            assert_eq!(&output.values[width..], &[-99.0; 24][width..]);
            assert_eq!((output.tick, output.kind, output.entity), (91, kind, 7));
            rows.push(input);
        }
        assert_eq!(FrameValues::from_row(&rows[1]), None);
        let packed = pack_rows(&rows);
        let expected: Vec<_> = (0..4).flat_map(|kind| [91.0, kind as f64, 7.0].into_iter().chain(source)).collect();
        assert_eq!(packed, expected);
    }

    #[test]
    fn generated_publisher_preserves_replay_and_refusal_semantics() {
        use contracts::{BoundaryError, GenerationId, RowPublisher};
        let mut b = Boundary::new().unwrap();
        let mut pins = Vec::new();
        for tick in 0..3 {
            assert_eq!(RowPublisher::publish(&mut b, &[Row::new(tick, 0, 0)]),
                Ok(GenerationId { epoch: 0, generation: tick as u64 + 1 }));
            pins.push(b.ring.read().unwrap().current());
        }
        for (rows, error) in [
            (vec![], BoundaryError::InvalidPayload),
            (vec![Row::new(2, 0, 0), Row::new(1, 0, 0)], BoundaryError::InvalidPayload),
            (vec![Row::new(3, 0, 0); ROW_CAPACITY + 1], BoundaryError::Capacity),
            (vec![Row::new(3, 0, 0)], BoundaryError::SlotsPinned),
        ] {
            assert_eq!(RowPublisher::publish(&mut b, &rows), Err(error));
            assert_eq!(b.generation, 3);
            assert_eq!(read_frame(&b.db, 2).unwrap().1, vec![Row::new(2, 0, 0)]);
        }
        drop(pins.remove(0));
        let replacement = [Row::new(1, 0, 42), Row::new(2, 0, 43)];
        assert_eq!(RowPublisher::publish(&mut b, &replacement),
            Ok(GenerationId { epoch: 0, generation: 4 }));
        assert_eq!(read_frame(&b.db, 0).unwrap().1, vec![Row::new(0, 0, 0)]);
        assert_eq!(read_frame(&b.db, 1).unwrap().1, vec![replacement[0]]);
        assert_eq!(read_frame(&b.db, 2).unwrap().1, vec![replacement[1]]);
        assert_eq!(pins[1].rows.last().unwrap().entity, 0);
        assert_eq!(b.layout, b.ring.read().unwrap().slot_layout());
    }

    #[test]
    fn generated_query_contract_preserves_rows_and_errors() {
        use contracts::{BoundaryError, FrameQuery, FrameRead, GenerationId};
        let mut b = Boundary::new().unwrap();
        let expected = [Row::new(7, 0, 0), Row::new(7, 1, 1)];
        assert!(b.publish(&[expected.to_vec()]));
        let sentinel = Row::new(-10, -20, -30);
        let mut output = [sentinel; 3];
        assert_eq!(FrameQuery::read_frame(&mut b, 7, &mut output[..1]), Err(BoundaryError::Capacity));
        assert_eq!(output, [sentinel; 3]);
        assert_eq!(FrameQuery::read_frame(&mut b, 99, &mut output), Err(BoundaryError::MissingFrame));
        assert_eq!(output, [sentinel; 3]);
        assert_eq!(FrameQuery::read_frame(&mut b, 7, &mut output), Ok(FrameRead {
            id: GenerationId { epoch: 0, generation: 1 }, rows_written: 2,
        }));
        assert_eq!(output, [expected[0], expected[1], sentinel]);
        b.db = Connection::open_in_memory().unwrap();
        assert_eq!(FrameQuery::read_frame(&mut b, 7, &mut output), Err(BoundaryError::InvalidPayload));
        assert_eq!(output, [expected[0], expected[1], sentinel]);
    }

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
