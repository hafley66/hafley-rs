# ryi deslop dogfood

Pass 1 manual edits before the addendum: containment, ownership context, edge constructor, dead expression-body branch. Subsequent moves and renames require ryi dry-run and apply, with gaps recorded below.

Analysis inputs:

- crates/hafley_scm/src/read/0c_arg_fields.rs
- crates/hafley_scm/src/read/0c_call_owner.rs
- crates/hafley_scm/src/read/0c_closure_flow.rs
- crates/hafley_scm/src/read/0c_flat_fact.rs
- crates/hafley_scm/src/read/0d_flow.rs
- crates/hafley_scm/src/read/lang/ts/0_df_rows.rs
- crates/hafley_scm/src/read/lang/ts/1_df.rs
- crates/hafley_scm/src/read/lang/ts/2_df_expr.rs
- crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs
- crates/hafley_scm/src/read/lang/ts/4_call_closure.rs
- crates/sprefa-extract/src/0d_graph_flow.rs
- crates/sprefa-extract/src/bin/ryi/0c_lines.rs
- crates/hafley_scm/src/read/lang/ts.rs
- crates/hafley_scm/src/read/types.rs
- crates/hafley_scm/src/read/dispatch.rs
- crates/sprefa-extract/src/0_graph.rs
- crates/sprefa-extract/src/bin/ryi/0_sqlite.rs

## Pass 1 query commands

- `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii query --lang rust --query "(function_item name: (identifier) @name) @definition" <analysis inputs>`: 40 rows, exit 0, 0.051s.
- `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii query --lang rust --query "(binary_expression operator: [\">=\" \"<=\"] @operator) @shape" <analysis inputs>`: 1 rows, exit 0, 0.062s.
- `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii query --lang rust --query "(field_expression value: (field_expression field: (field_identifier) @aux) field: (field_identifier) @field (#eq? @aux \"aux\") (#eq? @field \"functions\")) @shape" <analysis inputs>`: 5 rows, exit 0, 0.069s.
- `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii query --lang rust --query "(call_expression function: (field_expression field: (field_identifier) @method) arguments: (arguments (struct_expression name: (type_identifier) @type)) (#eq? @method \"push\") (#eq? @type \"FlowEdge\")) @shape" <analysis inputs>`: 0 rows, exit 0, 0.066s.
- `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii query --lang rust --query "(call_expression function: (field_expression field: (field_identifier) @method) (#eq? @method \"strip_prefix\")) @shape" <analysis inputs>`: 2 rows, exit 0, 0.060s.
- `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii query --lang rust --query "(call_expression function: [(identifier) @callee (scoped_identifier name: (identifier) @callee)]) @site" <analysis inputs>`: 264 rows, exit 0, 0.060s.

## Pass 1 caller commands

- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_arg_fields.rs#targets <analysis inputs>`: 0 rows, exit 0, 0.190s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_arg_fields.rs#objects <analysis inputs>`: 0 rows, exit 0, 0.198s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_arg_fields.rs#parameter_object <analysis inputs>`: 0 rows, exit 0, 0.235s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_call_owner.rs#covering_def <analysis inputs>`: 0 rows, exit 0, 0.191s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_call_owner.rs#call_node <analysis inputs>`: 0 rows, exit 0, 0.165s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_closure_flow.rs#emit <analysis inputs>`: 0 rows, exit 0, 0.153s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#as_str <analysis inputs>`: 0 rows, exit 0, 0.181s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#new <analysis inputs>`: 0 rows, exit 0, 0.166s; 30 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#flow_edges <analysis inputs>`: 0 rows, exit 0, 0.143s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_seed_params <analysis inputs>`: 0 rows, exit 0, 0.146s; 6 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_push <analysis inputs>`: 0 rows, exit 0, 0.173s; 35 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_edge <analysis inputs>`: 0 rows, exit 0, 0.177s; 24 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_loop_row <analysis inputs>`: 0 rows, exit 0, 0.158s; 4 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_owner <analysis inputs>`: 0 rows, exit 0, 0.147s; 9 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#project <analysis inputs>`: 0 rows, exit 0, 0.167s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_stmt <analysis inputs>`: 0 rows, exit 0, 0.149s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_decl <analysis inputs>`: 0 rows, exit 0, 0.161s; 1 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_class <analysis inputs>`: 0 rows, exit 0, 0.166s; 3 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_body <analysis inputs>`: 0 rows, exit 0, 0.169s; 5 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_lift_fn <analysis inputs>`: 0 rows, exit 0, 0.163s; 3 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_lift_arrow <analysis inputs>`: 0 rows, exit 0, 0.172s; 2 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_body_stmt <analysis inputs>`: 0 rows, exit 0, 0.164s; 9 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_for_in_of <analysis inputs>`: 0 rows, exit 0, 0.169s; 2 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_call <analysis inputs>`: 0 rows, exit 0, 0.168s; 2 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_member <analysis inputs>`: 0 rows, exit 0, 0.173s; 5 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/2_df_expr.rs#df_flow_expr <analysis inputs>`: 0 rows, exit 0, 0.178s; 43 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#df_jsx_element <analysis inputs>`: 0 rows, exit 0, 0.185s; 3 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#df_jsx_fragment <analysis inputs>`: 0 rows, exit 0, 0.182s; 3 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#children <analysis inputs>`: 0 rows, exit 0, 0.170s; 2 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#element <analysis inputs>`: 0 rows, exit 0, 0.166s; 2 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_jsx_element <analysis inputs>`: 0 rows, exit 0, 0.167s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_object_property <analysis inputs>`: 0 rows, exit 0, 0.166s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_arrow_function_expression <analysis inputs>`: 0 rows, exit 0, 0.171s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_function <analysis inputs>`: 0 rows, exit 0, 0.171s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_variable_declarator <analysis inputs>`: 0 rows, exit 0, 0.174s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_assignment_pattern <analysis inputs>`: 0 rows, exit 0, 0.170s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/0d_graph_flow.rs#flow_seed <analysis inputs>`: 0 rows, exit 0, 0.172s; 0 written-call query rows.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/bin/ryi/0c_lines.rs#line_col <analysis inputs>`: 0 rows, exit 0, 0.166s; 2 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/bin/ryi/0c_lines.rs#decorate_lines <analysis inputs>`: 0 rows, exit 0, 0.148s; 3 written-call query rows.
  Gap: expected resolved callers for the captured written calls; got 0. Zero is a delete candidate requiring binding evidence. Retained pending that evidence.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/bin/ryi/0c_lines.rs#whole_call_rows_include_end_lines_at_newline_boundaries <analysis inputs>`: 0 rows, exit 0, 0.146s; 0 written-call query rows.

