# Dead file bakeoff

## Fixture measurements

| Tool | Language | Scope | Agreement | ryi-only | tool-only |
|---|---|---|---:|---:|---:|
| madge | TypeScript | `tests/fixtures/ts5_findings` | 32 | 0 | 0 |
| Knip | TypeScript | `tests/fixtures/ts5_findings` | 32 | 0 | 29 |
| `mod` reachability | Rust | `crates/sprefa-extract/tests/fixtures/rust_module_reachability` | 1 | 0 | 0 |
| rustc `dead_code` | Rust | `crates/sprefa-extract/tests/fixtures/rust_module_reachability` | 0 | 1 | 1 |

TypeScript fixture verdict: madge and Ryi produce the same 32 orphan paths. Knip reports all 61 fixture files as unused with the minimal synthetic package manifest; its 29 extra paths have no declared package entrypoint to root them.

Rust fixture verdict: Ryi resolves `mod live;` to `src/live.rs` as a `file_edge`; its orphan set matches module reachability at `src/orphan.rs`. rustc reports the unused `unused_item` in reachable `src/live.rs` and does not compile `src/orphan.rs`.

Knip-only paths, each caused by the fixture package having no declared entrypoint:

```text
barrel_reexport/barrel.ts
barrel_reexport/helpers.ts
closure_callee.ts
destructured_receiver/ctx.ts
iface_receiver/api.ts
init_receivers/printers.ts
known_receiver/ns.ts
member_calls/classes.ts
module_plane/ambiguous_barrel.ts
module_plane/ambiguous_left.ts
module_plane/ambiguous_right.ts
module_plane/cycle_a.ts
module_plane/cycle_b.ts
module_plane/default_target.ts
module_plane/helpers.ts
module_plane/index.ts
module_plane/namespace_target.ts
module_plane/renamed_barrel.ts
module_plane/renamed_source.ts
module_plane/shadow_export.ts
module_plane/two_hop_inner.ts
module_plane/two_hop_middle.ts
module_plane/two_hop_outer.ts
module_plane/widgets.ts
namespace_members/api.ts
namespace_members/barrel.ts
namespace_members/impl.ts
private_shadows_export/nodeTests.ts
top_level_callee.ts
```

The rustc comparison projects diagnostic source paths to files: it identifies an unused item in reachable `src/live.rs`, while Ryi's file-level result is `src/orphan.rs`.

## Reproduction commands

Commands run from the repository root. Outputs stay in the lane scratch directory.

TypeScript fixture:

```sh
scratch=$HOME/.cache/lanes/the-gang-graph/dead-files
HAFLEY_TRACE="$scratch/ryi-ts5-trace.json" timeout 10 "$HOME/.cache/boop/cargo-target/debug/ryii" --deps --root crates/sprefa-extract/tests/fixtures/ts5_findings crates/sprefa-extract/tests/fixtures/ts5_findings > "$scratch/ryi-ts5-relative.jsonl"
timeout 10 madge --orphans --extensions ts crates/sprefa-extract/tests/fixtures/ts5_findings > "$scratch/madge-ts5.txt"
timeout 10 python3 - <<'PY'
import json
import pathlib
import shutil

source = pathlib.Path('crates/sprefa-extract/tests/fixtures/ts5_findings').resolve()
target = pathlib.Path.home() / '.cache/lanes/the-gang-graph/dead-files/ts5_findings'
if target.exists():
    shutil.rmtree(target)
shutil.copytree(source, target)
(target / 'package.json').write_text(json.dumps({
    'name': 'ts5-findings', 'version': '0.0.0', 'private': True,
}, indent=2) + '\n')
PY
timeout 10 npx -y knip --directory "$scratch/ts5_findings" --reporter json > "$scratch/knip-ts5.json"
timeout 10 "$HOME/.cache/boop/cargo-target/debug/lab-dead-files" crates/sprefa-extract/tests/fixtures/ts5_findings "$scratch/ryi-ts5-relative.jsonl" "$scratch/madge-ts5.txt"
timeout 10 "$HOME/.cache/boop/cargo-target/debug/lab-dead-files" crates/sprefa-extract/tests/fixtures/ts5_findings "$scratch/ryi-ts5-relative.jsonl" "$scratch/knip-ts5.json"
```

Rust fixture:

```sh
scratch=$HOME/.cache/lanes/the-gang-graph/dead-files
fixture=crates/sprefa-extract/tests/fixtures/rust_module_reachability
timeout 10 cargo check --manifest-path "$fixture/Cargo.toml" -j 2 --offline --message-format json > "$scratch/rust-cargo.jsonl"
HAFLEY_TRACE="$scratch/rust-ryi-trace-final.json" timeout 10 "$HOME/.cache/boop/cargo-target/debug/ryii" --deps --root "$fixture" "$fixture" > "$scratch/rust-ryi-final.jsonl"
timeout 10 printf '%s\n' 'src/orphan.rs' > "$scratch/mod-orphans.txt"
timeout 10 "$HOME/.cache/boop/cargo-target/debug/lab-dead-files" "$fixture" "$scratch/rust-ryi-final.jsonl" "$scratch/mod-orphans.txt"
timeout 10 "$HOME/.cache/boop/cargo-target/debug/lab-dead-files" --rustc "$fixture" "$scratch/rust-ryi-final.jsonl" "$scratch/rust-cargo-fixed.jsonl"
```

## Remaining gate: user-selected TypeScript repository

Only the real-repository measurements remain open. The TypeScript repository path has not been selected. After selection, run:

```sh
scratch=$HOME/.cache/lanes/the-gang-graph/dead-files
HAFLEY_TRACE="$scratch/real-ts-ryi-trace.json" timeout 10 "$HOME/.cache/boop/cargo-target/debug/ryii" --deps --root <typescript-repository> <typescript-repository> > "$scratch/real-ts-ryi.jsonl"
timeout 10 madge --orphans --extensions ts <typescript-repository> > "$scratch/real-ts-madge.txt"
timeout 10 npx -y knip --directory <typescript-repository> --reporter json > "$scratch/real-ts-knip.json"
timeout 10 "$HOME/.cache/boop/cargo-target/debug/lab-dead-files" <typescript-repository> "$scratch/real-ts-ryi.jsonl" "$scratch/real-ts-madge.txt"
timeout 10 "$HOME/.cache/boop/cargo-target/debug/lab-dead-files" <typescript-repository> "$scratch/real-ts-ryi.jsonl" "$scratch/real-ts-knip.json"
```
