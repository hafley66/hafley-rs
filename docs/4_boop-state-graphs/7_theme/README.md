# Boop diagram color assignments

`0_sources/` retains the audited source diagrams. The matching JSON manifests
assign edge colors through the reusable kit in the sibling `claude-research` repo.
The generated D2, SVG, PNG and inline Markdown fences remain in the parent directory.

```bash
node docs/4_boop-state-graphs/7_theme/2_rebuild.mjs
```

Requires sibling repositories `hafley-rs` and `claude-research`, Node, D2 and
`rsvg-convert`. Change `theme`, `palette`, `mode` or `seed` in a manifest, then
rebuild. Theme/palette options are documented in
`claude-research/skills/d2-authoring/theme-kit/README.md`.

ER, flow and classifier edges use source groups; the sequence uses operation phases.
State charts use named semantic roles. Original dashed logical references remain
dashed. Labels and graph relationships are preserved; these colors add no claim
about foreign-key enforcement or runtime behavior. `*.legend.json` records each
group's color and default dash.