## Pass 1 cleave dry run

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii cleave crates/hafley_scm/src/read/0c_call_owner.rs#covering_def crates/hafley_scm/src/read/0d_flow.rs --root . --json`

Expected a plan to move covering_def into the flow module and repair its reexport. Exit 0, 14.832s.

```text
root /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-v5-parity
plan crates/hafley_scm/src/read/0c_call_owner.rs#covering_def -> crates/hafley_scm/src/read/0d_flow.rs
travel CallF from family::CallF as crate::read::types (relative)
travel CallKind from family::CallKind as crate::read::types (relative)
travel FamilyBundle from rows::FamilyBundle as crate::read::types (relative)
travel NodeRef from shape::NodeRef as crate::atoms (relative)
travel Span from shape::Span as crate::span (relative)
drag fixpoint 1 passes
caller crates/hafley_scm/src/read/types.rs
replace crates/hafley_scm/src/read/0c_call_owner.rs -> crates/hafley_scm/src/read/0c_call_owner.rs  update crates/hafley_scm/src/read/0c_call_owner.rs (1023 bytes)
    --- original
    +++ modified
    @@ -1,49 +1,5 @@
     use super::*;
    
    -/// Explicit deferred sites bind to their closure; eager sites select the
    -/// tightest covering definition, excluding deferred-only closure spans.
    -pub fn covering_def(defs: &FamilyBundle<CallF>, site: Span) -> Option<NodeRef> {
    -    if let Some((_, owner)) = defs
    -        .aux
    -        .deferred_sites
    -        .iter()
    -        .find(|(call, _)| *call == site)
    -    {
    -        return defs
    -            .nodes
    -            .iter()
    -            .position(|node| node.kind == CallKind::Lambda && node.span == *owner)
    -            .map(|index| NodeRef(index as u32));
    -    }
    -    let mut best: Option<(Span, NodeRef)> = None;
    -    for (ix, node) in defs.nodes.iter().enumerate() {
    -        let span = node.span;
    -        if defs
    -            .aux
    -            .deferred_sites
    -            .iter()
    -            .any(|(_, deferred)| *deferred == span)
    -        {
    -            continue;
    -        }
    -        if !span.contains(site) {
    -            continue;
    -        }
    -        let key = (span.end() - span.start, span.start, span.end());
    -        let better = match best {
    -            None => true,
    -            Some((b, _)) => {
    -                let bkey = (b.end() - b.start, b.start, b.end());
    -                key < bkey
    -            }
    -        };
    -        if better {
    -            best = Some((span, NodeRef(ix as u32)));
    -        }
    -    }
    -    best.map(|(_, r)| r)
    -}
    -
     /// The caller's call node at `site`: the `CallRes`/`New` node whose span equals
     /// the site, else the smallest such span containing it, else `None`.
     pub(super) fn call_node(bundle: &FamilyBundle<DfF>, site: Span) -> Option<NodeRef> {
replace crates/hafley_scm/src/read/0d_flow.rs -> crates/hafley_scm/src/read/0d_flow.rs  update crates/hafley_scm/src/read/0d_flow.rs (6881 bytes)
    --- original
    +++ modified
    @@ -1,4 +1,7 @@
     use super::*;
    +use crate::read::types::{CallF, CallKind, FamilyBundle};
    +use crate::atoms::NodeRef;
    +use crate::span::Span;
    
     #[path = "0c_arg_fields.rs"]
     mod arguments;
    @@ -161,3 +164,47 @@
         });
         edges
     }
    +
    +/// Explicit deferred sites bind to their closure; eager sites select the
    +/// tightest covering definition, excluding deferred-only closure spans.
    +pub fn covering_def(defs: &FamilyBundle<CallF>, site: Span) -> Option<NodeRef> {
    +    if let Some((_, owner)) = defs
    +        .aux
    +        .deferred_sites
    +        .iter()
    +        .find(|(call, _)| *call == site)
    +    {
    +        return defs
    +            .nodes
    +            .iter()
    +            .position(|node| node.kind == CallKind::Lambda && node.span == *owner)
    +            .map(|index| NodeRef(index as u32));
    +    }
    +    let mut best: Option<(Span, NodeRef)> = None;
    +    for (ix, node) in defs.nodes.iter().enumerate() {
    +        let span = node.span;
    +        if defs
    +            .aux
    +            .deferred_sites
    +            .iter()
    +            .any(|(_, deferred)| *deferred == span)
    +        {
    +            continue;
    +        }
    +        if !span.contains(site) {
    +            continue;
    +        }
    +        let key = (span.end() - span.start, span.start, span.end());
    +        let better = match best {
    +            None => true,
    +            Some((b, _)) => {
    +                let bkey = (b.end() - b.start, b.start, b.end());
    +                key < bkey
    +            }
    +        };
    +        if better {
    +            best = Some((span, NodeRef(ix as u32)));
    +        }
    +    }
    +    best.map(|(_, r)| r)
    +}
replace crates/hafley_scm/src/read/lang/go_checker.rs -> crates/hafley_scm/src/read/lang/go_checker.rs  update crates/hafley_scm/src/read/lang/go_checker.rs (18766 bytes)
    --- original
    +++ modified
    @@ -265,7 +265,7 @@
             else {
                 continue;
             };
    -        let Some(src) = crate::read::types::covering_def(call, site.span) else {
    +        let Some(src) = crate::read::types::flow::covering_def(call, site.span) else {
                 continue;
             };
             edges.push(
replace crates/hafley_scm/src/read/types.rs -> crates/hafley_scm/src/read/types.rs  update crates/hafley_scm/src/read/types.rs (107429 bytes)
    --- original
    +++ modified
    @@ -1931,7 +1931,6 @@
    
     #[path = "0c_call_owner.rs"]
     mod call_owner;
    -pub use call_owner::covering_def;
    
     /// Same-file name lookup: the CallF def node in `defs` whose interned name is
     /// `name` (the same-file fast path before the corpus `DefIndex` join). Written
    @@ -2554,6 +2553,7 @@
     #[path = "0c_flat_fact.rs"]
     mod flat_fact;
     pub use flat_fact::FlatFact;
    +pub use crate::read::types::flow::covering_def;
    
     impl FlatFact {
         /// The row's `fact` ordinal slot, for the arms `flatten_each` numbers. The
stage c2778d9e2ee74875652f3cfa87b8d3d58a9493ef45f05d8b1f222d42611ce2c9 dry run, tree untouched
{"record":"cleave_plan","src":"crates/hafley_scm/src/read/0c_call_owner.rs","dest":"crates/hafley_scm/src/read/0d_flow.rs","item":"covering_def","item_span":{"start":15,"len":1328},"travelling":[{"name":"CallF","module":"family::CallF","dest_module":"crate::read::types","span":{"start":1754,"len":5},"kind":"relative"},{"name":"CallKind","module":"family::CallKind","dest_module":"crate::read::types","span":{"start":1761,"len":8},"kind":"relative"},{"name":"FamilyBundle","module":"rows::FamilyBundle","dest_module":"crate::read::types","span":{"start":4463,"len":12},"kind":"relative"},{"name":"NodeRef","module":"shape::NodeRef","dest_module":"crate::atoms","span":{"start":5725,"len":7},"kind":"relative"},{"name":"Span","module":"shape::Span","dest_module":"crate::span","span":{"start":5734,"len":4},"kind":"relative"}],"orphans":[],"callers":["crates/hafley_scm/src/read/types.rs"],"dragged":[],"drag_iterations":1,"unresolved":[]}
```

