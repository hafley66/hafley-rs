---
name: 2026-10-03-sqlite-interning
description: SQLite schemas intern every string to an integer surrogate id; rows and indexes hold ids only; decision of 2026-10-03.
---

| date | decision | where it bites |
| --- | --- | --- |
| 2026-10-03 | "absolutely zero strings in zero indexes ... use normal relational design. strings get interned to surrogate id, that is the indexes"; "never copy in sql" | crates/sprefa-extract/src/bin/ryi/2_scmpp.rs:16 |
