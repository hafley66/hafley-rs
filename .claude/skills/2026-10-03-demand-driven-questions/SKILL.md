---
name: 2026-10-03-demand-driven-questions
description: One-shot ryi questions run as a demand-driven worklist from the seed, never an eager whole-project build; decision of 2026-10-03.
---

| date | decision | where it bites |
| --- | --- | --- |
| 2026-10-03 | "we need dynamic whiling in any ask-one-time thing": every one-shot question (graph, rename, cleave, move, query) walks a worklist from its seed and infers/reads only what the walk reaches; no eager whole-project fact build per question | crates/sprefa-extract/src/0_graph.rs:51 (load_store) |
