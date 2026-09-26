---
created: 2026-09-25
updated: 2026-09-25
type: bug
reporter: claude-lane-w
status: open
priority: normal
related: ['@cleave-real-crate-defects']
---

# cleave --list leaves SRC imports that a later row made unused

## Description

Repro: a 10-row --list batch moving SourceSpan..GitEntryKind from crates/soopy/src/_0_types.rs into src/_0b_moved.rs (copy). Row k keeps 'use crate::_0b_moved::X;' in SRC because an item still in SRC uses X; a later row moves that user too, and nothing revisits the kept import. cargo check: 0 errors, 3 'unused import' warnings in src/_0_types.rs (SourceSpan, BytePosition, UntrackedFilePolicy). Expected: after the last row, drop SRC specifiers that name DEST and have no remaining free-name reference in SRC.
