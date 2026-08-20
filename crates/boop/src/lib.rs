//! boop as a library: the relational store over `~/.agent/boop.db` plus the
//! harness adapters that fill it. Linkable from a Rust host (the tauri side of
//! instant) so a caller runs the queries in-process instead of shelling out.
//!
//! The four reads a host needs are `Store::query_status`, `Store::query_facts`,
//! `Store::query_sessions` and `Store::usage_report`; `plans/boop-instant-v2-contract.md`
//! pins the exact call per view.

/// The build this binary is: the package version and the commit it came from,
/// `<version> (<short sha>[-dirty])`. `unknown` where no checkout answered.
/// Printed by `--version` and stamped into every lane spawn, so a lane death
/// names the binary that ran it.
pub const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " (", env!("BOOP_BUILD_SHA"), ")");

/// Just the commit stamp half of `BUILD`.
pub const BUILD_SHA: &str = env!("BOOP_BUILD_SHA");

// The store's modules keep their old paths so a library caller (and this
// crate's own `crate::ident::...` spellings) is unchanged by the crate split.
#[cfg(feature = "agent-read")]
pub use boop_store::_0_session_graph;
#[cfg(feature = "agent-read")]
pub use boop_store::activity;
pub use boop_store::{bus, event, proc, rows, runtime, session, tail, tmux, trail};
#[cfg(feature = "agent-read")]
pub use boop_store::{query, usage};
#[cfg(test)]
pub(crate) use boop_store::testing as test_support;

/// The store's ident module plus the two harness-driven sync entry points,
/// which need an adapter and so live one crate up.
pub mod ident {
    pub use boop_store::ident::*;

    pub use boop_harness::harness::{sync_session, sync_session_with_pid};
}

pub use boop_acp::channel;
pub use boop_harness::{harness, identity, registry, worktree};
pub use boop_proc::{concatmap, config, host, inbox, lane, mailwait, supervise};
#[cfg(feature = "agent-read")]
pub mod chat;
pub mod debug;
#[cfg(feature = "agent-read")]
pub mod summary;

pub use boop_store::open_default;
#[cfg(feature = "agent-read")]
pub use boop_store::{
    load_agent_session_graph, load_agent_session_graph_with_runtime, ActivityCount, ActivityScope,
    AgentSessionEdge, AgentSessionGraph, AgentSessionGraphQuery, AgentSessionGraphRuntime,
    AgentSessionIdentity, AgentSessionNode, AgentShellNode, FactKind, FactQuery, GroupBy,
    LoadAgentSessionGraph, ToolResultAvailability, UsageQuery, AGENT_SESSION_GRAPH_SCHEMA_VERSION,
};
pub use boop_store::{
    AgentRuntimeRow, CommandRow, CompletionRecord, EdgeRow, FactCursor, FetchRow, LaneRuntime,
    LiveSpanRow, MailboxCounts, ProcessIdentity, ProcessLiveness, ResolvedRoute, RuntimeDiagnostic,
    RuntimeLiveness, RuntimeSnapshotInput, SessionRow, StatusRow, Store, SyncStat, TmuxLiveness,
    TouchRow, TraceErrorRow, TraceEvent, TraceEventRow, TurnRow, UsageRow, WorktreeCoordinates,
    TRACE_EVENT_RETENTION_LIMIT,
};
pub use boop_harness::Registry;
#[cfg(feature = "agent-read")]
pub use summary::{
    agent_summary, agent_summary_now, AgentSummary, AgentSummaryActivity, AgentSummaryAgent,
    AgentSummaryQuery, AGENT_SUMMARY_SCHEMA_VERSION,
};
