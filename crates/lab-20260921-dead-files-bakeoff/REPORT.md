# Dead file bakeoff

| Tool | Language | Scope | Agreement | ryi-only | tool-only |
|---|---|---|---:|---:|---:|
| madge | TypeScript | `crates/sprefa-extract/tests/fixtures/ts5_findings` | 32 | 0 | 0 |
| knip | TypeScript | fixture and real repository | pending | pending | pending |
| `mod` reachability | Rust | `crates/sprefa-extract` | pending | pending | pending |
| rustc `dead_code` | Rust | `crates/sprefa-extract` | pending | pending | pending |

TypeScript fixture verdict: madge and ryi produce equal orphan sets, with 32 shared paths and no disagreements.

Rust verdict: pending the source module reachability and `dead_code` measurements.

## Remaining measurements

Run on the user-selected TypeScript repository:

```sh
HAFLEY_TRACE=1 timeout 10 /Users/chrishafley/.cache/boop/cargo-target/debug/ryii --deps --root <typescript-repository> <typescript-repository> > /tmp/dead-files-ryi.jsonl
timeout 10 madge --orphans --extensions ts <typescript-repository> > /tmp/dead-files-madge.txt
timeout 10 knip --reporter json <typescript-repository> > /tmp/dead-files-knip.json
```

Run the Rust comparison on this checkout:

```sh
HAFLEY_TRACE=1 timeout 10 /Users/chrishafley/.cache/boop/cargo-target/debug/ryii --deps --root crates/sprefa-extract crates/sprefa-extract > /tmp/dead-files-rust-ryi.jsonl
timeout 10 cargo check --manifest-path crates/sprefa-extract/Cargo.toml --message-format json > /tmp/dead-files-rustc.jsonl
```

Compare tool output with the lab binary:

```sh
timeout 10 cargo run --manifest-path crates/lab-20260921-dead-files-bakeoff/Cargo.toml -- <typescript-repository> /tmp/dead-files-ryi.jsonl /tmp/dead-files-madge.txt
timeout 10 cargo run --manifest-path crates/lab-20260921-dead-files-bakeoff/Cargo.toml -- <typescript-repository> /tmp/dead-files-ryi.jsonl /tmp/dead-files-knip.json
timeout 10 cargo run --manifest-path crates/lab-20260921-dead-files-bakeoff/Cargo.toml -- crates/sprefa-extract /tmp/dead-files-rust-ryi.jsonl
timeout 10 cargo run --manifest-path crates/lab-20260921-dead-files-bakeoff/Cargo.toml -- --rustc crates/sprefa-extract /tmp/dead-files-rust-ryi.jsonl /tmp/dead-files-rustc.jsonl
```
