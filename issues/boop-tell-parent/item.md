---
created: 2026-08-18
updated: 2026-08-18
type: feature
status: open
priority: high
labels:
- area:boop
---

# Boop derives parent route for tell-parent

## Description

A registered lane already has caller identity and a parent edge. Add a least-argument command such as  that resolves the caller through the harness identity trait, resolves its parent from the registered edge, sends through the existing mail/delivery path, and prints the message ID. No caller-supplied parent route. Named errors for missing or ambiguous caller/parent. Add CLI help and deterministic lane/native-agent tests.

## Decisions

### 2026-08-19T00:05:57Z · @codex

Required surface: boop tell-parent --kind completion --body TEXT. The command derives caller identity through the existing harness trait, then derives the parent route from the registered parent edge. It returns the created mail message ID. The caller never supplies a parent route.
