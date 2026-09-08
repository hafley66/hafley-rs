# Active game pipeline tasks

Recorded 2026-09-08. Execution ledger for a shared world with Lovers-style ships,
Terraria-style terrain, Smash-style combat, and items/regions/statuses modifying rules.
`101_current.json` selects the next task; this file owns acceptance and deferrals.

## Working rules

- Implement in this lab and `games/shared`. Sealed applications remain read-only
  references, never the destination, replacement runtime, or publish source.
- Reuse shared redux/rollback. Inspect existing reusable cascade, input, geometry
  and storage APIs/tests before adding equivalents. No new reducer/session framework.
- TSP owns shared IO, IDs, constants and effect payloads; use the local TypeSpec skill.
- Import and validate offline. Runtime uses versioned baked data. Distinguish
  decoded data, executed behavior, lab policy and source-game equivalence.
- Before each implementation, record existing type signatures and short body
  pseudocode, then lifetimes, storage, read/write order and identity rules.
- Commit tested increments. Update this ledger with source, command, receipt and
  remaining gap. A checked task requires its evidence, not a build-only success.
- Preserve `just test-reuse` in the core gate. Use `just test` at integration
  checkpoints. Visible changes require an inspected H.264 MP4 and deterministic checks.
- No deployment is required by this queue. `/game/` remains protected.

## Queue

| ID | State | Work | Depends on | Completion evidence |
| --- | --- | --- | --- | --- |
| G0 | Done | Reuse redux and GGRS execution | Existing libraries | `62832dc`; `.workflow/test-uAKBba/receipt.json`: redux unit/external tests, shared rollback replay, 27 Falcon tests |
| G1 | In progress | Verify local mirror and Falcon inventory | G0 | Offline inventory and Falcon decoding passed; full mirror and downloader truncation recovery remain open |
| G2 | Pending | Full available Falcon data and behavior coverage | Falcon subset of G1 | Versioned package, decoder tests, supported/unresolved callback report |
| G3 | Pending | Reuse rule resolution and generate shared rule IO | G0; G2 IDs/defaults | Library-fit receipt, generated outputs, conflict/removal tests |
| G4 | Pending | Stage entity and editable terrain | G0; G3 shared IDs | Bounded edit/contact/restore fixture in actual simulation |
| G5 | Pending | Boots pickup/drop changes air-jump rules | G2 movement subset; G3; G4 | Independent expected tape, replay and GGRS correction |
| G6 | Pending | Integrated boots/terrain recording | G5 | `just test`, reproducible MP4, inspected labels, SQL/mesh equality |
| G7 | On deck | Moving ship and larger-world qualification | G6 | Carry/contact regression and measured resource limits |

G2's complete inventory and unresolved behavior work remains open even when a
supported subset enables G5. The entire site mirror need not finish before using
its verified Falcon subset. No existing rollback proof is discarded.

## G1: offline acquisition

- [x] Inspect existing mirror process/log before starting another downloader.
  Cache: `../fixtures/rukaidata-mirror/`; command: `just mirror`.
- [ ] Manifest original URLs, hashes, content kinds, game/version and retrieval
  status; compare reachable URLs with local files. Record failures and exit status.
- [ ] Test decoded HTML, redirect directory handling and missing/truncated-file
  recovery. Existing filenames alone do not prove complete downloads.
- [ ] Record disk budget. Current 4 GB quota is transferred bytes per invocation,
  not retained/decompressed cache size. Report partial coverage if space runs out;
  never delete unrelated data. Routine importer tests must not fetch the site.

### G1/G2 checkpoint: 2026-09-08

- `just mirror-status` now writes `.workflow/mirror-inventory.json` without network
  requests. First scan: 1,811 local files, 119,499,072 bytes, zero detected invalid
  files; 85,006 unresolved same-host links from locally discovered HTML. This is
  partial-site coverage, not a full remote inventory. The active download changes it.
- All 491 Falcon subaction index links resolve locally. `just import-catalog`
  decoded all 491 through the existing brawllib 0.29.0 decoder, consuming complete
  payloads. 107 have zero frames; `SpecialLwEndAir.html` reports bad_interrupts.
  The unnamed `.html` subaction is retained explicitly.
- Catalog metadata: `.workflow/falcon-catalog.json`; diagnostics:
  `.workflow/falcon-catalog.log`. This is a data/compatibility audit; the playable
  runtime still uses its seven-action package. G2 is not complete.
