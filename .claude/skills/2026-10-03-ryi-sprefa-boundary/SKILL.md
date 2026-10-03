---
name: 2026-10-03-ryi-sprefa-boundary
description: Responsibility split between ryi (facts + one-shot questions) and sprefa (rules over facts), agreed to fixpoint with the sprefa session; decision of 2026-10-03.
---

| date | decision | where it bites |
| --- | --- | --- |
| 2026-10-03 | ryi vs sprefa boundary, agreed by the ryi and sprefa sessions at the user's ask ("keep the responsibilities separate"); sprefa copy: sprefa .claude/skills/2026-10-03-ryi-sprefa-boundary (e11d0bbef). Table below. | crates/sprefa-extract/AGENTS.md (extract = what is written; programs = what it means) |

| item | ryi | sprefa |
| --- | --- | --- |
| syntax rows, TSI rows, checker witnesses | produces | reads |
| scm++ (non-recursive relations, preorder range joins) | owns | - |
| recursion (method chains, transitive pointer rules) | - | owns (Datalog rules) |
| graph --slow demand walk (Call, Method) | owns | - |
| Passed / TraitImpl edges | owns | - |
| bespoke pointer rules within and across repos | `extra` seam only (accepts edges) | writes the rules |
| cross-language queries | - | over shared tsi.* relation names |
| engine research and shootouts (DuckDB, DuckPGQ, OpenIVM, DD, sqlite_ivm, pg_ivm, DBSP) | no engine crate enters ryi | owns; DuckDB CLI-only |
| strings interned to integer ids | applies | applies |
| refactor tools (rename, move, cleave) used on sprefa Rust | owns the tools | client only |
| defects sprefa finds in ryi | fixes | files issues only, never edits ryi |
| sprefa test `tests/it/_9j_no_recursion.rs` reads `ryi <root> --resolve --kinds cst,call --sqlite <db>` | provides the contract | consumes |

Contact points:
1. ryi's walk `extra` seam takes edges in; ryi never computes sprefa rules.
2. TSI rows flow ryi -> sprefa only.
3. Contract relations: `node(family, kind, name, span__start, span__end, _input_path)` and
   `resolved_edge(caller_path, caller_site_start, caller_site_end, callee_path, callee_start)`
   are SQL views over ryi's interned base tables (base tables ryi-private). A column change
   = version bump in crates/sprefa-extract/schema/1_facts.tsp + a note to sprefa. ryi keeps a
   contract test running the _9j command shape and asserting those columns.
