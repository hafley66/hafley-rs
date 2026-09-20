---
created: 2026-09-19
updated: 2026-09-19
type: improvement
status: open
priority: normal
epic: extract-parity-move-rename
related: ['@local-binding-inference']
labels: [extract, artifact-cli, phase-refinement-1, component-rename]
---

# rename records why it declined a site

## Description

## Description

Resolve saves why a site went unanswered: `Unresolved { span, reason, detail }`
(`src/types.rs:745`) with the closed vocabulary at `src/types.rs:756`. Rename
saves nothing per site. `RenameStop` (`src/types.rs:2904`) is a whole-run refusal
with four arms mapped to exit 3/4/5/6 in `src/0_rename.rs:44-49`, and only the
`Dynamic` arm names sites at all, through `Vec<SymbolSeat>`.

A site like `v.push(x)` whose receiver no leg typed is in neither the plan nor
any emitted record. The run exits and the site is invisible.

```prolog
rename_abstain(File, Span, SymbolName, Reason, ReceiverText) :-
    rename_request(_, SymbolName),
    unresolved(Span, Reason, ReceiverText),
    site(Span, SymbolName, null),
    file_of(Span, File).
```

This gives `ryi rename` a third exit shape: plan emitted, N sites abstained,
each listed with file, span, reason and receiver text. The reader closes them by
hand or by the slow lane (`--verify-scip`, checker tier).

The abstain count is also the recall metric for @local-binding-inference: every
abstain that the local-binding leg later resolves moves out of the list and into
the plan.

The existing policy stays: an arm emits a full plan or none. Abstains are
reported alongside a plan that covers the sites the arm did resolve, and the
exit code says whether abstains exist.

## Acceptance Criteria
- [ ] `RenameAbstain` row in `src/types.rs`, in the tsp schema and on the flat wire
- [ ] `ryi rename` emits one abstain row per unresolved site naming the symbol under rename
- [ ] a distinct exit code for "plan emitted, abstains present", documented next to the RenameStop codes
- [ ] `--json` carries the abstain list
- [ ] test: a fixture with one typed receiver and one untyped receiver yields a one-seat plan and a one-row abstain list, inline-snapshotted

## Tests Run

## Implementation Notes
