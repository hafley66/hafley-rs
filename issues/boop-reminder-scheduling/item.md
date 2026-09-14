---
created: 2026-09-14
updated: 2026-09-14
type: feature
status: open
priority: normal
---

# Schedule expiring Boop reminders

## Description

Preserved candidate: archive/boop-cleanup-20260914/feature/boop-reminders-instant at 09b65565a60848340e5c76aa8b781e4298f50dd0. Expiring reminders scheduled through existing routes; crates/boop-store/src/1_reminder.rs and delivery/supervision scheduling are absent from current main. This is historical unintegrated source, not a current test pass. Review and port the reminder slice onto main, without replaying superseded favorite code. Worktree removed after retaining the exact Git ref.
