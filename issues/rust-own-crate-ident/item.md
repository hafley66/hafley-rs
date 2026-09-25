---
created: 2026-09-25
updated: 2026-09-25
type: bug
status: fixed
priority: normal
labels: [extract]
closed: 2026-09-25
---

# rust module plane: use <own_crate>::x from a bin crate binds nothing

## Description

## Description
`use <own_package>::x` from a bin crate (e.g. `crates/soopy/src/main.rs` writing `use soopy::...`) binds nothing in the Rust module plane: the first segment is treated as an external crate. `ryi fast --entry soopy/src/main.rs soopy/src` reaches only main.rs.

## Acceptance Criteria
- [ ] a path whose first segment names a corpus crate (Cargo.toml package/lib name, `-` -> `_`) resolves from that crate's lib root
- [ ] `--entry soopy/src/main.rs` reaches the lib files main.rs uses
