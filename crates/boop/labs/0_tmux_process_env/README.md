# Tmux active-process environment lab

Run this host probe from the repository root:

```bash
cargo run -p boop --example tmux_process_env
```

It creates one tmux session and keeps its original interactive shell alive. A
single initial `tmux send-keys` sources `driver.sh` in that shell. The driver
runs a Claude-shaped child, waits for a file, then runs a Codex-shaped child.
The Rust parent controls the transition by creating files in the probe state
directory. The two children therefore occupy the same pane sequentially
without using tmux key injection for the phase switch.

Each child receives these three values through `exec` inheritance:

```text
BOOP_HARNESS=claude|codex
BOOP_ROUTE_ID=route-claude|route-codex
BOOP_INHERITED_MARKER=inherited-claude|inherited-codex
```

The child then calls shell `export` to set `BOOP_MUTATED_MARKER`. The probe
prints the four values, when visible, from both `ps eww` and
`sysinfo::Process::environ()`. It reports an unavailable route when neither
API exposes `BOOP_ROUTE_ID`.

The receipt contains these tmux values for each phase:

```text
#{pane_id}
#{pane_pid}
#{pane_tty}
#{pane_current_command}
```

It also prints:

```text
ps -p <harness-pid> -o pid=,ppid=,pgid=,tpgid=,lstart=,command=
ps -t <pane-tty-without-/dev/> -o pid=,ppid=,pgid=,tpgid=,state=,command=
ps eww -p <harness-pid> -o command=
```

## Host receipt, 2026-08-21

`cargo run -p boop --example tmux_process_env` produced:

```text
phase=first
pane_id=%3336
pane_pid=57897
pane_tty=/dev/ttys003
pane_current_command=bash
process=58126 57897 58126 58126 Fri Aug 21 21:58:41 2026 /bin/bash .../harness.sh .../state/first
tty_processes=57897 6938 57897 58126 Ss -bash 58126 57897 58126 58126 S+ /bin/bash .../harness.sh .../state/first 58134 58126 58126 58126 S+ sleep 0.05
ps_env=
sysinfo_env=
ps_env_key_count=0
sysinfo_env_key_count=0
route_selection=unavailable
phase=second
pane_id=%3336
pane_pid=57897
pane_tty=/dev/ttys003
pane_current_command=bash
process=58210 57897 58210 58210 Fri Aug 21 21:58:41 2026 /bin/bash .../harness.sh .../state/second
tty_processes=57897 6938 57897 58210 Ss -bash 58210 57897 58210 58210 S+ /bin/bash .../harness.sh .../state/second 58212 58210 58210 58210 S+ sleep 0.05
ps_env=
sysinfo_env=
ps_env_key_count=0
sysinfo_env_key_count=0
route_selection=unavailable
result=PASS
same_pane=%3336
first_pid=58126
second_pid=58210
first_route=unavailable
second_route=unavailable
mutation_ps=absent
mutation_sysinfo=absent
```

The `PGID` and `TPGID` of the active harness were both the harness PID in each
phase. `pane_pid` stayed `57897`; it was the interactive `-bash` parent.

The host did not expose either inherited markers or the post-start shell export
through `ps eww` or sysinfo. The probe treats that as a completed negative
environment-recovery result instead of treating the persistent pane as caller
identity.

## Resolution boundary

A stored route needs the process identity captured at launch: PID plus process
start time. Resolve a caller in this order:

1. Ask tmux for the target pane's tty and current pane process root.
2. Read the pane-root descendant tree and identify foreground process group
   members using `PGID == TPGID` when job control is present.
3. Match the foreground harness PID and its start time against the route's
   stored launch identity.
4. Re-read PID plus start time after the match. Reject the result if it changed,
   the process exited, or multiple live candidates match.

`#{pane_pid}` identifies tmux's long-lived pane root. It is useful only as the
start of the descendant walk. `#{pane_id}` remains the delivery coordinate.

On macOS, sysinfo 0.39 obtains environment data through `KERN_PROCARGS2`; its
`Process::environ()` is an API wrapper around the same kernel source. This host
returned no synthetic markers through that API, so a separate libc
`sysctl(KERN_PROCARGS2)` implementation would query the same unavailable source.

## Race and privilege limits

PID alone is reusable after exit. Store a launch-time start identity beside the
route, compare it before and after the match, and treat either failed read as no
live match. A process can still exit after the second read; delivery must keep
its existing failure handling.

`ps eww` and `KERN_PROCARGS2` expose the process environment subject to macOS
visibility policy. This lab uses public synthetic markers only.
