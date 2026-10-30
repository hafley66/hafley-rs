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