Cleave gap: the plan rewrites a Go caller from the public `crate::read::types::covering_def` to `crate::read::types::flow::covering_def`, although `flow` is private. Expected the public reexport to preserve callers. After applying the move, retain the existing public path and remove redundant imports/module wiring manually. The dry run reports no unresolved names despite this invalid path.

Caller repro: `ryii graph --callers objects crates/hafley_scm/src/read/0c_arg_fields.rs` returned 0 rows (the trace recorded 23.8ms busy time; wall time was not captured); `ryii --kinds call crates/hafley_scm/src/read/0c_arg_fields.rs` emitted the `objects` definition and the written `objects` call site at 242:249. `ryii graph --callers df_owner .../ts/0_df_rows.rs .../ts/1_df.rs` returned 0 rows (wall time was not captured).

`ryii cleave --list plans/0_deslop_cleave.tsv --root . --json`: exit 2, 0.026s. Two item plans; preserves the same public-path gap.

List/JSON gap: `error: the argument '--list <LIST>' cannot be used with '--json'`. Help lists both without explaining the incompatibility. Retried without JSON: exit 0, 13.563s, 2 plan lines.

`ryii cleave --list plans/0_deslop_cleave.tsv --root . --commit`: 2 plans, exit 0, 14.036s.

## Pass 2 query

`ryii query --lang rust --query (call_expression function: (identifier) @name (#eq? @name "dispatch")) @shape crates/sprefa-extract/tests/33_v5_parity_matrix.rs crates/sprefa-extract/tests/200_v5_call_lines.rs crates/sprefa-extract/tests/201_v5_owners.rs crates/sprefa-extract/tests/199_v5_labeled_break.rs crates/sprefa-extract/tests/203_v5_reach.rs crates/sprefa-extract/tests/199_v5_parity.rs crates/sprefa-extract/tests/202_v5_jsx.rs crates/sprefa-extract/tests/205_deferred_jsx.rs`: 6 JSON rows, exit 0, 0.179s.

