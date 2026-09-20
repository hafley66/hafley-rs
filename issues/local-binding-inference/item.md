---
created: 2026-09-19
updated: 2026-09-20
type: feature
status: deferred
priority: normal
epic: ryi-fast-tier
labels: [extract, artifact-cli, phase-refinement-1, intent-correctness]
related: ['@rename-abstain-record']
blocked_by: ['@scip-ingestion-conformance']
---

# Local binding types: a receiver leg from let-bindings

## Description

## Description

A member call whose receiver is a local binding gets no type today, so lane D's
rule drops it into `unresolved` with reason `inferred`. The binding's type is
derivable from rows already on the wire in three shapes, without an inference
engine.

Facts in hand: `df_node(span, kind, name)` kind `let_bind`; `sig(owner, slot,
pos, ty)` slot `decl`/`ret`; `site(span, callee, callee_path)`;
`method_owner(owner, self_type, trait)`; `df_field(owner, name, value)`.

One new phase-1 row is needed: `edge(kind: "init", from: binding_span, to:
rhs_span)`. No row links a `let_bind` to its initializer expression today.

```prolog
binding_type(Binding, Ty, spelled) :-
    df_node(Binding, let_bind, _),
    sig(Binding, decl, 0, Ty).

binding_type(Binding, Ty, constructor_return) :-
    df_node(Binding, let_bind, _),
    edge(init, Binding, RhsSpan),
    resolved_edge(_, RhsSpan, DefPath, DefSpan, _),
    sig(DefSpan, ret, 0, Ty),
    corpus_type(Ty, _).

binding_type(Binding, Ty, record_literal) :-
    df_node(Binding, let_bind, _),
    edge(init, Binding, LitSpan),
    df_field(LitSpan, _, _),
    literal_type_name(LitSpan, Ty).

binding_type_lost(Binding) :- rebind(Binding, _).
binding_type_lost(Binding) :- edge(mutable_borrow, _, Binding).
binding_type_lost(Binding) :-
    inner_scope_declares(Binding, Shadow), Shadow \= Binding.

receiver_type(CallSpan, Ty, Why) :-
    site(CallSpan, _, null),
    edge(receiver, CallSpan, RecvSpan),
    ref_binds(RecvSpan, Binding),
    binding_type(Binding, Ty, Why),
    \+ binding_type_lost(Binding).

resolved_edge(Caller, CallSpan, DefPath, DefSpan, local_binding) :-
    receiver_type(CallSpan, Ty, _),
    site(CallSpan, Name, null),
    method_owner(DefSpan, Ty, _),
    def(DefSpan, fn, Name),
    file_of(DefSpan, DefPath),
    enclosing_fn(CallSpan, Caller).
```

`ResolutionOrigin::LocalBinding` is a new arm in `src/types.rs:1664` so the lane
B histogram scores it apart from `receiver` and its wrong-target count is
readable before the leg is trusted.

The rules are monotone and additive: when `binding_type` derives nothing the
output is byte-identical to today. Rule 2 reads `resolved_edge`, so the leg runs
in a second stratum after the existing legs and chains
`let a = Foo::new(); let b = a.child(); b.m()`.

`binding_type_lost` is the correctness gate. A false negative costs recall. A
missing kill rule mints a wrong target.

### Worked example, `RustCallDefs::push`

| site | today | with this leg |
| --- | --- | --- |
| `RustCallDefs::push(x)` | `receiver` | unchanged |
| `self.push(x)` in the impl | `self_type` | unchanged |
| `defs.push(x)`, param-typed | `param` | unchanged |
| `let v = Vec::new(); v.push(x)` | `unresolved`/`inferred` | `local_binding` -> `Vec::push` |
| `acc.push(x)`, closure arg | `unresolved`/`inferred` | unchanged; closure param types come from the call site |

## Acceptance Criteria
- [ ] `edge(kind: "init")` emitted by the rust arm for every `let_bind` with an initializer
- [ ] `ResolutionOrigin::LocalBinding` added, wired through `Method`, the sqlite schema and the flat wire
- [ ] the three `binding_type` rules and the three `binding_type_lost` kills implemented in the rust arm
- [ ] RATCHET.tsv gains a `local_binding` row; its wrong_target is 0 on the bench corpora
- [ ] `ryi fast --sqlite f.db <CTF files>` then `SELECT resolution_origin, COUNT(*) FROM resolved_edge WHERE callee_name='push'` shows the `Vec::push` sites bound to the std shim or dropped, never to `RustCallDefs::push`
- [ ] unresolved count for reason `inferred` falls, and the delta is reported

## Tests Run

## Implementation Notes
