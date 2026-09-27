---
created: 2026-09-14
updated: 2026-09-14
type: feature
status: done
priority: normal
closed: 2026-09-14
closed_by: codex
---

# Publish mdBook and workspace rustdoc on GitHub Pages

## Description

User approved mdBook authored book plus rustdoc for every workspace crate, deployed together on GitHub Pages. Workspace crates/*; automatic API index and coverage validation, whole rustdoc output preserved, one Pages Actions artifact. Repo currently public, Pages not configured, admin capability confirmed. Worker docs-book-pages starts with lightweight implementation; heavy build slot pending. Parent owns integration, Pages enablement, deployment and URL verification.

## Resolution

### 2026-09-14T04:52:40Z · @codex

Published https://hafley66.github.io/hafley-rs/ with API at /api/. Main5a11a7c; Pages Actions run34807411267 build+deploy successful. Parent workspace cargo doc --workspace --no-deps --locked passed; guard18 cases and target-rule7 cases passed; coverage checker11 expected targets and23 explicitly nondocumented test/example/build targets. Live HTTP200 verified root, overview, api index, boop and soopy. Task lane/worktree/branch removed. Future main pushes rebuild and redeploy.