## Pass 2 move

`ryii move crates/sprefa-extract/tests/199_v5_parity.rs crates/sprefa-extract/tests/all.rs --root .`: 0 JSON rows, exit 2, 0.055s.
Expected test-suite integration into existing all.rs. Got:

```text
move destination already exists: /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-v5-parity/crates/sprefa-extract/tests/all.rs
```
File move cannot merge modules into an existing test binary. Cargo/module wiring and shared-setup edits require manual edits.

## Pass 2 shared setup cleave dry run

`ryii cleave crates/sprefa-extract/tests/201_v5_owners.rs#rows crates/sprefa-extract/tests/204_v5_support.rs --root . --json`: 0 JSON rows, exit 2, 10.252s.

Shared setup first dry run gap: `cleave source crates/sprefa-extract/tests/201_v5_owners.rs is in no module of the Cargo workspace rust-analyzer loaded at /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-v5-parity`. Expected the declared v5_parity test target. Retried with the excluded crate root.

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii cleave crates/sprefa-extract/tests/201_v5_owners.rs#rows crates/sprefa-extract/tests/204_v5_support.rs --root crates/sprefa-extract --json`: exit 0, 5.204s.

```text
rap();
    +    flatten_jsonl(&out)
    +        .into_iter()
    +        .map(|row| serde_json::from_str(&row).unwrap())
    +        .collect()
    +}
stage a111d04f8d35a586e3784c74f880340344ea9de7b1b3cbfbfe45e660732011ef dry run, tree untouched
{"record":"cleave_plan","src":"tests/201_v5_owners.rs","dest":"tests/204_v5_support.rs","item":"rows","item_span":{"start":417,"len":412},"travelling":[{"name":"Value","module":"serde_json::Value","dest_module":"serde_json::Value","span":{"start":16,"len":5},"kind":"package"},{"name":"dispatch","module":"sprefa_extract::dispatch","dest_module":"sprefa_extract::dispatch","span":{"start":44,"len":8},"kind":"package"},{"name":"flatten_jsonl","module":"sprefa_extract::flatten_jsonl","dest_module":"sprefa_extract::flatten_jsonl","span":{"start":54,"len":13},"kind":"package"},{"name":"FamilyMask","module":"sprefa_extract::FamilyMask","dest_module":"sprefa_extract::FamilyMask","span":{"start":69,"len":10},"kind":"package"}],"orphans":[{"name":"dispatch","module":"sprefa_extract::dispatch","dest_module":"sprefa_extract::dispatch","span":{"start":44,"len":8},"kind":"package"},{"name":"flatten_jsonl","module":"sprefa_extract::flatten_jsonl","dest_module":"sprefa_extract::flatten_jsonl","span":{"start":54,"len":13},"kind":"package"},{"name":"FamilyMask","module":"sprefa_extract::FamilyMask","dest_module":"sprefa_extract::FamilyMask","span":{"start":69,"len":10},"kind":"package"}],"callers":[],"dragged":[],"drag_iterations":1,"unresolved":[]}
```

Pass 3 query orientation: `ryii query --lang rust --query '(call_expression function: (field_expression field: (field_identifier) @method) (#eq? @method "insert")) @shape' crates/hafley_scm/src/read/lang/ts.rs`: 9 rows, exit 0, 0.083s. The closure-name insertion is among those rows.

Shared setup dry-run gap: orphan reporting lists dispatch, flatten_jsonl, and FamilyMask, but the proposed grouped import removes only dispatch. Expected all unused imports to be removed. Manual cleanup will remove the remaining unused imports. The helper move itself is applicable after the crate-root retry.

## Expression editing gap

The read `ryii rename --help`, `move --help`, and `cleave --help` expose symbol renames, file moves and item extraction. They expose no expression replacement, Cargo-target merge, snapshot projection, or removal of an orphaned empty module. Those changes require manual edits after query evidence. No type-checking lane was requested.

`ryii query --lang rust --query '(call_expression function: (field_expression field: (field_identifier) @method) (#eq? @method "strip_prefix")) @shape' crates/hafley_scm/src/lang/rust/12_df_control.rs crates/hafley_scm/src/read/lang/rust/3_df.rs`: 2 rows, exit 0, 0.018s. Both parse the just-built closure symbol to recover its owner. Reuse the already-stored enclosing-function name.

## Pass 1 gate

8 v5_parity tests, 2 t_13_flow_join tests, and 5 t_12_df_identity tests passed. Snapshot bytes unchanged. Initial errors: `error: no test target named `v5_parity` in default-run packages`; `error[E0425]: cannot find type `DfOwner` in this scope` and `error[E0422]: cannot find struct, variant or union type `DfOwner` in this scope` (22 compile errors, corrected by moving the context type to its parent module). Capped attempts printed `STOPPED: parity gate exceeded 110 seconds` and `STOPPED: pass 1 parity gate exceeded 110 seconds`; cached reruns passed.

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii cleave crates/sprefa-extract/tests/201_v5_owners.rs#rows crates/sprefa-extract/tests/204_v5_support.rs --root crates/sprefa-extract --commit`: 1 plan, exit 0, 4.944s. Applied the inspected shared-setup extraction.

