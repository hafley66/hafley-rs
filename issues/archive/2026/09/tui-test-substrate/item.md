---
created: 2026-09-17
updated: 2026-09-27
type: improvement
status: obsolete
priority: normal
related: ['@mux-paste-path']
labels: [domain-boop, testing]
closed: 2026-09-27
---

# Consolidate Boop harness and terminal substrate for deterministic live TUI tests

## Description

## Plan
[Shared cleanup plan in Instant](../../../instant/plans/2026-09-17-tui-testing/0_plan.md).

## Scope
Audit boop-harness launch recipes against Instant scripts/2_agentTuiReplay.ts. Specify boop-mux TerminalSnapshot physical rows, retained-history viewport origin, copy-mode scroll coordinates and generation coherence. Resolve socket/environment selection consistently across capture and input. Consolidate isolated test-session ownership and deterministic scenario fixtures using existing llmock and Microsoft TUI integrations.

## Boundaries
Preserve mux-paste-path ownership and its input API decision. Renderer projection and browser assertions remain in Instant. Start with one harness; expand the matrix when launch adapters change. No real-model spending or daily-driver process mutation.

## Acceptance
Targeted tests exercise actual CLI requests and terminal input/output. Capture coordinates describe a coherent generation. Wrapped text maps to physical rows. Setup and teardown own only validated scratch resources. Request, subprocess and wall-time caps are enforced; failure receipts include mock requests, transcript, grid, viewport and exits.

## Cross-repository consumer
[Instant renderer tracker](../../../instant/issues/tui-renderer-testing/item.md) covers active-turn scrolling, transient tool-gap rendering and final feed wake.

## Open question
Pinned llmock v0.1.2 supports tool-call responses but the current fixture path cannot deterministically match subsequent tool results. Confirm sequence support or a cassette before A/tool-only/B acceptance.

## Resolution

### 2026-09-27T05:03:09Z · @issuectl

Repro receipt: ~/.cargo/bin/boop --help lists loopback llmock recipes for every harness; the current card is a consolidation proposal and no runtime failure reproduces.
