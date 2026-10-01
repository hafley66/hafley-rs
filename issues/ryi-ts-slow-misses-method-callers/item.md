---
created: 2026-10-01
updated: 2026-10-01
type: bug
status: open
priority: normal
---

# ryi TS: slow misses method callers that fast finds; both tiers print every edge twice

## Repro
`cd ~/projects/hafley-rxjs/packages/md && ryii graph --callers render [--slow] src`

| caller | fast | slow |
| --- | --- | --- |
| `src/lib/0_diagramRenderCache.test.ts:7,8,10,11,12,14,15` (`cache.render(...)`, `cache = new DiagramRenderCache()`) | found (name_resolve) | missing |
| `0_Streamdown.plugins.browser.test.tsx` (7 sites), `2_echartsPlugin.browser.test.tsx` (3), `2_marblesPlugin.browser.test.tsx` (3) | found | found (checker_resolve) |

The file is inside tsconfig `include` (`src/**/*`). `--callers trim` (same class, non-test file) agrees in both tiers.

## Second defect
Every `graph_edge` row is printed twice in both tiers (e.g. `0_diagramRenderCache.ts:21 -> :37` twice for `--callers trim`).
