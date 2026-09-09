# Lift, extend, lift, stop

Predecessor: `104_tasks.md`; its G1-G7/P1-P4 dispositions and evidence remain.
User approved exactly one lift, one new thing, one lift, then stop for review.

| ID | State | Scope and terminal condition |
| --- | --- | --- |
| L1 | Done | Shared replay/restore harness consumed by Falcon; just test-reuse and scoped buffer_proof test pass. |
| N1 | In progress | Add held-versus-repressed attack scenario using existing buffer/Redux; prove held input consumes once and a fresh press can consume again; commit tests. |
| L2 | Pending | Extract comparison layout/text/repetition into shared/capture, preserve original proof and record/inspect new MP4; run core/WASM checks, commit and stop. |

L1 API: `proof::run<S>(config, tape, distance) -> Result<Trace<S::State>, GgrsError>`;
`proof::restore_suffixes<S>(config, tape, states, checkpoints, restore) -> Result<usize, E>`.
Tape rows are ticks, columns stable player handles. Inputs/config are borrowed;
the trace owns post-tick states. Each restored state lives for one suffix.
The harness compares complete state after each GGRS correction and restored step.
Caller owns codecs, scenario checkpoints, expected game outcomes and SQL/mesh IO.
No new reducer/session abstraction or network behavior.

N1 uses the existing edge detector and imported interruptibility flags. It adds
a scenario, not source-game fidelity or a new controller policy. Preserve the
original zero/eight-buffer recording in `.workflow/buffer/`.

L2 will retain game label content and pixel assertions in Falcon. Shared code
owns geometry placement/text rasterization and repeated presentation frames.
No mirror, deployment, TypeSpec emitter, storage or sealed-application edits.

Carried: G1 deferred; G2-G6 pending, G7 on deck; P1/P2/P4 pending, P3 partial.
I1-I3 done, receipts in predecessor and `108_buffer_receipt.md`. Following this
bounded sequence G2 stays on deck; do not start it during this run.

L1 immediate checkpoint checks exposed statig's decoded initialization flag in
derived equality. Hook-free Buffer now compares logical phase; immediate decoded
equality and replay tests pass. Negative entry-hook regression remains intact.
Harness counter fixture tests two players, codec errors and corrupt restored state.
Disk exhaustion interrupted the ledger write; scoped cargo clean -p falcon-lab
removed rebuildable package artifacts before retry. Sources/recordings retained.
