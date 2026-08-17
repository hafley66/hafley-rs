# Strip the concatmap output path (send-only)

Date: 2026-08-17. Status: built, green (cargo check + 21 unit tests + clippy).
Continuation of `2026-08-16-concatmap-e2e-receipts.md` and
`2026-08-17-turn-event-system.md`.

## TOC

1. Why
2. Type signatures
3. Edit list
4. Verification

## Why

The `.md` file write and the reply-text round-trip are already removed. What
remains is the flag surface and the dead dir creation: `Args.out_dir`,
`run()`'s `create_dir_all(&args.out_dir)`, the `--out` CLI flag, the help
text that advertises it, the unit-test helper that threads it, and the e2e
fixture that passes it. None of it is read by any consumer; the store
(`agent_turn`) is the only output. This strips the last of the dead weight.

## Type signatures

```rust
// before
pub struct Args { ..., pub out_dir: PathBuf, ... }
// after
pub struct Args { ..., /* out_dir removed */ }
```

`Args::out_dir` has no reads left after the `.md` write is gone, only the
`create_dir_all` side effect and test plumbing.

## Edit list

| file | site | change |
| --- | --- | --- |
| `src/concatmap.rs` | `Args` (38) | delete `out_dir: PathBuf` |
| `src/concatmap.rs` | `run()` (409-410) | delete `create_dir_all(&args.out_dir)` |
| `src/concatmap.rs` | `window_args` helper (1088-1094) + callers (1175-1184) | drop the `out_dir` param/field |
| `src/main.rs` | concatmap `Args` struct (247-249) | delete `out` field + `--out` flag |
| `src/main.rs` | `Args { ... }` construction (819) | delete `out_dir: out` |
| `src/main.rs` | `concatmap --help` text (64, 71, 74, 81) | drop `--out ...` tokens |
| `tests/concatmap_e2e.rs` | fixture + `start_mapper` (66, 113, 123-124) | drop `out` dir + `--out` arg |

## Verification

```bash
cargo check -p boop
cargo test -p boop --lib concatmap   # 21 unit tests stay green
cargo check -p boop --tests          # e2e + integration tests compile
```
