# Increment 2: input-driven actions and GGRS correction

Run `bash 9_run_rollback.sh` in this directory, or invoke it by absolute path.
The preceding `5_falcon_knee.mp4` remains preserved.

## Executed scenario

Two GGRS 0.13.0 P2P sessions communicate over a deterministic in-memory message
transport. Peer A owns Falcon's input. Peer B owns a neutral second input slot and
predicts Falcon's remote input when messages are unavailable. This exercises GGRS
protocol requests and snapshots without using OS UDP or an external network.

Input pulses: jump at tick 60; attack at tick 78. The world responds to rising
input edges and retains action, animation clock, jump origin, prior input, hit
latch, damage, hit count, last-hit tick, and current pose in each snapshot.
PM data supplies the poses and attack parameters. Root travel remains the
preceding fixture's approximate trajectory.

A-to-B packets sent during ticks 78–96 are queued until tick 97. B initially
predicts no attack and remains in its jump action. At tick 91, A has inflicted
18 damage while B still has zero. On tick 97 GGRS requests restoration of state
78, replays 19 previous ticks, and advances the current tick. Both complete
world states then match for every remaining tick. Each has one hit and 18 damage.

## On-screen state

Each pane reports simulation tick, global confirmed-through tick, source action
and pose frame, applied Falcon input, input prediction/confirmation status,
airborne/grounded state, contact, transport hold/delivery, restore frame,
replayed ticks, total rollback loads, damage, hit count, and whole-state equality.

Labels are derived from captured world states and GGRS request/input status.
The transport banner describes the common A-to-B queue condition. Confirmation
uses GGRS's session-wide confirmed frame; the current input's status is separate.

Playback repeats ordinary ticks twice and event ticks 60, 78, 91, 97, and 179
60 times each. These are presentation holds; no extra simulation ticks run.
Each pane has 650 encoded frames at 60 FPS (10.833 seconds).

## Verification

- Full on-time world equals direct tick-input execution on all 180 ticks.
- Delayed world equals on-time world on every tick from 97 onward.
- Predicted missing attack produces the expected temporary damage divergence.
- Actual LoadGameState and multiple AdvanceFrame requests occur on correction.
- Final accumulated damage is 18 with one hit on each peer.
- Zero-delay sessions match each other on every tick.
- Repeated delayed runs produce identical per-tick world states.
- Existing PM decoding/contact fixture tests remain passing.
- Clippy passes with warnings denied for lab targets.
- GPU readback checks Falcon and target geometry in both pane captures.

## Artifacts

- `10_peer0.mp4`, `10_peer1.mp4`: actual GPU captures per peer.
- `11_rollback_trace.json`: world snapshots and request/input diagnostics per tick.
- `12_rollback.mp4`: side-by-side H.264 composite.
- `13_rollback_frames.png`: inspection montage.

No Godot/Bevy, full costume mesh, exact PM movement, knockback, hitlag, SQLite,
real-network behavior, or cross-machine determinism is established by this test.
