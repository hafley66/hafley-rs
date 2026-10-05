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
