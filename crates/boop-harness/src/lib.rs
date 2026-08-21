//! What each harness on this machine writes and how to re-open it: the
//! transcript format per harness, its session roots, the identity ladder that
//! names the caller, and the worktree a spawn runs in.

pub mod harness;
pub mod identity;
pub mod registry;
pub mod worktree;

pub use harness::{
    supervisor_command, sync_session, sync_session_with_pid, Capabilities, Harness, Ingested,
    KnownSession, KnownSessions, NativeSessionRef, NativeTuiPlan, NativeTuiSpec, OneShotSpec,
    ReadChunk, SendOutcome, SessionRef, SpawnSpec,
};
pub use identity::Identity;
pub use registry::Registry;