Additional pass 1 caller command: `ryii graph --timeout 20 --callers closure_expr crates/hafley_scm/src/lang/rust/12_df_control.rs crates/hafley_scm/src/lang/rust/11_df_syntax_rows.rs`: 0 rows, exit 0, 0.058s. Retained the closure lift; query inspection showed its dispatch arm in the parent.

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii move crates/sprefa-extract/tests/204_v5_support.rs crates/sprefa-extract/tests/198_v5_support.rs --root crates/sprefa-extract`: 1 edit rows, exit 0, 0.068s. Moves the helper before its numbered consumers.

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii move crates/sprefa-extract/tests/204_v5_support.rs crates/sprefa-extract/tests/198_v5_support.rs --root crates/sprefa-extract --commit`: 2 edits, exit 0, 0.108s.

Pass 2 manual setup extraction initially left the old stderr-formatting tail in two files. rustfmt reported `error: unexpected closing delimiter: `}`` at tests/203_v5_reach.rs:88 and tests/205_deferred_jsx.rs:19. Removed those tails before the test compile.

## Pass 3 rename dry run

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii rename crates/sprefa-extract/tests/205_deferred_jsx.rs#deferred_jsx_and_generic_object_capture_flow_have_whole_outputs deferred_jsx_props_captures_and_callers --root crates/sprefa-extract --json`: 0 JSON rows, exit 2, 0.053s. Rename the deferred fixture test to match its projected assertions.
```text
rename anchor is not a file: /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-v5-parity/crates/sprefa-extract/crates/sprefa-extract/tests/205_deferred_jsx.rs
```

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii rename tests/205_deferred_jsx.rs#deferred_jsx_and_generic_object_capture_flow_have_whole_outputs deferred_jsx_props_captures_and_callers --root crates/sprefa-extract --json`: 1 JSON rows, exit 0, 0.563s. Retried with root-relative anchor.

Pass 2 compile correction: `error[E0308]: mismatched types`, `expected struct `RyiOutput`, found struct `Arc<RyiOutput>`` at tests/198_v5_support.rs:5. The shared helper now returns dispatch's Arc directly.

Pass 2 setup proof: `ryii query --lang rust --query '(struct_expression name: (type_identifier) @name (#eq? @name "FamilyMask")) @shape' <v5 test files>`: 2 rows, exit 0, 0.037s. This glob also included the pre-existing 33_v5_parity_matrix.rs mask. The new suite has its mask in the shared helper.

Pass 2 narrowed setup query over 198, 199, 200, 201, 202, 203, 205: 1 row, exit 0, 0.035s.

Pass 2 snapshot setup correction: the initial function-name matcher omitted digits, leaving four tests without the settings guard and naming their snapshots `None`. 12 passed, 4 failed: call_lines::v5_call_definitions_include_the_end_line, jsx::whole_jsx_rows_and_resolved_prop_edges_match_v5_cases, reach::jsx_ten_forward_and_reverse_reach_checks_match_v5, reach::seeded_forward_and_reverse_reach_port_v5_dataflow_cases. Exact assertion text included `snapshot assertion for 'v5_parity__reach__None' failed in line 221`. Corrected the names/guards and deleted rejected .snap.new outputs; original snapshots remain untouched.

## Pass 2 gate

16 passed: 8 v5_parity, 2 flow join, 5 df identity, and the test-module inventory check. No snapshot changes. Removed the previous receipt/error log and separate test target; shared mask count is 1 in the new suite. No branch-added committed *.log files remained.

`/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/target/debug/ryii rename tests/205_deferred_jsx.rs#deferred_jsx_and_generic_object_capture_flow_have_whole_outputs deferred_jsx_props_captures_and_callers --root crates/sprefa-extract --json --commit`: 1 edit, 1 JSON row, exit 0, 0.672s. Applied the inspected test rename.

Pass 3 failure analysis: `ryii query --lang rust --query '(macro_invocation macro: (identifier) @macro (#eq? @macro "format")) @shape' crates/sprefa-extract/tests/69_ts_closure_mirror.rs`: 0 rows, exit 0, 0.042s. Three hash-based expected closure names need the authorized span spelling; expression replacement remains the previously logged edit capability gap.

## Pass 3 helper caller audit
Function query: `ryii query --lang rust --query '(function_item name: (identifier) @name) @definition' crates/sprefa-extract/tests/198_v5_support.rs`: 7 rows, 0.086s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#facts <198–205 v5 test files>`: 1 rows, exit 0, 0.068s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#rows <198–205 v5 test files>`: 0 rows, exit 0, 0.046s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#snapshots <198–205 v5 test files>`: 0 rows, exit 0, 0.044s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#run <198–205 v5 test files>`: 0 rows, exit 0, 0.046s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#node_label <198–205 v5 test files>`: 0 rows, exit 0, 0.047s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#endpoint <198–205 v5 test files>`: 0 rows, exit 0, 0.044s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/tests/198_v5_support.rs#project <198–205 v5 test files>`: 0 rows, exit 0, 0.043s.
Expected callers for facts, rows, snapshots, run, node_label, endpoint and project are present in these fixtures or the helper itself. The graph returned zero for all seven, matching the previously recorded unresolved Rust caller gap. Retained these zero-caller deletion candidates because their written calls and targeted tests exercise them.

Macro query gap above: expected 3 format! invocations containing hash-based closure expectations; got 0 rows, exit 0, 0.05s. Used the bounded file read to inspect the three expressions.

