//! The boop database: the relational store over `~/.agent/boop.db`, its schema
//! and migrations, the transcript projection that fills it, and the typed
//! queries a caller reads it back through.
//!
//! The four reads a host needs are `Store::query_status`, `Store::query_facts`,
//! `Store::query_sessions` and `Store::usage_report`; `plans/boop-instant-v2-contract.md`
//! pins the exact call per view.

#[cfg(feature = "agent-read")]
pub mod _0_session_graph;
#[cfg(feature = "agent-read")]
pub mod activity;
pub mod bus;
pub mod event;
pub mod ident;
pub mod proc;
#[cfg(feature = "agent-read")]
pub mod query;
pub mod rows;
pub mod runtime;
pub mod session;
pub mod tail;
#[cfg(any(test, feature = "testing"))]
pub mod testing;
pub mod tmux;
pub mod trail;
#[cfg(feature = "agent-read")]
pub mod usage;

#[cfg(feature = "agent-read")]
pub use _0_session_graph::{
    load_agent_session_graph, load_agent_session_graph_with_runtime, AgentSessionEdge,
    AgentSessionGraph, AgentSessionGraphQuery, AgentSessionGraphRuntime, AgentSessionIdentity,
    AgentSessionNode, AgentShellNode, LoadAgentSessionGraph, AGENT_SESSION_GRAPH_SCHEMA_VERSION,
};
#[cfg(feature = "agent-read")]
pub use activity::{ActivityCount, ActivityScope, ToolResultAvailability};
pub use ident::{
    Store, SyncStat, TraceErrorRow, TraceEvent, TraceEventRow, TRACE_EVENT_RETENTION_LIMIT,
};
#[cfg(feature = "agent-read")]
pub use query::{FactKind, FactQuery};
pub use rows::{
    CommandRow, EdgeRow, FactCursor, FetchRow, LiveSpanRow, SessionRow, StatusRow, TouchRow,
    TurnRow, UsageRow,
};
pub use runtime::{
    runtime_snapshot, runtime_snapshot_now, AgentRuntimeRow, CompletionRecord, LaneRuntime,
    MailboxCounts, ProcessIdentity, ProcessLiveness, ResolvedRoute, RuntimeDiagnostic,
    RuntimeLiveness, RuntimeSnapshotInput, TmuxLiveness, WorktreeCoordinates,
};
#[cfg(feature = "agent-read")]
pub use usage::{GroupBy, UsageQuery};

/// Open the default store at `~/.agent/boop.db`.
pub fn open_default() -> anyhow::Result<Store> {
    Store::open(Store::default_path()?)
}
