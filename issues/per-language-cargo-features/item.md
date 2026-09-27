---
created: 2026-09-27
updated: 2026-09-27
type: feature
status: fixed
priority: medium
epic: capability-as-data
related: ['@capability-as-data']
labels: [extract, cargo-features]
---

# Select language front ends with Cargo features

## Current feature graph

```text
sprefa-extract (library)
└── hafley_scm/read                         # unconditional path dependency
    ├── rust_syn                            # default in hafley_scm
    ├── parser/shared deps                  # serde_json, rayon, tracing, cache, sqlite, soopy
    └── all front ends                      # all currently unconditional within read
        ├── Rust: syn, tree-sitter-rust
        ├── TypeScript/JavaScript: oxc_*, tree-sitter-typescript, tree-sitter-javascript
        ├── Go: tree-sitter-go
        ├── Kotlin: tree-sitter-kotlin-sg
        ├── Python: tree-sitter-python
        ├── Prolog: tree-sitter-prolog
        ├── Markdown: tree-sitter-md
        ├── data: tree-sitter-json, tree-sitter-yaml, tree-sitter-toml-ng
        ├── fallback: tree-sitter-html
        ├── GDScript: tree-sitter-gdscript
        └── Common Lisp: tree-sitter-commonlisp

sprefa-extract/cli
├── ryi-proto, clap, axum, tokio, transport and subscriber dependencies
└── hafley_scm/cli -> read

sprefa-extract/rust-checker
├── hafley_scm/rust-checker
└── ra_ap_hir, ra_ap_ide, ra_ap_ide_db, ra_ap_load-cargo,
    ra_ap_project_model, ra_ap_vfs

sprefa-extract/ts-checker -> hafley_scm/ts-checker -> read
sprefa-extract/go-checker -> hafley_scm/go-checker -> read
```

Receipts: `crates/sprefa-extract/Cargo.toml` has `hafley_scm` with `features = ["read"]` as a non-optional dependency, and the `cli`, checker, and mimalloc features. `crates/hafley_scm/Cargo.toml` defines the aggregate `read` feature with every grammar and front-end dependency, plus `cli`, checker features, and `rust_syn`.

Baseline repro (2026-09-27): current `ryii capabilities` emits every Source row;
`cargo tree --manifest-path crates/sprefa-extract/Cargo.toml --no-default-features -e features -i tree-sitter-rust` still includes `tree-sitter-rust` through unconditional Rust grammar deps.

## Acceptance criteria

- [x] Add paired language features to `sprefa-extract` and `hafley_scm` for every `Source` row: `rust`, `typescript` (including JavaScript/JSX), `go`, `kotlin`, `python`, `prolog`, `markdown`, `data`, `fallback`, `gdscript`, and `commonlisp`.
- [x] Make grammar and language-specific parser dependencies optional under their owning language feature; leave shared parse/runtime dependencies in a named shared feature.
- [x] Preserve `read` as an aggregate feature enabling all language features, so existing consumers retain current behavior.
- [x] Keep `cli` and checker features forwarding through the same graph without enabling languages outside the selected aggregate.
- [x] Gate source modules, grammar registration, roster rows, and `Source::planes` declarations with those features; unsupported source rows are absent from the selected build's capability matrix.
- [x] Add a feature-matrix test that compares `ryii capabilities` output to `cargo tree -e features` for each grammar dependency and verifies the default/aggregate build contains every current source.
- [x] Verify `cargo check -p hafley_scm --no-default-features --features read` and representative single-language builds, including Rust and TypeScript.

Implementation receipt: `t_184_language_feature_matrix::capabilities_roster_and_single_language_grammar_features_match`; no-default, Rust-only, and TypeScript-only checks pass; full sprefa-extract 1,120 passed / 18 skipped; workspace 1,378 passed / 203 skipped.

## Scope note

The capability matrix is the dependency inventory for this cut. Keep feature names aligned with `Source::name()` where the names match. `typescript` owns JavaScript and JSX because they share `TsSource`; the fallback tree-sitter grammar has its own `fallback` feature. Shared `tree-sitter`, `serde`, and read runtime dependencies remain reachable through the crate rather than requiring downstream duplicate dependencies.
