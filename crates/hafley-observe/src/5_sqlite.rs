use rusqlite::trace::{TraceEvent, TraceEventCodes};
use rusqlite::{Connection, StatementStatus};

pub const SQLITE_TARGET: &str = "sqlite";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StatementCounters {
    pub vm_step: i32,
    pub fullscan_step: i32,
    pub sort: i32,
    pub autoindex: i32,
    pub reprepare: i32,
    pub run: i32,
    pub mem_used: i32,
}

/// What each counter means when it is nonzero. SQLite documents these as
/// opaque integers; the names below are the reason anyone would read them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatementFinding {
    TableScan,
    TemporaryBtreeSort,
    RuntimeIndexBuilt,
    SchemaChangedUnderStatement,
}

impl StatementFinding {
    pub fn as_str(self) -> &'static str {
        match self {
            StatementFinding::TableScan => "table scan, no index served this statement",
            StatementFinding::TemporaryBtreeSort => {
                "temporary b-tree sort, an ORDER BY or GROUP BY had no index"
            }
            StatementFinding::RuntimeIndexBuilt => {
                "transient index built at run time, a permanent index is missing"
            }
            StatementFinding::SchemaChangedUnderStatement => {
                "statement was reprepared, the schema changed beneath it"
            }
        }
    }
}

impl StatementCounters {
    pub fn read(statement: &rusqlite::trace::StmtRef<'_>) -> Self {
        StatementCounters {
            vm_step: statement.get_status(StatementStatus::VmStep),
            fullscan_step: statement.get_status(StatementStatus::FullscanStep),
            sort: statement.get_status(StatementStatus::Sort),
            autoindex: statement.get_status(StatementStatus::AutoIndex),
            reprepare: statement.get_status(StatementStatus::RePrepare),
            run: statement.get_status(StatementStatus::Run),
            mem_used: statement.get_status(StatementStatus::MemUsed),
        }
    }

    pub fn findings(&self) -> Vec<StatementFinding> {
        let mut findings = Vec::new();
        if self.fullscan_step > 0 {
            findings.push(StatementFinding::TableScan);
        }
        if self.sort > 0 {
            findings.push(StatementFinding::TemporaryBtreeSort);
        }
        if self.autoindex > 0 {
            findings.push(StatementFinding::RuntimeIndexBuilt);
        }
        if self.reprepare > 0 {
            findings.push(StatementFinding::SchemaChangedUnderStatement);
        }
        findings
    }
}

/// `vm_step` is the deterministic cost of a statement: the same input yields
/// the same count on every machine, unlike elapsed time.
fn emit(event: TraceEvent<'_>) {
    match event {
        TraceEvent::Stmt(statement, expanded) => {
            tracing::trace!(
                target: SQLITE_TARGET,
                sql = %statement.sql(),
                expanded = %expanded,
                "statement begins"
            );
        }
        TraceEvent::Profile(statement, elapsed) => {
            let counters = StatementCounters::read(&statement);
            let findings = counters.findings();
            tracing::debug!(
                target: SQLITE_TARGET,
                sql = %statement.sql(),
                nanos = elapsed.as_nanos() as u64,
                vm_step = counters.vm_step,
                fullscan_step = counters.fullscan_step,
                sort = counters.sort,
                autoindex = counters.autoindex,
                reprepare = counters.reprepare,
                run = counters.run,
                mem_used = counters.mem_used,
                "statement finished"
            );
            for finding in findings {
                tracing::warn!(
                    target: SQLITE_TARGET,
                    sql = %statement.sql(),
                    vm_step = counters.vm_step,
                    "{}",
                    finding.as_str()
                );
            }
        }
        _ => {}
    }
}

pub fn instrument(connection: &Connection) {
    connection.trace_v2(
        TraceEventCodes::SQLITE_TRACE_STMT | TraceEventCodes::SQLITE_TRACE_PROFILE,
        Some(emit),
    );
}

pub fn silence(connection: &Connection) {
    connection.trace_v2(TraceEventCodes::empty(), None);
}

/// The planner's own account of a statement, one row per plan node.
pub fn query_plan(connection: &Connection, sql: &str) -> rusqlite::Result<Vec<String>> {
    connection
        .prepare(&format!("EXPLAIN QUERY PLAN {sql}"))?
        .query_map([], |row| row.get::<_, String>(3))?
        .collect()
}
