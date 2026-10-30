# TS 7.1 API research (2026-10-30)

Inputs for `ts71-snapshot-slow-tier.md`. Sources at the end.

## 1. Versions (npm, 2026-10-30)

| package | tag | version |
| --- | --- | --- |
| `typescript` | latest | 7.0.2 (no stable JS API; only `typescript/unstable/*`) |
| `typescript` | beta | 7.1.0-beta (2026-10-20) |
| `typescript` | next | 7.1.0-dev.20261030.1 |
| `tsrs` | — | 0.20.1-ts7.1.0-beta |

`crates/sprefa-extract/ts7/package.json` pins 7.0.2. RC about a month after the beta, final after.

## 2. The official API (typescript 7.1)

Entry points (the `/unstable` prefix is gone in 7.1):

| subpath | content |
| --- | --- |
| `typescript/sync` | `API`, synchronous; MessagePack over the server's stdio |
| `typescript/async` | same API, promise-based |
| `typescript/fs` | `createFileSystem`, `createFileSystemWithLib`, `createFileSystemLayer` |
| `typescript/ast`, `/ast/is`, `/ast/factory`, `/ast/visitor`, ... | node types, guards, factory |
| `typescript/proto` | protocol types |

- Server: the native compiler started with `--api`; the client spawns it.
- Snapshot = every project and program at one point in time. `api.createSnapshot({ openProjects })`
  builds one; `snapshot.update(...)` derives the next without a full rebuild; program and project
  ids persist across updates. Objects from different snapshots must not be mixed.