Command limit: the first a44eced8 targeted baseline compile stopped with exact text `STOPPED: baseline targeted command exceeded 110 seconds`; retried from the populated cache. The initial pass 3 projection gate similarly stopped with `STOPPED: pass 3 projection gate exceeded 110 seconds`; its cached retry passed.

Pass 3 parity analysis: `ryii query --lang rust --query '(call_expression function: (identifier) @name (#eq? @name "flatten")) @shape' crates/sprefa-extract/tests/golden_parity.rs`: 2 rows, exit 0, 0.078s. The capture comparison is an expression edit, covered by the logged edit capability gap.

## Final source caller audit (a44eced8 through pass 3)
`ryii query --lang rust --query '(function_item name: (identifier) @name) @definition' <12 changed flow/TS/graph/line files>`: 186 rows, exit 0, 0.072s. Diff-span intersection selected 41 changed/added functions.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_arg_fields.rs#targets <same 16 bounded source files>`: 0 rows, exit 0, 0.154s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_arg_fields.rs#objects <same 16 bounded source files>`: 0 rows, exit 0, 0.154s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_arg_fields.rs#parameter_object <same 16 bounded source files>`: 0 rows, exit 0, 0.150s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0c_closure_flow.rs#emit <same 16 bounded source files>`: 0 rows, exit 0, 0.144s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#as_str <same 16 bounded source files>`: 0 rows, exit 0, 0.141s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#new <same 16 bounded source files>`: 0 rows, exit 0, 0.150s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#flow_edges <same 16 bounded source files>`: 0 rows, exit 0, 0.150s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#covering_def <same 16 bounded source files>`: 0 rows, exit 0, 0.151s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/0d_flow.rs#call_node <same 16 bounded source files>`: 0 rows, exit 0, 0.143s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts.rs#project <same 16 bounded source files>`: 0 rows, exit 0, 0.143s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_seed_params <same 16 bounded source files>`: 0 rows, exit 0, 0.149s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_push <same 16 bounded source files>`: 0 rows, exit 0, 0.140s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_edge <same 16 bounded source files>`: 0 rows, exit 0, 0.141s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_loop_row <same 16 bounded source files>`: 0 rows, exit 0, 0.156s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/0_df_rows.rs#df_owner <same 16 bounded source files>`: 0 rows, exit 0, 0.141s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#project <same 16 bounded source files>`: 0 rows, exit 0, 0.160s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_stmt <same 16 bounded source files>`: 0 rows, exit 0, 0.147s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_decl <same 16 bounded source files>`: 0 rows, exit 0, 0.146s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_class <same 16 bounded source files>`: 0 rows, exit 0, 0.164s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_body <same 16 bounded source files>`: 0 rows, exit 0, 0.144s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_lift_fn <same 16 bounded source files>`: 0 rows, exit 0, 0.144s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_lift_arrow <same 16 bounded source files>`: 0 rows, exit 0, 0.145s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_body_stmt <same 16 bounded source files>`: 0 rows, exit 0, 0.139s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_for_in_of <same 16 bounded source files>`: 0 rows, exit 0, 0.145s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_call <same 16 bounded source files>`: 0 rows, exit 0, 0.149s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/1_df.rs#df_flow_member <same 16 bounded source files>`: 0 rows, exit 0, 0.149s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/2_df_expr.rs#df_flow_expr <same 16 bounded source files>`: 0 rows, exit 0, 0.142s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#df_jsx_element <same 16 bounded source files>`: 0 rows, exit 0, 0.142s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#df_jsx_fragment <same 16 bounded source files>`: 0 rows, exit 0, 0.150s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#children <same 16 bounded source files>`: 0 rows, exit 0, 0.144s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/3_df_jsx.rs#element <same 16 bounded source files>`: 0 rows, exit 0, 0.151s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_jsx_element <same 16 bounded source files>`: 0 rows, exit 0, 0.160s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_object_property <same 16 bounded source files>`: 0 rows, exit 0, 0.142s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_arrow_function_expression <same 16 bounded source files>`: 0 rows, exit 0, 0.147s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_function <same 16 bounded source files>`: 0 rows, exit 0, 0.151s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_variable_declarator <same 16 bounded source files>`: 0 rows, exit 0, 0.140s.
- `ryii graph --timeout 20 --callers crates/hafley_scm/src/read/lang/ts/4_call_closure.rs#visit_assignment_pattern <same 16 bounded source files>`: 0 rows, exit 0, 0.147s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/0d_graph_flow.rs#flow_seed <same 16 bounded source files>`: 0 rows, exit 0, 0.138s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/bin/ryi/0c_lines.rs#line_col <same 16 bounded source files>`: 0 rows, exit 0, 0.142s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/bin/ryi/0c_lines.rs#decorate_lines <same 16 bounded source files>`: 0 rows, exit 0, 0.140s.
- `ryii graph --timeout 20 --callers crates/sprefa-extract/src/bin/ryi/0c_lines.rs#whole_call_rows_include_end_lines_at_newline_boundaries <same 16 bounded source files>`: 0 rows, exit 0, 0.142s.
Zero-caller rows remain deletion candidates. This audit reproduces the documented Rust call-resolution gap; the written calls and passing fixture gates retain the used functions. No resolver changes were made during deslop.

