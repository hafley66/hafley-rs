//! CLI flag shaping for the shared request-scoped fast receiver policy.
use super::cli::{Cmd, FastArgs, FileArgs, Ryi};
use hafley_scm::read::lang::fast_receivers::{FastReceivers, ScopedFastReceivers};

pub(super) fn scoped_policy(ryi: &Ryi) -> Result<ScopedFastReceivers, String> {
    let selected = match &ryi.cmd {
        Some(Cmd::Fast(args)) => args.fast_receivers.as_deref(),
        Some(Cmd::Graph(args)) => args.fast_receivers.as_deref(),
        _ => ryi.file.fast_receivers.as_deref(),
    };
    FastReceivers::scoped_override(selected)
}

pub(super) fn file_args_from_fast(fast: FastArgs) -> FileArgs {
    FileArgs {
        inputs: fast.inputs,
        sqlite: fast.sqlite,
        lines: fast.lines,
        fast_receivers: fast.fast_receivers,
        ..FileArgs::default()
    }
}
