# Independent UDP process proof

Run `sh 77_run_processes.sh`, or `sh 77_run_processes.sh --skip-build` after a
successful current-source release build. Outputs stay in a fresh temporary
directory. The controller exits after 25 seconds if the peers fail to finish;
it terminates only its own children and escalates to killing those children
after a two-second grace period. Rendering happens after simulation validation.

## Reused implementation

GGRS 0.13.0 (MIT) supplies `UdpNonBlockingSocket::bind_to_port`, its bincode wire
codec, `SessionBuilder`, input acknowledgement, prediction, and rollback requests.
Source inspected in the pinned Cargo registry package's `network/udp_socket.rs`
and `sessions/p2p_session.rs`. No new Cargo dependency was added. Python standard
library sockets/selectors/heapq/subprocess implement only the bounded lab relay
and child lifecycle. The existing Rapier/Parry simulation, decoded fighter data,
SQL FrameRing/vtab, wire geometry, wgpu capture, and ffmpeg H.264 encoder are reused.

The request executor now accepts any GGRS Config with the same Input and State,
plus a callback after each authoritative advance. Existing in-process callers
use an empty callback. Process peers use it to retain corrected full World
snapshots, replacing earlier speculative history during replay.

## Ownership and sequencing

Each child owns one GGRS peer, one simulation, one SQLite boundary, and an
independent Instant clock. Peer 0's pacing is 40 ms; peer 1's is 41 ms. Both use
the existing 1/60 simulation timestep and execute 200 inputs. Peer 0 supplies
the fighter inputs, peer 1 supplies zero; the sandbag remains the target.
This does not add a second controllable fighter.

The controller launches two distinct PIDs and relays handshake datagrams.
After both report readiness, a shared future start time releases their independent
timers. There is no per-tick controller barrier. UDP carries all GGRS input
traffic; ready/start files carry only initial coordination. The controller does
not advance either simulation. Each child polls for two seconds after its local
timeline ends and requires confirmation through input frame 199.

All destinations and relay binding are loopback. GGRS's supplied socket binds
wildcard IPv4; a wrapper accepts only messages from the configured loopback relay.
The parent selects available ephemeral peer ports before spawn; another process
could claim a released port, in which case the experiment fails rather than
silently using another endpoint. NAT, authentication, and remote hosts are untested.

## Fault rule and reproducibility

After the start epoch, each direction cycles 20/30/40 ms delay by packet sequence,
and every 17th packet is dropped. A-to-B traffic received during wall time
3.050..4.070 seconds is held until at least 4.070 seconds, suppressing the attack
input long enough to show prediction disagreement. The queue is bounded at 4,096
datagrams. Handshake traffic before the start epoch passes without faults.

The rule and policy boundary test are deterministic. OS scheduling and GGRS
retransmit timing affect packet counts, precise arrival order, and recovery tick;
bit-identical network schedules are not claimed. `network.json` records packet
sequence, direction, receive/delivery time, size, drop decision, and queue peak.
Repeated runs must satisfy the same full-state and recovery assertions.

## Assertions and display

Every child publishes actual original/replayed presentation rows through its
SQLite vtab and reads the current tick back exactly. The verifier checks 360
corrected states against the existing 180-tick golden fixture and compares all
200 corrected peer pairs. It also requires speculative disagreement during the
attack, a restore, final damage 18, final peer equality, final confirmation 199,
and measured wall-clock separation between the different pacing rates. Originally
displayed states that were already confirmed must match corrected history.

The MP4 consumes recorded SQL-read rows, using the existing wireframe renderer.
The two views align by simulation tick. Labels report actual PID, pacing interval,
wall time, pose, input/prediction, confirmation, damage, stun, restore/replay depth,
and SQL generation. Post-run assertions are explicitly labeled POST-RUN. Playback
is 0.5x relative to 60 Hz simulation with presentation-only event holds. It is
an offline recording of the executed process states, not a simultaneous wall-time
screen capture. Recorded history and packet evidence are kept separately.

This establishes local cross-process transport and recovery for this fixture.
Cross-machine/architecture determinism, uncontrolled network conditions, automatic
clock correction, process crash recovery, and live renderer attachment remain
untested. The fixture has no late-changing inputs near shutdown; general end-of-
session correction requires its own protocol test.

## Executed evidence

| Run | Peer PIDs | Datagrams received | Dropped | Peak queued |
| --- | --- | --- | ---: | ---: |
| Recorded | 22172 / 22173 | 503 / 505 | 58 | 61 |
| Headless repeat | 25153 / 25154 | 504 / 505 | 57 | 60 |

Both runs showed speculative disagreement on ticks 78 through 99. Peer 1
restored frame 78 at tick 100 and executed 23 advances (22 replay + 1 current).
Both confirmed through 199, matched all 200 corrected peer pairs, matched 360
golden states, and ended with damage 18. The recorded final local-step wall
times were approximately 7.961 and 8.160 seconds, reflecting independent pacing.

The final-code Rust library suite passed all 15 tests in 12.05s. The Python
fault-policy boundary test passed. No source simulation equations changed.

- `78_processes.mp4`: inspected actual GPU output, 615 frames, 10.25 seconds,
  960x540 H.264. Previous recordings remain intact.
- `79_process_verification.json`: recorded-run assertion summary and restore data.
- `80_process_evidence.tar.gz`: both runs' full original and corrected state
  histories, SQL-read rows, network audits, verification summaries, and the
  recorded run's peer logs. Paths retain their original temporary run names.
- `81_process_frames.png`: inspected idle, jump, hit disagreement, pre-recovery,
  rollback recovery, and final-state frames.

To revalidate archived histories, extract into a fresh temporary directory,
change to one extracted run directory, and execute the absolute built binary
with `--process-video --verify-only`. Omit `--verify-only` to render its history.
