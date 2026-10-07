# Recon brief: does installed ryi/ryii pass every v5 call + dataflow golden, plus v6 RTKQ

Read-only. No commits, no code edits in hafley-rs. Never run the v5 binary (user rule); read v5
tests as specifications only. Write results to plans/recon/2026-10-07-v5-golden-parity.md + .tsv.

## Scope
1. ~/projects/sprefa/v5/tests/it/*.rs: every test whose claim is about call extraction, call
   resolution, JSX (flow_jsx.rs, type_graph_js.rs, ...), dataflow/prop flow, template parts,
   string fns, unresolved. List the files you include and the ones you exclude with a reason.
2. v6 RTK Query golden: ~/projects/sprefa/v6/tsv2/labs/1_rtkq-extraction-golden.ts, its DL6
   program and fixtures (sprefa-extract/tests/fixtures/ast_pattern/0_rtkq.ts, 2_rtkq_edited.ts).
   Expected: 9 capture rows initial, 6 after edit, exact byte spans.
3. Rust + TS only (hafley-rs CLAUDE.md).

## Method per test case
- Write the test's fixture source to a scratch dir under plans/recon/v5-golden/<case>/ (small).
- Run installed `ryii` / `ryi` (built from main 689c2b71): `ryii <files> --resolve`,
  `ryi graph --callers|--from|--flow-path ...`, `ryi query ...` as fits the claim.
- Map each positive AND negative assertion of the v5 test to a ryi command + the row that
  satisfies it (or its absence). Byte offsets are 0-based UTF-8.
- Verdict per assertion: pass / fail (wrong answer) / missing (no record kind for it) / abstain.

Worked example already verified: flow_jsx.rs `jsx_props_flow_into_the_component_by_name`:
`ryi graph --flow-path src/app.tsx@173:179 src` reaches 15:20 (Card's `title` param) and 90:95,
and never 181:193 (`note`). jsx_element/jsx_attribute rows present.

## Report
TSV columns: v5_file, test_name, assertion, ryi_command, verdict, evidence_row.
md: counts per verdict per v5 file; a list of each fail/missing with the exact command to repro.
