# boop-main-split REPORT

`crates/boop/src/main.rs` was 7383 lines with 47 inline tests. It is now 1786
lines and holds the clap tree, `main()`, the dispatch match and 5 tests. Every
other line moved verbatim into `crates/boop/src/cli/*.rs`. Help output is
byte-identical on all 84 subcommand screens and `cargo test -p boop` passes the
same 461.

## Contents

1. [File table](#file-table)
2. [Where each section landed](#where-each-section-landed)
3. [Gates](#gates)
4. [Deviations from the brief](#deviations-from-the-brief)
5. [Commits](#commits)

## File table

| file | lines | takes |
|---|---|---|
| `crates/boop/src/main.rs` | 1786 | `Cli`, `SubCmd`, the arg/format enums, `help_wanted`, `main`, the startup-sync helpers, `init_tracing`, `supervised_lane`, the sub-enum bank, 5 tests |
| `crates/boop/src/cli/mod.rs` | 477 | module list, `CONCATMAP_EXAMPLES`, `doctrine`, `line`, `write_line`, `emit_event`, `mail_dir`, `pad`, `now_ms`, the bus store helpers, `testkit`, 1 test |
| `crates/boop/src/cli/job.rs` | 3057 | measure, dispatch, resolve, wait, sweep, lane, beep, pstree, 31 tests |
| `crates/boop/src/cli/db.rs` | 1076 | pass-1 transcript verbs, the `db` tree, usage, price, 6 tests |
| `crates/boop/src/cli/me.rs` | 512 | adopt, prune, whoami, the `me` verbs, 3 tests |
| `crates/boop/src/cli/mail.rs` | 499 | list, hail, tell-parent/children, inbox + hook drain, 0 tests |
| `crates/boop/src/cli/debug.rs` | 164 | `debug`, host chat, config presets, 1 test |

## Where each section landed

| main.rs section marker | destination |
|---|---|
| Pass 1 verbs: layer 2 (transcript) | `cli/db.rs` |
| The verb output helpers | `cli/mod.rs` |
| list | `cli/mail.rs` |
| measure (layer 0) | `cli/job.rs` |
| dispatch (layer 1 + bus) | `cli/job.rs` |
| resolve | `cli/job.rs` |
| hail | `cli/mail.rs` |
| wait | `cli/job.rs` |
| sweep | `cli/job.rs` |
| lane | `cli/job.rs` |
| adopt / prune + bus store helpers | split: adopt/prune to `cli/me.rs`, bus store helpers to `cli/mod.rs` |
| The two trees (`beep` / `db` enum bank) | stays in `main.rs` |
| beep | `cli/job.rs` |
| pstree | `cli/job.rs` |
| db | `cli/db.rs` |
| whoami | `cli/me.rs`; its usage/price tail to `cli/db.rs` |

Three straddles, named as the brief asks:

- `run_lane_supervisor`, `record_lane_purpose`, `record_lane_mood` sat under the
  `hail` marker but are called only from `run_lane` / `run_beep_lane`, so they
  went to `cli/job.rs`. `record_control_edge` stayed with hail.
- `run_inbox`, `run_inbox_drain`, `write_inbox_hooks`, `report_inbox_hooks` sat
  under the `wait` marker; they are the inbox verbs, so they went to
  `cli/mail.rs`. `run_wait`, `waiting_as`, `wait_and_exit` and `WAIT_POLL`
  stayed with `job`.
- `USAGE_TOTALS_SQL`, `open_ro_store` and the `run_usage*` / `run_price` block
  sat under the `whoami` marker; the brief gives usage and price to `db`, so
  they went to `cli/db.rs`.

## Gates

### 1. Help, byte-identical

```
$ diff -r TASKS/help-before TASKS/help-after
$ echo $?
0
```

Empty. A second, wider sweep walked every `Commands:` block recursively, 84
help screens in all, and diffed before against after:

```
$ diff help-full-before.txt help-full-after.txt
$ echo $?
0
```

Neither directory is committed.

### 2. Tests and clippy

```
base  (62525d0): 461 passed, 2 ignored, 0 failed   (bin target: 47)
after (db6b919): 461 passed, 2 ignored, 0 failed   (bin target: 47)

$ cargo clippy -p boop -- -D warnings
    Finished `dev` profile [unoptimized + debuginfo] target(s) in 0.07s
rc=0

$ cargo clippy -p boop --bin boop --profile test -- -D warnings
rc=0
```

`cargo clippy -p boop --all-targets -- -D warnings` is red on both the base and
this branch, on `crates/boop/tests/host_chat.rs:44` (`clippy::needless_borrow`),
a file this lane did not touch.

### 3. main.rs line count

```
$ wc -l crates/boop/src/main.rs
    1786 crates/boop/src/main.rs
```

Over the brief's 1500 target, and the two instructions collide. The brief's
table keeps `Cli`, `SubCmd` **and the sub-enums** in `main.rs`; the sub-enum
bank alone (`BeepCmd` at :946 through `CursorCmd` ending :1675) is 738 lines:

| block | lines |
|---|---|
| module doc + imports | 37 |
| `Cli`, `SubCmd`, `QueryArgs`, the four format enums, `ConfigCmd` | 455 |
| `help_wanted`, `main`, startup-sync helpers, `init_tracing`, `supervised_lane` | 445 |
| sub-enum bank | 738 |
| `mod tests` | 110 |

Moving each sub-enum next to the verb file that matches on it would put
`main.rs` at roughly 1048. That is one instruction against the other, so this
lane took the explicit table row and left the enums in place.

### 4. semver-checks

`crates/boop/src/lib.rs` is unchanged (`git diff 62525d0 -- crates/boop/src/lib.rs`
is empty), so the gate does not apply. `boop` is a bin plus a lib whose public
surface this branch never touches.

### 5. eprintln

```
$ grep -rn 'eprintln!' crates/boop/src/cli
crates/boop/src/cli/db.rs:67:                    eprintln!("resume offset: {}", chunk.next_offset);
crates/boop/src/cli/db.rs:388:        eprintln!("note: transcript shorter than stored offset; restarted from byte 0");
crates/boop/src/cli/db.rs:391:        eprintln!("note: skipped {skipped} line(s) that failed to parse as JSON");
crates/boop/src/cli/job.rs:446:        eprintln!("[boop] lane purpose not recorded: {error}");
crates/boop/src/cli/job.rs:539:            eprintln!("{timed_out}"); // @eprintln-ok: the re-run line must survive a redirected stdout
```

All five came out of the base `main.rs`, which had exactly five. The set is
byte-identical before and after; none was added.

## Deviations from the brief

- **The 47 tests could not become `crates/boop/tests/cli_*.rs`.** All 47 drive
  private items of the **bin** crate, and a `tests/` integration target can only
  link the `boop` **lib**. Every test landed as a `#[cfg(test)] mod tests` at the
  bottom of the file holding the code it drives, which is the brief's stated
  fallback. No `cli_*.rs` file was created. Split: job 31, db 6, main 5, me 3,
  mod 1, debug 1.
- **`cli::testkit`.** `temp_mail_dir` is used by tests in four modules and
  `route_with` by two, so both moved into a `#[cfg(test)] pub(crate) mod testkit`
  in `cli/mod.rs` rather than being copied. Bodies unchanged.
- **One file outside the ownership list changed**:
  `crates/boop/tests/temp_home_rail.rs:41`. `STORE_WAIVED` waived `"main.rs"`
  for a measured false positive; the two lines it waived (`Store::default_path()`
  next to a fixture lane name) now live in `cli/db.rs` and `cli/job.rs`, so the
  rail failed. The waiver follows the code:
  `&["main.rs", "supervise.rs"]` became `&["cli/db.rs", "cli/job.rs", "supervise.rs"]`,
  and the doc comment above it names the two files instead of one. The
  measurement it cites (that binary wrote 0 `agent_trace_event` rows) is
  unchanged: both files still compile into the same bin test binary.
- **Two comment edits**, both moving a comment with its code:
  - The doc comment `/// Write one line, treating a closed pipe as a normal end…`
    sat on `help_wanted` in the base while describing `line`; it moved to `line`
    in `cli/mod.rs`.
  - The `// adopt / prune + bus store helpers` marker lost its second half in
    `cli/me.rs`, because the bus store helpers went to `cli/mod.rs`.
- **`crate::tmux::mux()` became `tmux::mux()`** in the two `LiveTmuxSession`
  test impls: `tmux` was a crate-root import that now lives in `cli/job.rs`.
- **Visibility.** Every moved item is `pub(crate)`. `LaneArgs`, `DispatchArgs`,
  `HookWiring` and `ChatQueryOptions` fields are `pub(crate)` because `main()`
  constructs them. No verb, flag, help string or enum variant was renamed.
- Wrapping changed on 16 function signatures: the `pub(crate) ` prefix pushed
  them past 100 columns and `rustfmt` broke them one-argument-per-line. A
  normalized line-multiset diff of base `main.rs` against the seven new files
  shows no other content change.

## Commits

| sha | subject |
|---|---|
| `f1e7b19` | cli/mod.rs takes the shared help text, output and bus-store helpers |
| `42dae0b` | cli/db.rs takes the transcript pass-1 verbs, the db tree, usage and price |
| `6c0e8dc` | cli/mail.rs takes list, hail, tell-parent/children and the inbox hooks |
| `bb070b6` | cli/me.rs takes adopt, prune, whoami and the me verbs |
| `6bdffd7` | cli/job.rs takes measure, dispatch, resolve, wait, sweep, lane, beep and pstree |
| `242bfd1` | cli/debug.rs takes the debug window, host chat and the config verbs |
| `47ecc94` | restore the hail section marker dropped in the cli/mail.rs move |
| `db6b919` | the 47 main.rs unit tests follow their code into cli/*.rs |
