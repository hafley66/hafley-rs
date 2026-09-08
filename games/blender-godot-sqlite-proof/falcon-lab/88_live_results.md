# Live cross-process Godot presentation

## Hop 1: live attachment

Two independently clocked GGRS UDP peers retain simulation ownership. Each
optionally exports its current SQL-read rows through the existing bincode 2
serde codec to one bounded latest-only file. A same-directory atomic rename
publishes a complete snapshot. Godot's gdext adapter imports the newest snapshot
into its own existing SQLite FrameRing/vtab, checks rows exactly, and reuses the
existing mesh packing/upload acknowledgement path. It starts no simulation worker.

No new dependency was added. Existing bincode/serde, Rust filesystem APIs,
rusqlite/core-labs, gdext, and Godot supply the implementation. This is local
filesystem IPC. It does not share process-local SQLite connections or Arc slots.
The payload is bounded at 256 KiB and 1,024 rows; version and row tick consistency
are checked. Binary encoding preserves f64 bits. There is one committed file and
at most one staging file per producer, with no presentation queue. Source writes
are synchronous and can incur filesystem latency; no allocation or I/O-free claim
is made. Files are not fsynced, so crash durability is outside this proof.

In the executed baseline, source PID 51295 fed Godot PID 50481. Godot acknowledged
128 generations, skipped 72, and displayed tick 199. All acknowledgement digests
matched the recorded source rows, and local SQL and mesh roundtrips were exact.
The usual 200 corrected peer pairs and 360 golden states passed. All 16 Rust
library tests passed in 12.26s, including latest replacement and f64 bit retention.

`85_live_godot.mp4` contains inspected live Godot GPU output: 435 frames,
7.25 seconds, 960x540 H.264. MovieMaker omits periods when the renderer produces
no frame, so movie duration differs from source wall time. Wall time, source
generation, imported generation, skipped generations, state, damage, prediction,
confirmation, and restore/replay metadata are labeled from actual observations.

The control files are initial/lifecycle coordination only. The renderer has no
input/control channel into the authoritative simulation. Snapshots contain only
the current frame's presentation rows, not the full corrected historical window.
Reattaching recovers the latest display, not missed animations or every event.

## Hop 2: pause, restart, and cold attach

The recorded lifecycle run OS-stopped Godot PID 62571 for 801.42 ms, confirmed
process state T, and observed no new Godot acknowledgements while stopped.
Peer progress moved from [51,50] to [71,70]. The resumed renderer read source
tick 70 immediately, skipping 19 intermediate generations.

Godot then exited normally at tick 120. Replacement PID 62704 first imported
source tick 132/generation 133 into local SQL generation 1. Both peer counters
advanced during the restart, from [123,120] to [135,132]. The replacement reached
tick 199. After the relay had waited for both peers to exit successfully, a
third Godot PID 62779 cold-attached to the persisted final snapshot. It imported
source generation 200 into local generation 1, with a single exact acknowledgement.

All 168 lifecycle-run acknowledgements matched source-row digests, local SQLite,
and mesh uploads. All 200 corrected peer pairs and 360 golden states passed.
`86_godot_restart.mp4` joins the actual first and replacement renderer recordings;
`87_godot_cold_attach.mp4` shows the post-peer-exit attachment. Both were inspected.
The join does not synthesize frames during the process pause or restart gap.

The restart is graceful renderer termination, not an unhandled crash. Simulation
process restart, remote filesystems, host failure, and shared-memory IPC remain
untested. Transport progress is renderer-independent here; filesystem stalls
can still delay the synchronous source adapter.

## Hop 3: repeatable suite and offline evidence

Run `sh 89_run_live.sh` from this lab. It runs the 16-test Rust library suite,
builds with two jobs, runs the relay and audit tests, checks the Godot script,
executes baseline and lifecycle scenarios, validates both archives, and encodes
three new MP4s in a fresh temporary directory. `--skip-build` skips only Cargo
tests/build after a successful current-source build; Python tests and all live
checks still execute. No checked-in recordings are overwritten.

The finalized wrapper completed end to end. Its repeated baseline acknowledged
198 generations; the lifecycle renderers acknowledged 101, 67, and 1. The live
replacement first displayed tick 131, and the cold renderer first displayed
tick 199. Both scenarios passed full-state/row/acknowledgement checks. All three
encoded streams were verified as 960x540 H.264 with nonzero frame counts.
The audit unit test accepts exact source rows and rejects changed source data.

`90_live_verification.json` contains initial and repeated lifecycle evidence.
`91_live_evidence.tar.gz` preserves both baseline and lifecycle runs' original
and corrected peer histories, source SQL rows, packet audits, and renderer
acknowledgements. Initial-run Godot logs are included. `92_live_frames.png` is
the inspected pause/restart montage. Prior MP4s and prior evidence remain intact.

To verify without running Godot or the network experiment:

```sh
# Extract 91_live_evidence.tar.gz into a new temporary directory, then enter
# an extracted run, such as falcon-live-lifecycle.fnuSX7.
python3 /absolute/path/to/falcon-lab/84_live_controller.py --verify-archive
```

This executes the built Rust full-state verifier and recomputes source-row
digests against the stored Godot acknowledgements. It checks recorded mesh
acknowledgement flags; a new GPU upload requires the live suite. Verification
from a freshly extracted archive also passed. Paths inside the archive retain
their original run-directory names.

The controller owns only its spawned Godot and relay processes. Cleanup resumes
a stopped renderer before termination, and the relay handles SIGTERM by cleaning
up its own peer children. Per-run lifecycle watchdogs are bounded at 40 seconds.
The completed runs left no experiment processes running.