- `just test core` passed, receipt `.workflow/test-o8sEcu/receipt.json`, including
  shared-library and 27 Falcon tests. Offline inventory tests cover missing,
  truncated, repaired, compressed and unchanged fixtures; they do not establish
  actual network downloader recovery.
- Remaining G1: verify downloader completion/exit, preserve source retrieval
  metadata, and implement/test targeted recovery of truncated existing downloads.
  Wget `--no-clobber` alone retains a truncated existing file. Inventory diagnoses
  it; it does not currently repair it. No second crawler was added.

## G2: imported fighter package

- [ ] Reuse brawllib for all available Falcon subactions, attributes, actions,
  scripts and collision/pose metadata. Keep the seven pinned fixtures until replaced
  by verified local sources. Report absent fields instead of inventing defaults.
- [ ] One authored action catalog replaces loader-order and HUD-label duplication.
  Source identity remains separate from runtime indexing.
- [ ] Package identity includes game/version, source hashes and conversion version.
  Immutable assets stay outside per-tick snapshots; snapshot compatibility checks
  prevent replay against a different package.
- [ ] Inventory common movement and Falcon callback dependencies. Reuse existing
  evaluators/translators. Explicitly report unsupported executable constructs.
  Rukaidata pages do not supply every callback; Melee and PM remain distinct sources.
- [ ] Compare supported guards, timing, priorities and effects with independent
  source-game observations. Lab goldens establish local regression safety only.

## G3: item/region rule composition

- [ ] Inspect existing reusable cascade/rule signatures, dependencies and tests;
  record reusable operations and gaps before writing resolution code.
- [ ] Generate shared property IDs, selectors, operations and inspection payloads
  through TSP; executable phase implementations use Rust redux slices.
- [ ] Specify/test precedence. Candidate: character, region, equipment, temporary
  status. Define replace/add/multiply and stable equal-priority tie-breaking.
- [ ] Snapshot equipment, membership, statuses and gameplay RNG/selected outcomes.
  Effective-value caches need explicit revision keys and invalidation.
- [ ] Inspect base value, contributing source items and final value. Test conflicting
  overrides, removal restoring prior values, and same-tick pickup/drop ordering.

## G4: stage and terrain state

- [ ] Stage identity + transform + chunk references; chunks hold cells/materials
  and revisions. Fighters/items/projectiles retain independent identities.
- [ ] Select identity/reuse and storage policy from existing libraries. Declare
  bounded fixture capacities before making any large-world allocation claim.
- [ ] Collision and render geometry derive from terrain with revision invalidation.
  Use existing geometry/physics libraries for collision.
- [ ] Mutable transforms, edits and contact-affecting state participate in snapshots.
  Shared immutable chunks must not allow edits to mutate earlier snapshots.
- [ ] Define edit/rule/contact/simulation/effect tick order; restore across an edit
  at contact and compare subsequent states. Moving ships extend this model in G7.

## G5-G6: first visible acceptance

One input tape: Falcon acquires boots, gains an air jump, uses it, drops the boots,
and loses the override on editable terrain. Explicitly decide/test whether a spent
jump remains spent when equipment changes.

- [ ] Independent expected pickup/drop, jump availability, edits and final state.
- [ ] Restore/replay before and after equipment and terrain transitions.
- [ ] Delayed-input correction through shared GGRS; compare corrected states.
- [ ] Any durable effect requires confirmation/deduplication before persistence.
- [ ] Existing SQLite presentation and generated IO preserve exact row/mesh checks.
- [ ] Reusable recording labels tick, item, rule sources, resolved jump count,
  terrain revision, damage and prediction/rollback where applicable. Label lab rules.

## Deferrals

| Work | Resume trigger |
| --- | --- |
| Native/browser connected play | Selected cross-target acceptance; verify transport/ABI first |
| Moving ship carry | G6 passes; same stage ownership model, G7 |
| World streaming/persistence scale | Measured fixture exceeds resident chunk/entity budget |
| SQLite stress/zero-allocation | Concrete budget selected; measure full boundary and allocator costs |
| Arbitrary scripting | Concrete item cannot be expressed with existing rules/reducers |
| UI router/egui-kit migration | Inspector needs a missing capability; inspect reusable libraries first |
| LAN HTTPS/physical device | Web acceptance selected on another machine/device |
| Publication | Specific integrated artifact selected and its proof gates pass |

Older browser/photo goals remain in `92_web_import_plan.md`; they are not complete.