- `API`, `Program`, `Snapshot` are `Disposable`. Not disposing leaks on the Go side (blog).
- File changes are pushed by the client: `fileNotifications` in `snapshot.update()`; no built-in watching.
- Stability (#64709): the client API stays backward compatible through 7.x; the client/server
  protocol is an implementation detail and may change in minor releases. Watch, incremental and
  build-orchestrator APIs are not stable in 7.1.
- Nightly break reported downstream: `api.updateSnapshot(base, params)` requires a base snapshot;
  `snapshot.getProject()` takes an id, config paths go through `getConfiguredProject()`.

Checker methods in the installed 7.1.0-beta `.d.ts` (`dist/api/async/api.d.ts`):
`getSymbolAtPosition(file, positions[])` and `getTypeAtPosition(file, positions[])` batch;
`getTypeOfSymbol(symbols[])` batches; `getContextualType(node)` is one node per call;
`isTypeAssignableTo(source, target)`; `getPropertiesOfType(type)`; `Type.getTypes()` (union members);
`getIndexType()`. `API.batch(...gens)` sends several generator calls in one round trip.

## 3. Memory: what we measured vs what the sources say

Measured (`bench/scripts/11_ts71_memory.mjs`, 7.1.0-beta, TS 5.9 `src/compiler`):

| | server RSS |
| --- | --- |
| `tsc --noEmit` full check | 322 MB |
| API: open snapshot | 154 MB |
| API: symbol at all 364577 identifiers | 844 MB |
| dispose, then a new snapshot doing the same pass | 881 -> 903 MB |

Blog 7.0 -> 7.1 beta `--noEmit` peak memory: vscode 8387 -> 6943 MiB, mui-docs 8093 -> 4079 MiB.
No source documents RSS behaviour of the API server after dispose; the measurement shows reuse,
not return to the OS.

## 4. Clients and alternative servers

| option | language | what it is | state |
| --- | --- | --- | --- |
| `typescript/sync` (official) | JS, node | the client the TS team ships and keeps compatible through 7.x | beta |
| `corsa_client` / corsa-bind | Rust (+ napi, C, Go, ...) | typed stdio client for the native compiler, msgpack; process isolation, restart, memory-kill controls | 0.x API, 1.15.0 (2026-10-18), 2.0.0-beta.1 (2026-10-21), MIT, 66 stars |
| tsrs `tsrs --api` + `tsrs/sync` | Rust server, official JS client copied | Rust port of TS 7 (oxc-project, Cloudflare/VoidZero), serves the same protocol at TS 7.1 beta `3e9236361fa9` | 0.20.1, Apache-2.0; 170/177 SDK methods answered; `getSymbolAtPosition`, `getTypeAtPosition`, `getContextualType`, `isTypeAssignableTo`, `getPropertiesOfType`, `getAliasedSymbol` marked `partial` |

tsrs memory notes (docs/NODE_API.md): snapshot release frees programs and checkers
(`createSnapshot` + release cycles flat, `tests/memory_test.rs`); claims ~70% less memory than
TS 7.0 at equal workers; independent run 72% less vs 7.0; vs 7.1 beta: 2.7x faster, no memory figure.
Critics question its arena model that "never frees" outside freeable regions.

## 5. Consequences for the plan

- Driver: node sidecar on `typescript/sync`. The protocol is not a stable surface; the JS client is.
  The same sidecar runs against tsrs by passing `tsserverPath`, so the server is swappable.
- Pin `typescript@7.1.0-beta` (later 7.1.0) in `ts7/package.json`; import from `typescript/sync`.
- Ask only demanded sites (unresolved fast-tier sites, keyof/literal occurrences), batched per file
  with `API.batch`; `getContextualType` costs one call per literal.
- Memory cap: restart the server past `max_rss_mb`; dispose does not lower RSS on tsgo.
- Not yet measured: the same experiment against `tsrs --api` (running the downloaded binary needs
  user approval in this environment).

## 6. Round-trip cost: the real limit of the API (other tools' numbers)

| source | finding |
| --- | --- |
| TypeScript #65051 (jsii over aws-cdk-lib, open) | 698k requests, 34.5 s wall, server checker work ~6 s; handler 1.6-5 µs per call vs 26 µs round trip. Proposed `getMembersOfType(type, filter)` and `getSymbolDocumentations(symbols[])` cut it to 304k requests / 29.4 s. Dependent requests cannot be batched. |
| TypeScript PR #64266 (Wes Wigham, back to draft 2026-10-28) | server-side "smart batch requests" for every method, client auto-grouping; not merged. |
| typescript-eslint PR #12803 (open, experimental `EXPERIMENTAL_backend: 'tsgo'`) | per-file bulk prefetch of `getTypeAtLocation` after ~10% of a file is queried singly; one contextual-type batch per file; 8192 files `recommendedTypeChecked`: 58.49 s / 3921 MB classic vs 100.63 s / 7349 MB native. Author attributes the gap to call count. |
| zinfer #200 | plans to leave ts-morph after 7.1 stabilises; per-lookup IPC flagged as the cost. |

For ryi: one request per demanded site is the cost driver, not the checker. Per file: one
`getSymbolAtPosition(file, positions[])`, one `getTypeAtPosition(file, positions[])`; contextual
types (literal sites) are one call each until a bulk method exists.

## 7. Memory reports elsewhere

| source | report |
| --- | --- |
| typescript-go #985 (closed) | LSP up to ~20 GB on a large monorepo, crashes above ~5 GB |
| typescript-go #2780 | pnpm monorepo, one edit -> ~35 GB |
| typescript-go #2910 (closed) | ~55 GB RAM/swap, macOS |
| typescript-go #3032 (closed, not planned) | `tsgo --lsp` OOM on large projects |
| tsrs #655 (fixed by #656, 2026-10-22) | tsrs `resolveModuleName` grew 9 -> 210 MB over 60k calls, `dispose()` freed nothing; the Go server stayed ~18 MB |

## 8. node_modules `.d.ts` for the fast tier: library already in the lock

`oxc_resolver` 11.24.3 (pinned in `hafley_scm/Cargo.toml:29`) has
`Resolver::resolve_dts(containing_file, specifier) -> Result<Resolution, ResolveError>`,
aiming at `ts.resolveModuleName` with `moduleResolution: "bundler"`:

- `exports` with the `types` condition always added (then `types`/`typings`/`main` are ignored)
- `types`/`typings`/`main` directory entry, `index` fallback
- `@types/<name>` fallback, scoped names mangled (`@babel/core` -> `@types/babel__core`)
- `.js` -> `.ts`/`.tsx`/`.d.ts` sibling substitution, `.mjs`/`.cjs` -> `.d.mts`/`.d.cts`
- tsconfig `paths`/`baseUrl`
- gap: `typesVersions` takes the first range regardless of version (oxc-resolver #1421;
  fix PR #1422 adds a `types_versions` option, merge state not confirmed)

Today `ts_resolve.rs` uses `Resolver::resolve` with `condition_names = ["source", "node", "import"]`,
`main_fields = ["module", "main"]`, skips `node_modules` in its walk, and gives corpus-external
bindings no row (`ts_resolve.rs:115-131`, `:61`, `:1134`). The option adds a second lookup through
`resolve_dts` for bare specifiers that resolve outside the corpus, and TSI over the `.d.ts` it returns.

## 9. Mapped-key remapping (RTK Query hooks)

- RTK Query hooks are named `use` + endpoint name + `Query`/`Mutation`/`InfiniteQuery`
  (`getPosts` -> `useGetPostsQuery`); endpoint names come from the OpenAPI `operationId`
  (`operationIdTransformer`).
- TypeScript #61094: go-to-definition on a key produced by a template-literal `as` clause does
  not reach the original property in VS Code (reported on 5.7.3). TypeScript #47813 (closed):
  with key remapping, definitions are lost but references remain.
- So `getSymbolAtPosition` on `useGetPostsQuery` is not expected to land on the `getPosts`
  endpoint declaration; the inside-out trace needs the mapped type's key type (template literal
  over `keyof`) and the literal `"getPosts"`, i.e. `getTypeAtPosition` + union/literal members,
  not symbol identity. Needs a fixture against 7.1 to confirm.

## 10. Measured: tsgo 7.1.0-beta vs tsrs 0.20.1-ts7.1.0-beta (same client script)

`bench/scripts/11_ts71_memory.mjs` with `tsserverPath` = tsrs binary, TS 5.9 `src/compiler`:

| phase | tsgo RSS | tsrs RSS | tsgo ms | tsrs ms |
| --- | --- | --- | --- | --- |
| snapshot open | 154 | 105 | 77 | 89 |
| symbols, 364577 identifiers | 844 | 625 | 3967 | 2628 |
| same symbols again | 881 | 599 | 3280 | 2241 |
| types, every identifier | 881 | 626 | 2652 | 1427 |
| dispose snapshot 1 | 881 | 504 | | |
| snapshot 2, same three passes | 903 | 739 | | |
| dispose snapshot 2 | 903 | 612 | | |
| plain `--noEmit` check, peak / wall | 322 MB / 0.51 s | 151 MB / 0.16 s | | |

tsrs returns memory on dispose (-122 MB, -127 MB) but each snapshot cycle still nets about
+108 MB (504 -> 612). tsgo holds its high-water mark and reuses it (+22 MB on cycle 2).

## Sources

- https://devblogs.microsoft.com/typescript/announcing-typescript-7-1-beta/
- https://github.com/microsoft/TypeScript/issues/63875 (API feature roadmap, closed 2026-10-09)
- https://github.com/microsoft/TypeScript/issues/64709 (API status for 7.1 beta)
- https://github.com/microsoft/typescript-go/issues/2824 (API usage patterns for editor extensions)
- https://github.com/microsoft/typescript-go/discussions/455
- https://github.com/tazo90/next-openapi-gen/pull/191 (nightly snapshot API break)
- https://dev.to/gabrielanhaia/typescript-7-has-no-stable-compiler-api-yet-thats-why-typescript-eslint-wont-install-4pa
- https://docs.rs/crate/corsa_client/latest, https://github.com/ubugeeei-prod/corsa-bind
- https://github.com/oxc-project/tsrs (docs/NODE_API.md, docs/CHECKER.md)
- https://www.infoq.com/news/2026/10/cloudflare-tsrs-ai-typescript/
- https://blog.authon.dev/tsrs-vs-tsgo-typescript-rust-port-vs-go
- https://github.com/microsoft/TypeScript/issues/65051, https://github.com/microsoft/TypeScript/pull/64266
- https://github.com/typescript-eslint/typescript-eslint/pull/12803, https://github.com/toiroakr/zinfer/issues/200
- https://github.com/microsoft/typescript-go/issues/985, /2780, /2910, /3032
- https://github.com/oxc-project/tsrs/issues/655
- https://docs.rs/crate/oxc_resolver/latest/source/src/dts_resolver.rs, https://github.com/oxc-project/oxc-resolver/issues/1421, https://github.com/oxc-project/oxc-resolver/pull/1422
- https://redux.js.org/toolkit/rtk-query/usage/code-generation, https://redux-toolkit.js.org/rtk-query/api/created-api/hooks
- https://github.com/microsoft/TypeScript/issues/61094, https://github.com/microsoft/TypeScript/issues/47813
