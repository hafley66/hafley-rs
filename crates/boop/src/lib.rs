//! boop as a library: the relational store over `~/.agent/boop.db` plus the
//! harness adapters that fill it. Linkable from a Rust host (the tauri side of
//! instant) so a caller runs the queries in-process instead of shelling out.
//!
//! The four reads a host needs are `Store::query_status`, `Store::query_facts`,
//! `Store::query_sessions` and `Store::usage_report`; `plans/boop-instant-v2-contract.md`
//! pins the exact call per view.

#[cfg(feature = "agent-read")]
pub mod _0_session_graph;
#[cfg(feature = "agent-read")]
pub mod activity;
pub mod bus;
pub mod channel;
#[cfg(feature = "agent-read")]
pub mod chat;
pub mod concatmap;
pub mod config;
pub mod event;
pub mod harness;
pub mod ident;
pub mod identity;
pub mod lane;
pub mod mailwait;
pub mod proc;
#[cfg(feature = "agent-read")]
pub mod query;
pub mod registry;
pub mod rows;
pub mod runtime;
#[cfg(feature = "agent-read")]
pub mod summary;
pub mod supervise;
pub mod tail;
#[cfg(test)]
pub(crate) mod test_support;
pub mod tmux;
pub mod trail;
#[cfg(feature = "agent-read")]
pub mod usage;
pub mod worktree;

#[cfg(feature = "agent-read")]
pub use _0_session_graph::{
    load_agent_session_graph, load_agent_session_graph_with_runtime, AgentSessionEdge,
    AgentSessionGraph, AgentSessionGraphQuery, AgentSessionGraphRuntime, AgentSessionIdentity,
    AgentSessionNode, AgentShellNode, LoadAgentSessionGraph, AGENT_SESSION_GRAPH_SCHEMA_VERSION,
};
#[cfg(feature = "agent-read")]
pub use activity::{ActivityCount, ActivityScope, ToolResultAvailability};
pub use ident::{Store, SyncStat};
#[cfg(feature = "agent-read")]
pub use query::{FactKind, FactQuery};
pub use registry::Registry;
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
pub use summary::{
    agent_summary, agent_summary_now, AgentSummary, AgentSummaryActivity, AgentSummaryAgent,
    AgentSummaryQuery, AGENT_SUMMARY_SCHEMA_VERSION,
};
#[cfg(feature = "agent-read")]
pub use usage::{GroupBy, UsageQuery};

/// Open the default store at `~/.agent/boop.db`.
pub fn open_default() -> anyhow::Result<Store> {
    Store::open(Store::default_path()?)
}
