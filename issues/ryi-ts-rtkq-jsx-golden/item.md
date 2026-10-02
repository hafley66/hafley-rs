---
created: 2026-10-02
updated: 2026-10-02
type: feature
reporter: claude-375
status: open
priority: high
labels: [ryi, ts, jsx]
---

# ryi TS: dl5 rtkq + JSX goldens (hook call sites, JSX elements as facts)

## Description

dl5 golden: ~/projects/sprefa/v5/examples/rtkq-op-recovery.dl over examples/openapi-sim/{openapi.json,components.tsx}; test tests/it/string_fns.rs:351. dl5 binds call_site(_,_,hook,p,l). ryii fast on components.tsx (2026-10-02) emits records free_name 14, local 5, occurrence 5, symbol 5 and no call-site record; the 5 hooks (useGetUserQuery, useListUsersQuery, useCreateOrderMutation, useLazyGetUserQuery, useDeleteWidgetMutation) appear only as free_name. Also dl5 examples/flow-jsx.dl. Acceptance: ryii emits call-site and JSX element/attribute facts for .tsx; a ryi port of rtkq-op-recovery yields the same linked/orphan/unhooked diags as dl5 (deleteWidget orphan, createOrder hook present).