Capture normalizer compile correction: FlatFact uses SpanOut and optional endpoint kinds. Switched the index to byte tuples and matched Some endpoint kinds. Exact diagnostics:
```text
error[E0308]: mismatched types
   --> tests/golden_parity.rs:280:45
    |
280 |                     if !captures.contains(&(from, to)) {
    |                                             ^^^^ expected `Span`, found `SpanOut`

error[E0308]: mismatched types
   --> tests/golden_parity.rs:280:51
    |
280 |                     if !captures.contains(&(from, to)) {
    |                                                   ^^ expected `Span`, found `SpanOut`

error[E0277]: the trait bound `sprefa_extract::SpanOut: std::hash::Hash` is not satisfied
    --> tests/206_v5_capture_parity.rs:20:10
     |
  20 |         .collect();
     |          ^^^^^^^ the trait `std::hash::Hash` is not implemented for `sprefa_extract::SpanOut`
     |
help: the trait `FromIterator<(K, V)>` is conditionally implemented for `std::collections::HashMap<K, V, S>`
    --> /Users/chrishafley/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust/library/std/src/collections/hash/map.rs:3005:1
     |
3005 | / impl<K, V, S> FromIterator<(K, V)> for HashMap<K, V, S>
3006 | | where
3007 | |     K: Eq + Hash,
     | |             ---- unsatisfied requirement introduced here: `(sprefa_extract::SpanOut, &str): std::hash::Hash`
3008 | |     S: BuildHasher + Default,
     | |_____________________________^
     = note: required for `(sprefa_extract::SpanOut, &str)` to implement `std::hash::Hash`
     = note: required for `std::collections::HashMap<(sprefa_extract::SpanOut, &str), &str>` to implement `FromIterator<((sprefa_extract::SpanOut, &str), &str)>`
note: required by a bound in `std::iter::Iterator::collect`
    --> /Users/chrishafley/.rustup/toolchains/stable-aarch64-apple-darwin/lib/rustlib/src/rust/library/core/src/iter/traits/iterator.rs:2077:19
     |
2077 |     fn collect<B: FromIterator<Self::Item>>(self) -> B
     |                   ^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Iterator::collect`

error[E0599]: the method `get` exists for struct `std::collections::HashMap<(sprefa_extract::SpanOut, &str), &str>`, but its trait bounds were not satisfied
    --> tests/206_v5_capture_parity.rs:32:37
     |
  32 |                 let source = owners.get(&(*from, from_kind.as_str()))?;
     |                                     ^^^ method cannot be called due to unsatisfied trait bounds
     |
    ::: /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-v5-parity/crates/hafley_scm/src/read/types.rs:2520:1
     |
