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

## Fixpoint, rounds 3-6 (2026-10-03, same day)

| date | decision | where it bites |
| --- | --- | --- |
| 2026-10-03 | ryi = extraction + closed store + on-request fact providers + edit application. sprefa = every rule, every walk, every scope, every plan IR. ryi tables are CLOSED: ryi writes, nobody else mutates, sprefa reads. | plans/2026-10-03-graph-slow-demand-walk.md |

| item | ryi | sprefa |
| --- | --- | --- |
| open recursion over graphs (call paths, chains, points-to) | per-body provider: edges out of one body, warm RA Session | owns the walk (demand-lowered rules) |
| closed-form tree recursion (preorder pre/last intervals) | owns (index) | reads |
| graph --slow loop | thin loop over the provider until the protocol exists; then deleted, graph --slow = canned sprefa program | canned program |
| `extra` seam | dropped | - |
| provider protocol | daemon (server_auto) API is the default seam; in-process only behind an opt-in feature of a thin client crate | never compiles RA |
| scope ("which things") | commands take `--scope <relation>`; --entry/--pattern/PATH#NAME are sugar for it | computes scope relations; syntax atoms = scm++ patterns evaluated by ryi |
| plan IR / SQL | scm++ lowers to ivm-ir (sqlite_ivm, engine-free), consumed via local kellnr registry, pinned; no new SQL generator meanwhile | owns ivm-ir + batch and incremental renderers |
| fact schema truth | 1_facts.tsp (+ tsi .tsp) is truth | dl8 reads it via a TypeSpec importer; regenerated registry.rs compared byte for byte |
| ids | contract carries ids + dict(id, text); id = 64-bit content hash, collision = error | joins on ids, decodes at output |
| deltas | owns emission + format (`ryii watch`): relation, ids, sign, epoch per settled change; file edit retracts old content id rows | consumes as frontiers |
| formats | SQLite views now; Parquet later as a second materialization | one reader adapter |
| witness tier | contract views carry it | filter / rank by it; a gap is a missing witness |
| edits | binding-exact sites (provider); applies span edits to hand-written files | policy sites; dl8 fs.file whole-writes generated files only |
