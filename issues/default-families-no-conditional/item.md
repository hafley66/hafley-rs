---
created: 2026-09-19
updated: 2026-09-20
type: feature
status: done
priority: normal
epic: extract-parity-move-rename
related: ['@cst-out-of-default', '@lab-scopegraph-queries', '@extract-graph-verb']
labels: [extract, artifact-cli, intent-architecture]
closed: 2026-09-20
---

# one default family set for every language, and guessed calls instead of a parse tree

## Description

## Description

User-set 2026-09-19, superseding the per-language table in @cst-out-of-default:
one default for every language, no conditional, cst never in it.

`ryi src/main.ts` prints 10852 lines for a 587-line file and 7571 of them are the
parse tree. The default family set is the union of every option, which is an
unmade decision.

The reflex fix is a conditional: drop cst for languages that have better planes,
keep it for the 18+ extensions that have nothing else (sh, md, html, rb, lua,
css, java, c, cpp). That conditional is rejected. It makes the tool's output
shape a thing you look up rather than a thing you know, and it exists only to
avoid printing nothing.

There is a better answer than printing a parse tree: guess the calls.

## The rule

| | |
| --- | --- |
| default families, every language | `call, type, df` |
| cst | opt-in via `--family cst`, always |

No branch on language. No branch on file content (that is what the rejected
implementation at 32f82d98 does: it extracts with `FamilyMask::ALL` and prunes
based on which bundles answered, so two files of one language differ).

## A guessed call plane for languages with no front-end

Every tree-sitter grammar names its call nodes. The python fixture's cst output
already shows `kind:"call"` with an `identifier` child and an `argument_list`
sibling. That is a call site with a name attached, sitting in the CST, in every
one of those 18 languages.

A generic CST rule mints sites without a per-language front-end:

- match any node whose kind is in a call-kind table (`call`, `call_expression`,
  `method_invocation`, `function_call_expression`, and the rest, collected from
  the grammars actually loaded)
- callee name from the first identifier descendant
- span from the node

These are ordinary rows in the existing vocabulary. No new record type and no
new `ResolutionOrigin` variant: a guessed site either binds through
`corpus_unique` or lands in `unresolved` with a reason, and the confidence
column already says how much to trust it.

A guessed site that resolves is worth more than 211 lines of parse tree. One
that does not is a single `unresolved` row instead of silence.

## When nothing answers, disclose

The crate already carries this doctrine. @extract-graph-verb states it: a state
that cannot answer prints what it can plus the commands that would answer it.

```
$ ryi vendor/blob.xyz
0 facts. No Source matches .xyz.
  ryi --family cst vendor/blob.xyz    the parse tree, if a grammar loaded
  ryi --schema                        which extensions have a Source
```

Never a bare empty stream, never a refusal.

## Relationship to the existing work

`improvement/cst-out-of-default` at 32f82d98 implements the rejected
per-file conditional. Its gate is green and its receipts check out (default
3281, `--family cst` 7571, sum 10852; sh/md/html byte-identical), but the rule
is wrong and it should not merge as-is. Salvageable from it: the golden
regeneration of `tests/fixtures/kind_vocab/wire_golden.jsonl`, and the explicit
mask now passed by `tests/4_capability_parity.rs`, which is correct either way.

## Acceptance Criteria
- [ ] the default family set is one constant, `call,type,df`, with no branch on language or file
- [ ] a call-kind table drives a generic CST-derived call plane for any language lacking a front-end
- [ ] `ryi foo.sh` emits guessed call sites, not a parse tree and not nothing
- [ ] guessed sites use the existing `resolved_edge` / `unresolved` vocabulary, with no new record type and no new ResolutionOrigin variant
- [ ] a file that yields zero facts prints the disclosure block naming the next commands, exit 0
- [ ] `ryi --family cst <anything>` is byte-identical to today
- [ ] `ryi src/main.ts` drops to the non-cst count (3281 on 252a7346)
- [ ] cargo test --features cli --no-fail-fast, zero failures

## Tests Run

## Implementation Notes

The call-kind table is the only per-language artifact and it is data, not code.
Adding a language contributes a row, never a branch.

Related: @lab-scopegraph-queries is the larger version of this idea, per-language
`.scm` query files with fixed capture names over one engine. The call-kind table
here is the smallest useful slice of that, and should be written so it does not
have to be thrown away if the lab lands.

## Decisions

### 2026-09-20T17:52:18Z · @chris

landed on origin/main as 9b25f783; closed 2026-09-20 board sweep
