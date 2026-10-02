---
created: 2026-10-02
updated: 2026-10-02
type: bug
reporter: claude-375
status: open
priority: low
epic: burndown-2026-10
labels: [boop]
---

# boop --version prints (unknown) under rcargo

## Description

rcargo wrapper (~/projects/sparkup/scripts/rcargo-bin/cargo) builds into target/rcargo/aarch64-apple-darwin; BOOP_BUILD_SHA not stamped. Installed binary prints 'boop 0.0.10 (unknown)'.
