//! Layer 1 seam: the `Multiplexer` trait and its `Tmux` implementation live in
//! the `boop-mux` crate. This module is a thin re-export plus the one shared
//! `&dyn Multiplexer` instance the CLI binds to.
pub use boop_mux::{
    kill_test_server, parse_event, rows_from_capture, ControlClient, ControlEvent, History,
    LiveSessions, Multiplexer, Notification, Screen, TerminalRow, TerminalSize, TerminalSnapshot,
    TerminalTarget, Tmux,
};

/// The one shared multiplexer instance. Stateless (the socket is a per-call
/// argument), so one `&dyn Multiplexer` serves every call site.
pub fn mux() -> &'static dyn Multiplexer {
    static MUX: Tmux = Tmux;
    &MUX
}
