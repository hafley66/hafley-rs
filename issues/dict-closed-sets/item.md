---
created: 2026-10-01
updated: 2026-10-01
type: task
status: done
priority: normal
epic: burndown-2026-10
labels: [boop]
closed: 2026-10-01
---

# Dict fix: 15 closed dict_* tables become enum columns, inside each slice migration

## Description

Closed: dict_attach, attr_key, edekind, harness, mood_name, netkind, observation_source, price_source, role, session_relation_kind, status, trace_classification, trace_delivery, trace_kind, verb. Keep dict_record/request/path (large). Measure the other 14.

## Comments

### 2026-10-02T00:16:34Z · @claude-375

hafley-rs main 7114f7ae, boop2 main 14ccfcd. Schema 40. CHECK audit plans/dict-closed-sets/18_check_audit.tsv: role, raw_verb, model_price.source, sync_root_stamp.harness stay open; claude-transcript value was missing (caught). boop-store 246 passed; suite 0 not-ok; live db v38 untouched. Search 1356/774/777 ms vs 1940/870/884.
