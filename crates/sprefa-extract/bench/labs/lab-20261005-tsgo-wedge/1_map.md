# Need -> tsgo entry point (TypeScript v7.0.2, 1e4744d6, Go source under `tsc/`)

Paths are relative to `bench/repos/typescript-go/tsc/`; line numbers are the unpatched tag.

| need | checker / service entry point | LSP method | stock API method (`tsc --api --async`) | cost today |
| --- | --- | --- | --- | --- |
| definition of a call site / JSX attribute / type reference | `internal/ls/definition.go:17` ProvideDefinition (alias, overload and declaration-map mapping at `:196` getMappedLocation) | `textDocument/definition`, `internal/lsp/server.go:767` | no definition method; nearest is `getSymbolsAtPositions` (`internal/api/session.go:1155`) + `getAliasedSymbol` (`:2553`); declarations come back as node handles `index.kind.path` (`:99`), positions only after `getSourceFile` (`:1045`) + binary decode (`internal/api/encoder/encoder.go:22`); no `.d.ts` -> source mapping | LSP: one request per site; API: per file + per alias + per declaration file |
| resolved signature at a call | `internal/checker/exports.go:240` GetResolvedSignature | inside ProvideDefinition (`tryGetSignatureDeclaration`) | `getResolvedSignature` (`internal/api/session.go:1361`), node-handle input | one request per call node |
| references to a symbol | `internal/ls/findallreferences.go:694` ProvideReferences | `textDocument/references`, `internal/lsp/server.go:791` | `getReferencedSymbolsForNode` (`internal/api/session.go:3249`), `getReferencesToSymbolInFile` (`:3123`) | per symbol |
| symbol identity | `internal/checker/checker.go:31338` GetSymbolAtLocation, `:31924` GetAliasedSymbol | none | `getSymbolsAtPositions` returns `id` (ast.GetSymbolId) | batched per file |
| type identity / has_type | `internal/checker/checker.go:31912` GetTypeAtLocation | none | `getTypesAtPositions` (`internal/api/session.go:1458`) returns `id` | batched per file |
| assignable | `internal/checker/exports.go:325` IsTypeAssignableTo | none | `isTypeAssignableTo` (`internal/api/session.go:1959`) | one request per pair |
| subtype / strict subtype | `internal/checker/relater.go:154` isTypeSubtypeOf, `:158` isTypeStrictSubtypeOf (unexported) | none | none | unreachable |
| equivalent (identical) | `internal/checker/relater.go:118` isTypeIdenticalTo (unexported) | none | none | unreachable |
| comparable | `internal/checker/relater.go:162` isTypeComparableTo (unexported) | none | none | unreachable |
| conforms (declared heritage) | GetBaseTypes | none | `getBaseTypes` (`internal/api/session.go:2317`) | one request per type |
| type structure (Node script: properties, signatures, type args, constraint, typeToString) | checker exports | none | `getPropertiesOfType`, `getSignaturesOfType`, `getTypeArguments`, `getBaseConstraintOfType`, `typeToString`, `getReturnTypeOfSignature`, ... (`internal/api/proto.go:57-181`); `isReadonlySymbol` absent | one request per handle |
| project load without per-file rebuild | `internal/project/api.go:14` APIUpdate (one snapshot for many openFiles/openProjects) | `textDocument/didOpen` -> `internal/project/session.go:295` DidOpenFile runs one UpdateSnapshot per opened file; an unopened file is served from disk through `getSnapshot` (`:904`) | `updateSnapshot` (`internal/api/session.go:827`) with `openFiles` / `openProjects`, one request | LSP: one snapshot per didOpen |
| batch: many positions, one round trip | none | none | per-file arrays only (`getSymbolsAtPositions`, `getTypesAtPositions`) | |
| shared state between LSP and API | `internal/lsp/server.go:1732` handleInitializeAPISession: an API session over a pipe on the LSP's project session | `custom/initializeAPISession` (`:812`) | same API methods | one extra pipe |

Transport facts:

- `tsc --api` (`cmd/tsgo/main.go:24`) speaks MessagePack tuples by default; `--async` (`cmd/tsgo/api.go:22`)
  switches to JSON-RPC 2.0 with LSP Content-Length framing, the same framing ryi's LSP client reads.
- API positions are UTF-16 offsets (`positionMap.UTF16ToUTF8`); LSP positions are line/character.
- The API path for a file excluded by its package tsconfig (`5_Route.browser.test.ts`) is the inferred project
  `/dev/null/inferred`; project ids are lower-cased paths on this (case-insensitive) file system.
- The Node script (`hafley_scm/src/read/lang/ts_checker.mjs`) calls `ts.sys.fileExists` at line 91; on
  typescript@7.0.2 `ts.sys` is undefined (TypeError, reproduced).
