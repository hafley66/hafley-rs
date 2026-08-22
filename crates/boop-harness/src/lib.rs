//! What each harness on this machine writes and how to re-open it: the
//! transcript format per harness, its session roots, the identity ladder that
//! names the caller, and the worktree a spawn runs in.

pub mod door;
pub mod harness;
pub mod identity;
pub mod live;
pub mod registry;
pub mod worktree;

pub use harness::{
    supervisor_command, sync_session, sync_session_with_pid, Capabilities, ControlCapabilities,
    Harness, HarnessId, Ingested, KnownSession, KnownSessions, LanePolicy, MailPolicy,
    NativeChildEvent, NativeSessionRef, NativeTuiPlan, NativeTuiSpec, OneShotSpec, ReadChunk,
    SendOutcome, SessionRef, SpawnSpec, VariantSupport,
};
pub use door::{Delivered, Door, IdleNotice};
pub use identity::Identity;
pub use live::{DoorAddress, LiveSession, LiveSessions, LiveStatus};
pub use registry::Registry;