2520 | pub struct SpanOut {
     | ------------------ doesn't satisfy `sprefa_extract::SpanOut: std::hash::Hash`
     |
     = note: the following trait bounds were not satisfied:
             `sprefa_extract::SpanOut: std::hash::Hash`
             which is required by `(sprefa_extract::SpanOut, &str): std::hash::Hash`

error[E0599]: no method named `as_str` found for reference `&std::option::Option<std::string::String>` in the current scope
  --> tests/206_v5_capture_parity.rs:32:60
   |
32 |                 let source = owners.get(&(*from, from_kind.as_str()))?;
   |                                                            ^^^^^^ method not found in `&std::option::Option<std::string::String>`

error[E0599]: the method `get` exists for struct `std::collections::HashMap<(sprefa_extract::SpanOut, &str), &str>`, but its trait bounds were not satisfied
    --> tests/206_v5_capture_parity.rs:33:37
     |
  33 |                 let target = owners.get(&(*to, to_kind.as_str()))?;
     |                                     ^^^ method cannot be called due to unsatisfied trait bounds
     |
    ::: /Users/chrishafley/projects/hafley-rs/.boop-worktrees/feature/ryi-v5-parity/crates/hafley_scm/src/read/types.rs:2520:1
     |
2520 | pub struct SpanOut {
     | ------------------ doesn't satisfy `sprefa_extract::SpanOut: std::hash::Hash`
     |
     = note: the following trait bounds were not satisfied:
             `sprefa_extract::SpanOut: std::hash::Hash`
             which is required by `(sprefa_extract::SpanOut, &str): std::hash::Hash`

error[E0599]: no method named `as_str` found for reference `&std::option::Option<std::string::String>` in the current scope
  --> tests/206_v5_capture_parity.rs:33:56
   |
33 |                 let target = owners.get(&(*to, to_kind.as_str()))?;
   |                                                        ^^^^^^ method not found in `&std::option::Option<std::string::String>`

```

Wire preview command correction: `ryii --df tests/fixtures/ts/sample.ts` exited 2 with `error: unexpected argument '--df' found`. Used the uniform test UPDATE_SNAP entry point.

## Thirty-test environment and baseline comparison

`npm ci` in crates/sprefa-extract/ts7: exit 0, added 2 packages, audited 3 packages, 407ms, 0 vulnerabilities. The baseline lockfile is byte-identical; its node_modules points to the installed dependency directory.

Both checkouts ran `KACHE_DISABLED=1 cargo test --features cli --no-fail-fast --lib --test all -- --exact <the 30 names below>`. Initial branch: 5 lib + 20 all passed, 5 all failed. a44eced8: all 30 passed. This compares the isolated reruns after setup; it does not attribute every recovered full-suite failure solely to npm.

### Fixed after npm ci (25)

- `edit::ts7_rename::tests::lsp_stops_on_ambiguous_document_symbols`
- `edit::ts7_rename::tests::lsp_fixture_edits_match_classic_tsserver`
- `edit::ts7_rename::tests::lsp_renames_plain_identifier`
- `edit::ts7_rename::tests::warm_lsp_returns_the_cold_edits`
- `edit::ts7_rename::tests::lsp_selects_an_unpositioned_declaration_from_document_symbols`
- `t_134_ts_binding_legs::a_self_named_initializer_still_binds_the_outer_fn`
- `t_155a_cleave_ts_oracle::overloads_move_and_export_together_and_reexports_are_not_imports`
- `t_155a_cleave_ts_oracle::slow_diagnostic_stops_before_writing`
- `t_155a_cleave_ts_oracle::slow_preserves_default_namespace_and_type_imports`
- `t_155a_cleave_ts_oracle::slow_type_alias_uses_lsp_items_and_diagnostics`
- `t_167_graph_paths::flow_paths_follow_derived_interprocedural_edges`
- `t_168_graph_revision::a_path_added_between_commits_is_reported_once`
- `t_193_ts_rtkq_jsx::written_tsx_calls_are_additive_to_the_original_resolved_site`
- `t_195_ts_lib_globals::a_lib_global_never_binds_a_corpus_twin`
- `t_195_ts_lib_globals::the_lib_declares_dom_and_ecmascript_globals`
- `t_1_resolve_cli::resolve_mode_streams_cross_file_edges`
- `t_23_flow_cli_dispatch::call_and_flow_arms_emit_both_families`
- `t_23_flow_cli_dispatch::flow_is_a_resolve_arm`
- `t_23_flow_cli_dispatch::resolve_without_family_is_byte_identical`
- `t_4_capability_parity::every_library_capability_is_reachable_through_the_binary`
- `t_55_diff_verb::one_to_two_matches_the_hand_derived_rows`
- `t_55_diff_verb::a_dirty_worktree_does_not_change_the_delta`
- `t_98_resolve_witness::one_witness_per_leg_on_a_syntax_run`
- `t_98_resolve_witness::the_flag_off_stream_is_the_committed_golden`
- `t_golden_parity::call_resolve_scip_ratchet_ts`

### Fails on base too (0)

None.

### New on this branch (5), subsequently fixed

- `t_69_ts_closure_mirror::a_module_level_arrow_mirrors_to_the_module`
- `t_69_ts_closure_mirror::nested_arrows_mirror_to_the_named_fn`
- `t_6_kind_vocab::wire_output_is_byte_identical_to_the_kind_vocab_golden`
- `t_golden_parity::ported_facets_match_v5`
- `t_snapshot::ts_uniform_surface`

### Golden changes and reasons

- `tests/fixtures/kind_vocab/wire_golden.jsonl`: 505 DF node rows gain the owning `function` column; after removing that column, all 505 replacements equal the old rows. One new direct capture edge, let_bind 679:689 -> var_read 817:823. Kind tags and all other rows remain identical.
- `tests/fixtures/ts/sample.dff.snap`: update the uniform-surface DF golden for the owning `function` column. Other family snapshots must remain identical.
- Eight parity snapshots shrink to claim projections; `deferred_jsx_proofs` also replaces content-hash closure names with `closure@App:120` and `closure@App:125`. No other fixture behavior changes.
- The captured `lambdas.v5.jsonl` oracle stays unchanged. Its existing facets omit the new cross-closure capture edge; the new 206 test snapshots that extension explicitly.
- The two closure-mirror tests now expect closure@outer:591, closure@outer:622, and closure@<module>:665, preserving all mirror edges.

`ryii graph --timeout 20 --callers crates/sprefa-extract/tests/206_v5_capture_parity.rs#capture_edges <206 + golden_parity>`: 1 rows, exit 0, 0.154s. Expected the two written calls; got no resolved callers, matching the documented gap.

Scratch cleanup first returned `fatal: '/Users/chrishafley/projects/hafley-rs/.boop-worktrees/scratch/ryi-deslop-base' contains modified or untracked files, use --force to delete it`. Status showed only the node_modules symlink. Removed that symlink and retried ordinary worktree removal.

## Final validation

Final immutable rerun: the same 30 tests pass (5 lib, 25 all). Parity gate: 17 passed (the original 8 parity tests, 1 new capture-oracle projection, 2 flow join, 5 identity, and the module inventory). `git diff --check` passed.

Changed .snap files across a44eced8..HEAD plus this pass: 13 files, 977 lines. Uniform DF ownership proof: strip the `function` key from its 39 owned nodes and the snapshot exactly equals its previous contents.

Validation logs moved out of plans and excluded from commits: `/Users/chrishafley/.cache/boop/lanes/feature-ryi-v5-parity/deslop-validation/`. The original full gate failed 30 tests; subsequent isolated reruns with TS7 dependencies and the intended expectation updates pass all 30. No second whole-suite run was performed, following the coordinator's instruction to rerun only the 30 names. Scratch checkout removed.

Dogfood per pass, row counts: pass 1 query results 40 / 1 / 5 / 0 / 2 / 264, caller queries 0, cleave 2 plans; pass 2 setup queries 6 / 2 / 1, helper cleave 1 plan, helper move 2 edits, existing-destination move rejected; pass 3 closure-name query 9, helper definitions 7, flatten calls 2, final source definitions 186 (41 changed/added functions), caller queries 0, rename 1 edit. Exact commands and their walls are recorded above.
