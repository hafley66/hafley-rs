use rusqlite::{types::Value, Connection, Result};
use std::{collections::HashMap, ffi::c_int};

/// Rows held in memory before the collector spills to its shadow table.
/// Protects the process from one transaction that inserts a whole file.
pub const STAGED_ROWS: usize = 10_000;

/// Bytes of payload held in memory before spilling. Same protection, for wide rows.
pub const STAGED_BYTES: usize = 8 << 20;

/// Direction of a row-level event. An UPDATE arrives as `Delete` of the old
/// image then `Insert` of the new image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sign {
    Insert,
    Delete,
}

impl Sign {
    /// The integer a source trigger writes into the collector's `__sign` column.
    pub(crate) const fn as_integer(self) -> i64 {
        match self {
            Sign::Insert => 1,
            Sign::Delete => -1,
        }
    }

    pub(crate) fn from_integer(value: i64) -> Option<Self> {
        match value {
            1 => Some(Sign::Insert),
            -1 => Some(Sign::Delete),
            _ => None,
        }
    }
}

/// One row-level event, captured in xUpdate, delivered in a batch.
#[derive(Clone, Debug, PartialEq)]
pub struct RowChange {
    pub table: String,
    pub sign: Sign,
    /// New image for `Insert`, old image for `Delete`, in declared column order.
    pub values: Vec<Value>,
    /// Position within the transaction, monotone from 0. `ROLLBACK TO` restores
    /// the counter, so a delivered batch carries contiguous numbers.
    pub sequence: u64,
}

/// Called once per transaction at xSync with every surviving change in sequence
/// order. Ordinary-table writes are legal inside it; an empty batch is skipped.
pub trait BulkTrigger: 'static {
    fn on_batch(&mut self, db: &Connection, batch: &[RowChange]) -> Result<()>;
}

/// Times SQLite entered each collector callback since the last reset.
/// `watch` zeroes it after its own DDL.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Counts {
    pub begin: u64,
    pub savepoint: u64,
    pub release: u64,
    pub rollback_to: u64,
    pub update: u64,
    pub sync: u64,
    pub commit: u64,
    pub rollback: u64,
}

/// Position the collector returns to when SQLite rolls back to `savepoint`.
#[derive(Clone, Copy, Debug)]
struct Mark {
    savepoint: c_int,
    staged_len: usize,
    staged_bytes: usize,
    spilled_rows: usize,
    next_sequence: u64,
}

pub(crate) struct Collector {
    /// Taken out for the duration of `on_batch`, so a statement that re-enters
    /// the collector from inside the callback finds no borrow.
    pub(crate) trigger: Option<Box<dyn BulkTrigger>>,
    /// Column count per watched table, used to cut the trigger's padding off.
    pub(crate) arity: HashMap<String, usize>,
    /// Widest watched table, which is the count of `__value` columns.
    pub(crate) width: usize,
    pub(crate) counts: Counts,
    pub(crate) staged: Vec<RowChange>,
    staged_bytes: usize,
    spilled_rows: usize,
    marks: Vec<Mark>,
    next_sequence: u64,
    staged_row_cap: usize,
    staged_byte_cap: usize,
}

impl Collector {
    pub(crate) fn new(
        trigger: Box<dyn BulkTrigger>,
        arity: HashMap<String, usize>,
        staged_row_cap: usize,
        staged_byte_cap: usize,
    ) -> Self {
        let width = arity.values().copied().max().unwrap_or(0);
        Self {
            trigger: Some(trigger),
            arity,
            width,
            counts: Counts::default(),
            staged: Vec::new(),
            staged_bytes: 0,
            spilled_rows: 0,
            marks: Vec::new(),
            next_sequence: 0,
            staged_row_cap,
            staged_byte_cap,
        }
    }

    pub(crate) fn spilled_rows(&self) -> usize {
        self.spilled_rows
    }

