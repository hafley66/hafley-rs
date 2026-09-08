# Main-thread, process, and render-thread fault experiment

## Existing APIs and scope

The fixture uses the existing Rust worker, GGRS, Rapier, and SQLite code.
`std::thread::sleep` / `Instant` retain the prior clock. Python's standard-library
`subprocess.Popen`, `selectors`, and `os.kill` control only the Godot child created
by this harness. No new package dependency is added.

Godot 4.7 is MIT licensed. `RenderingServer.call_on_render_thread(Callable)`
dispatches the callback to the render thread; `force_sync()` synchronizes the
rendering path. `OS.get_thread_caller_id()` records caller identity. The command
uses `--render-thread separate`, and the controller requires distinct main and
render thread IDs. References: [RenderingServer](https://docs.godotengine.org/en/stable/classes/class_renderingserver.html#class-renderingserver-method-call-on-render-thread),
[OS thread identity](https://docs.godotengine.org/en/stable/classes/class_os.html#class-os-method-get-thread-caller-id).

The render callback executes a bounded 800 ms sleep. This tests a CPU-side
render-thread delay and the associated engine synchronization. It does not
simulate GPU execution saturation, a driver hang, device loss, or recovery from
a driver reset. Deliberately hanging the system GPU is outside this experiment.

## Interfaces, lifetime, and sequence

```rust
schedule::spawn(shared: State, faults: bool) -> JoinHandle<Result<Vec<Status>, String>>
// Fault mode disables the previous synthetic SQL slot/consumer pause schedule.
// Log each tick's start and monotonic worker time, then execute the fixed step.

FalconSql::start_faults()
// Start the same in-process worker; keep independent SQL publication.
```

`54_fault_controller.py` owns the subprocess handle and reads its combined output
without blocking on a line. Its 35-second watchdog resumes a stopped child before
terminating it on failure. SIGSTOP/SIGCONT target exactly `child.pid`; no process
name search or process-group signaling is used. The child's existing simulation
and SQL worker remains a thread inside Godot. No process isolation is introduced.

| Trigger | Injection | Measurement |
| --- | --- | --- |
| Display reaches tick 60 | Main thread sleeps 800 ms | Count Rust tick starts between begin/end markers. |
| Display reaches tick 90 | Controller SIGSTOPs Godot, confirms OS state T, waits 800 ms, SIGCONTs | Drain already queued output; require zero tick starts during the remaining stopped interval. |
| Display reaches tick 130 | Render-thread callback sleeps 800 ms | Require separate thread ID and count Rust tick starts during the callback. |

The worker uses absolute deadlines. After process resume it executes overdue
fixed steps without sleeping until caught up. No simulation tick is discarded;
the controller requires exactly the sequence 0 through 179. This fixture does
not impose a maximum catch-up burst or prove a deadline guarantee.

Every peer World is still compared with the prior typed golden state on every
tick, including complete Rapier state. Every successful SQL publication checks
the full corrected window. Godot still verifies rows and uploaded mesh vertices,
and every consumed generation must be current at the SQL observation boundary.

## Evidence and reproduction

Run `bash 55_run_faults.sh`. It runs tests, builds gdext, checks extension loading
and GDScript parsing, runs the controller, and only encodes after its assertions
pass. Output files are separate from the preceding increments:

- `56_fault_worker.json`: all simulation/publication observations.
- `57_fault_consumed.json`: actual SQL/mesh readback observations.
- `58_faults.json`: external timing, thread identity, stopped-state, and tick audit.
- `59_fault_run.log`: captured Godot and worker output.
- `60_faults.mp4`, `61_fault_frames.png`: actual MovieMaker capture and montage.
- `62_faults.avi`: ignored regenerable intermediate.

MovieMaker advances with rendered frames, so a time interval where the engine
cannot render does not automatically appear as an equally long freeze in the
encoded movie. Captions report the injected operation and returned state; the
external monotonic timestamps carry elapsed-time evidence. Earlier captures are
preserved.

The separate render-thread mode emits Godot's experimental-feature warning.
MovieMaker and mesh readback also emit repeated render synchronization warnings;
the controller counts these in `58_faults.json` and collapses their repeated
text in the log. These synchronized reads remain part of the implementation
under test. No performance benchmark or production-readiness claim follows.

## Final recorded result

- Main-thread delay: 20 worker tick starts during 801 ms.
- Render-thread delay: 20 worker tick starts during 805 ms; main thread ID 1,
  render callback thread ID 5.
- Whole-process suspension: OS state T, zero worker tick starts in the drained
  quiet interval; suspension measurement 809 ms. The first 200 ms are reserved
  for draining pre-signal pipe output.
- Recovery: ticks 0 through 179 exactly once in the tick-start audit, 360 exact
  peer-state comparisons, 129 SQL/mesh acknowledgements, final tick 179 and
  generation 180. No SQL publications were refused in this increment.
- Validation: 13 CPU tests, the external fault assertions, and all-target gdext
  Clippy with warnings denied passed. Cargo retains the prior `block` 0.1.6
  future-incompatibility notice.
- Inspected H.264 MP4: 960x540, 338 frames, 5.633333 seconds, 443,184 bytes.
  Captions show 20 completed simulation ticks across each thread delay, and the
  final image shows tick 179 with damage 18. The run counted 2,127 repeated
  render-synchronization warnings.
