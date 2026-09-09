# Lift, extend, lift, stop

Predecessor: `104_tasks.md`; its G1-G7/P1-P4 dispositions and evidence remain.
User approved exactly one lift, one new thing, one lift, then stop for review.

| ID | State | Scope and terminal condition |
| --- | --- | --- |
| L1 | Done | Shared replay/restore harness consumed by Falcon; just test-reuse and scoped buffer_proof test pass. |
| N1 | Done | Held attack consumes at 25 only; release 64/repress 65 consumes at 25/65. Both scoped Falcon tests pass with GGRS/SQL and six clone/JSON/binary restore checkpoints. |
| L2 | Done | Shared text/panel/repetition consumed by original and held proofs; core/WASM checks pass, both MP4s recorded and inspected. See receipt below. |

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

L1 committed `ad3cf57`. N1 adds proof coverage of existing edge-detector semantics;
it does not modify the runtime policy. Both panels use window 8. At tick 65 the
held path enters Fall and the repressed path enters Fair. Tape inputs are retained
in the receipt for labels; no inferred input labels. Recording follows in L2.

## Final receipt

N1 commit: `a8c2f01`. L2 moves text rendering from `2_main.rs` into
`shared/capture/src/1_comparison.rs`, alongside panel placement; shared Capture
owns presentation repetition. Font8x8 dependency moves with its consumer. Shared
tests cover panel coordinates and text translation; Falcon assertions remain local.
API/lifetime details are in `../../shared/0_reuse.md`.

Executed `just test core`: 29 Falcon tests, 2 capture tests, 2 rollback tests,
3 input tests and existing Redux/generator/controller gates pass. Receipt:
`.workflow/test-8j55ek/receipt.json`; final rerun is `.workflow/test.json`.
`just test-simulation-wasm` passed. No browser runtime/deployment/network tests
were added. The existing lab input policy is unchanged.

Executed `just held-proof` with Metal access. It preserves `.workflow/buffer/`
and records both scenarios into `.workflow/buffer-held/`. New `held-proof.mp4`:
H.264 960x540, 60 fps, 540 frames, 9 seconds, 709,941 bytes.
SHA-256: `ee3d9dc7e4fafe9c966eb0bf042b89e08be67414bc40d4633122b24223737af6`.
Inspected `held-comparison.png` at tick 67: left Fall pose 3, one consumption at
25; right Fair pose 3, two consumptions at 25/65. Both show actual held buttons,
GGRS load 60 and eight advances. Original comparison also inspected at tick 28.
Each new tape checks 760 suffix ticks per restore method, 172 GGRS loads and
1,384 advances. Raw receipts and ffprobe JSON are beside the MP4.

Original MP4 hash remains
`dcfbcc1cbc7838d1fc46a39776825479e80396660237b097216ca277bbfa62f5`.
L1/N1/L2 complete. Stop for review; G2 and remaining P gates retain their status.