    /// Numbers the change and reports where it goes. `Some(change)` means the
    /// caller must write it to the shadow table; the spill is already counted.
    pub(crate) fn admit(
        &mut self,
        table: String,
        sign: Sign,
        values: Vec<Value>,
    ) -> Option<RowChange> {
        let change = RowChange {
            table,
            sign,
            values,
            sequence: self.next_sequence,
        };
        self.next_sequence += 1;
        let bytes = row_bytes(&change);
        let room = self.staged.len() < self.staged_row_cap
            && self.staged_bytes + bytes <= self.staged_byte_cap;
        if room {
            self.staged_bytes += bytes;
            self.staged.push(change);
            return None;
        }
        self.spilled_rows += 1;
        Some(change)
    }

    pub(crate) fn begin(&mut self) {
        self.counts.begin += 1;
        self.reset();
    }

    pub(crate) fn rollback(&mut self) {
        self.counts.rollback += 1;
        self.reset();
    }

    /// Hands the memory half of the batch to the caller and clears transaction
    /// state, so the shadow-table read and `on_batch` run with no borrow held.
    pub(crate) fn drain_staged(&mut self) -> (Vec<RowChange>, usize) {
        self.counts.sync += 1;
        let staged = std::mem::take(&mut self.staged);
        let spilled = self.spilled_rows;
        self.staged_bytes = 0;
        self.spilled_rows = 0;
        self.marks.clear();
        (staged, spilled)
    }

    pub(crate) fn savepoint(&mut self, savepoint: c_int) {
        self.counts.savepoint += 1;
        // SQLite numbers savepoints as a stack, so a repeat of an index retires
        // the older mark at that index.
        self.marks.retain(|mark| mark.savepoint < savepoint);
        self.marks.push(Mark {
            savepoint,
            staged_len: self.staged.len(),
            staged_bytes: self.staged_bytes,
            spilled_rows: self.spilled_rows,
            next_sequence: self.next_sequence,
        });
    }

    /// RELEASE invalidates the named savepoint and everything inside it.
    pub(crate) fn release(&mut self, savepoint: c_int) {
        self.counts.release += 1;
        self.marks.retain(|mark| mark.savepoint < savepoint);
    }

    /// ROLLBACK TO leaves the named savepoint open, so its mark survives. No
    /// mark at that index means the savepoint predates the first write here.
    pub(crate) fn rollback_to(&mut self, savepoint: c_int) {
        self.counts.rollback_to += 1;
        let restored = self
            .marks
            .iter()
            .rposition(|mark| mark.savepoint == savepoint)
            .map(|at| self.marks[at]);
        self.marks.retain(|mark| mark.savepoint <= savepoint);
        match restored {
            Some(mark) => {
                self.staged.truncate(mark.staged_len);
                self.staged_bytes = mark.staged_bytes;
                self.spilled_rows = mark.spilled_rows;
                self.next_sequence = mark.next_sequence;
            }
            None => self.reset(),
        }
    }

    fn reset(&mut self) {
        self.staged.clear();
        self.staged_bytes = 0;
        self.spilled_rows = 0;
        self.marks.clear();
        self.next_sequence = 0;
    }
}

/// Interleaves the memory half and the shadow-table half of one batch. Both
/// arrive sorted by sequence.
pub(crate) fn merge(staged: Vec<RowChange>, spilled: Vec<RowChange>) -> Vec<RowChange> {
    let mut merged = Vec::with_capacity(staged.len() + spilled.len());
    let mut staged = staged.into_iter().peekable();
    let mut spilled = spilled.into_iter().peekable();
    // Bound: staged.len() + spilled.len(). Each step moves one row out of one
    // of two finite vectors and neither is refilled.
    while staged.peek().is_some() || spilled.peek().is_some() {
        let from_staged = match (staged.peek(), spilled.peek()) {
            (Some(left), Some(right)) => left.sequence <= right.sequence,
            (Some(_), None) => true,
            _ => false,
        };
        let next = if from_staged {
            staged.next()
        } else {
            spilled.next()
        };
        merged.extend(next);
    }
    merged
}

fn row_bytes(change: &RowChange) -> usize {
    std::mem::size_of::<RowChange>()
        + change.table.len()
        + change.values.iter().map(value_bytes).sum::<usize>()
}

fn value_bytes(value: &Value) -> usize {
    std::mem::size_of::<Value>()
        + match value {
            Value::Text(text) => text.len(),
            Value::Blob(bytes) => bytes.len(),
            _ => 0,
        }
}
