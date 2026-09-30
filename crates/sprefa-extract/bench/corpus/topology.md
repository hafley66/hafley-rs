# Topology derivations

Artifacts use Package URL identities from the CycloneDX root component (`metadata.component.purl`). PURL component grammar and qualifiers are defined in the Package URL specification: https://ecma-tc54.github.io/ECMA-427/multipage/purl-specification.html.

Package and dependency graph source: `sbom/<repo>.cdx.json`, emitted by `@cyclonedx/cyclonedx-npm` for npm packages and `cargo cyclonedx` or cdxgen for Cargo workspaces. `bomFormat`, `specVersion`, `metadata.component.purl`, nested `metadata.component.components`, top-level `components`, and `dependencies` retain their CycloneDX field names. CycloneDX JSON specification: https://cyclonedx.org/specification/overview/.

SCIP ownership is recorded as `scip/<repo>.ownership.tsv`. Each row comes from an SCIP definition occurrence: SCIP `Symbol` encodes a `Package` with `manager`, `name`, and `version`; the definition `Document.relative_path` is the owned file. Package and Document fields are specified in https://github.com/scip-code/scip/blob/main/scip.proto. npm repos were indexed with scip-typescript; Rust repos with rust-analyzer SCIP. The raw index records remain in `scip/*.index.scip`.

| Topology class | Derivation query | Evidence |
|---|---|---|
| Single package | Count root `metadata.component` plus workspace package components in the CycloneDX BOM; require count = 1. Cross-check SCIP definition occurrences have one distinct `Package(manager,name,version)`. | `corpus.tsv:2` Ajv; `corpus.tsv:7` Anyhow; their BOM files in `sbom/` |
| Monorepo with many packages | Count CycloneDX root plus nested workspace components; require count > 1. Cross-check distinct SCIP Package triples from ownership records. | `corpus.tsv:3` Codegraph; `corpus.tsv:4` Vite; `corpus.tsv:5` Tokio; `corpus.tsv:6` hafley-rs |
| TypeScript path aliases and project references | Parse `compilerOptions.paths` and `references` arrays from each tsconfig; select a repo where both predicates are true. | Vite `playground/resolve-tsconfig-paths/tsconfig.json:5` and `playground/resolve-tsconfig-paths/src/nested/tsconfig.json:3` |
| TypeScript barrels and re-export chains | Find `index.ts` SCIP Documents whose occurrences resolve only to re-export declarations; follow exported symbols to their destination Documents. | Codegraph `src/index.ts:67` and `src/index.ts:72`; query source is `ryii query --root repos/codegraph-src --lang typescript --query '(export_statement) @export' repos/codegraph-src/src` |
| Rust `pub use` facade | Query `use_declaration` nodes with public visibility; retain facade documents with re-export occurrences and follow symbols to definition documents. | Tokio `tokio/src/lib.rs:621` and `tokio/src/lib.rs:708`; query source is `ryii query --root repos/tokio --lang rust --query '(use_declaration) @use' repos/tokio/tokio/src` |
| Cross-package move | Join source and destination `Document.relative_path` rows to their SCIP Package triple and CycloneDX PURL. Require distinct package identities; only count rows where `cross_package=true`. | `targets.tsv` rows; ownership TSVs in `scip/`; BOM dependency graph in `sbom/` |

For optional file-level namespace/import edges, protobuf names remain `FileDescriptorProto.package`, `FileDescriptorProto.dependency`, and `FileDescriptorProto.public_dependency`, as defined in https://github.com/protocolbuffers/protobuf/blob/main/src/google/protobuf/descriptor.proto. No protobuf import-edge table was generated. Cross-language identity was not needed for the selected target set; Kythe's existing VName fields are `corpus`, `root`, `path`, `language`, and `signature`: https://kythe.io/docs/schema/.

`ryii` help was checked before queries. Declaration choices came from ryi query results and graph spot-checks. Example graph calls used `ryii graph --root repos/vite --callers createServer repos/vite/packages` and `ryii graph --root repos/ajv --callers validate repos/ajv/lib`. The client `ryi` daemon did not become ready in this environment, so direct query execution used its analyzer binary `ryii`.

## Barrel query outcome

The strict derived class query was evaluated over `index.ts` Documents in the three TS SCIP indexes: retain a document only when it has at least one occurrence, zero definition occurrences, zero `local ...` SCIP symbols, and every occurrence resolves to an imported package symbol. This returned zero documents in Ajv, Codegraph, and Vite. Codegraph `src/index.ts` has explicit re-export chains at `src/index.ts:67` and `src/index.ts:72`, but the document also contains locally declared exports later in the file, so it is not a qualifying all-re-exports document. The strict TypeScript barrel topology cell remains uncovered by the current corpus.
