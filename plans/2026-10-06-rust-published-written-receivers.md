# Written receivers through published rust-analyzer APIs

Base: `76d19fd6`. Reference only: `182cdd03`, retained as
`reference/ryi-written-182cdd03`. The branch was reset before this implementation.

`RYI_FAST_RECEIVERS=written` and `--fast-receivers written` select the same
request-scoped policy. Default fast continues to abstain `needs_types`.

The implementation depends on published `ra_ap_hir_def` and `ra_ap_hir_ty`
`=0.0.352`, plus their published `ra-ap-rustc_type_ir` `=0.166.0` API.
There are no rust-analyzer patches, vendored sources, or private API calls.

| Operation | Public API used from hafley_scm |
| --- | --- |
| Declaration owner / source identity | `Semantics::store_owner_for`, `ExpressionStore::with_source_map`, `Body::with_source_map` |
| Lexical scope / value path | `ExprScopes`, `resolver_for_scope`, `Resolver::resolve_path_in_value_ns_fully` |
| Path absent from the body store | `ExprCollector::new` / `lower_path` |
| Written type | `TyLoweringContext::new` / `lower_ty` |
| Self, fields, signature returns | `HirDatabase::impl_self_ty`, `field_types`, `callable_item_signature` |
| Method selection | `MethodResolutionContext::probe_for_name`, `HirDatabase::lookup_impl_method` |
| Function identity | Public `TryFrom<Function>` / `From<FunctionId>` conversions |

Method selection uses RA's local solver context with the declaration environment.
It does not request caller-body inference. Binding identity, annotations, signature
returns and fields supply receiver types. Unsupported expressions abstain.

Without sysroot Try lang items, RA can omit `?` operands from its body store.
The glue lowers their source paths with the public collector and uses the nearest
stored expression scope. No copied RA implementation is needed.

## Validation

The 30-case table and pinned receiver fixture are byte-identical to `182cdd03`.
The table passed with both env and flag selection.

| Soopy comparison | Baseline misses | Recovered | Wrong | Remaining |
| --- | ---: | ---: | ---: | ---: |
| Initial public-API diagnostic, before missing-path correction | 143 | 41 | 0 | 102 |
| Final published implementation | 143 | 91 | 0 | 52 |

The final measurement reran only the changed written provider; baseline and SCIP
oracle outputs were reused. The 91 physical call sites and destinations exactly
match the reference: reference-only 0, published-only 0. Raw JSON, SQLite outputs
and logs remain under gitignored `crates/sprefa-extract/bench/rust-resolution-agreement/`.

One full `cargo test --features cli` run: 1257 passed, 1 failed, 19 ignored
in the combined integration suite; library tests 7 passed, CLI tests 18 passed.
The failure was `t_178_generated_contract`: temporary TypeSpec `http` and
`streams` links targeted absent root dependencies. Both now point to existing
package-local dependencies, with no installation. Its targeted rerun passed.
No source or generated artifact changed for this repair. `t_186`, `t_207` and
`t_214` passed in the full run. Doc tests passed (0 tests).
