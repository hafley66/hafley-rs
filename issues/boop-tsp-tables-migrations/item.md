---
created: 2026-10-02
updated: 2026-10-02
type: feature
reporter: claude-375
status: open
priority: normal
epic: burndown-2026-10
labels: [boop, tsp, sql]
---

# boop: tables + migrations from TypeSpec (Atlas CE)

## Description

Slices 2-3 of plans/2026-10-02-boop-tsp-cli-db.md. v40 floor (user 2026-10-02). tables.tsp (port boop2 2_tables.tsp, refresh to v40) -> hafley-tsp packages/sql -> schema.sql; Atlas community edition (Apache-2.0, release.ariga.io/atlas/atlas-community-darwin-arm64-latest) migrate diff -> migrations/NN.sql -> rusqlite_migration. Emitter rules from dry-run: no inline UNIQUE, emit CREATE UNIQUE INDEX <table>_<col>; views (9) + triggers (2) outside atlas, recreated per migration; strip sqlite_sequence. sqlite3def rejects columns sequence/query/key/value; prisma migrate diff drops CHECK and adds AUTOINCREMENT.
