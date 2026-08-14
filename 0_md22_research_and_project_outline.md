# md22 research and project outline

research date: 2026-08-13

## target

`md22` turns markdown into d2 source through small, composable extraction and rendering algorithms. the initial implementation targets deterministic structural facts. later source extensions can add authored conventions, repository relations, and semantic extraction without replacing the parser, graph model, or d2 emitter.

the local `projects/hafley-rxjs/packages/grapht` package already defines a related graph-document system. `md22` can begin as a source adapter and algorithm workbench for `grapht`, while keeping its pure transformations usable without the `grapht` process protocol.

## existing systems researched

| system | current state checked | relevant capability | boundary for md22 |
|---|---:|---|---|
| [d2](https://github.com/terrastruct/d2) | latest github release shown as `v0.7.1`; release page checked 2026-08-13 | text graph language, cli rendering, go library, svg/png/pdf/txt output, imports, variables, multiple layout engines | consumes generated d2; does not define how arbitrary markdown becomes graph topology |
| [comrak](https://github.com/kivikakk/comrak) | repository currently documents `0.54`, rust `1.85+`, commonmark `0.31.2` | rust commonmark/gfm ast, source positions, front matter, wikilinks, tables, task lists, footnotes, alerts | tree-oriented and allocation-heavier than event parsers; useful when parent/child structure and spans are required |
| [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark) | current repository checked 2026-08-13 | fast rust event parser with byte-offset iteration | requires md22 to build only the retained structural state it needs |
| [markdown-rs](https://github.com/wooorm/markdown-rs) and [mdast](https://github.com/syntax-tree/mdast) | current repositories checked 2026-08-13 | common syntax-tree vocabulary, commonmark/gfm/mdx parsing, json interchange | useful protocol vocabulary; mdast round trips do not preserve every concrete source detail |
| [markdown-rs-cli](https://docs.rs/crate/markdown-rs-cli/latest/source/README.md) | docs currently show `0.2.1` | extracts headings, sections, links, lists, and text to raw mdast json | close to the parse/extract half; no d2 projection or md22 algorithm protocol |
| [iwe](https://github.com/iwe-org/iwe) | repository checked 2026-08-13 | rust markdown knowledge graph, inclusion links, cross-references, backlinks, rename/refactor, cli/lsp/mcp, file watching | useful reference for repository-scale identity and incremental indexing; its published library is described as api-unstable |
| [okf](https://github.com/W4G1/okf) | search result reports cli/crate `0.2.1` | parses markdown plus yaml metadata and emits a mermaid knowledge graph | format-specific graph semantics rather than arbitrary projection algorithms |
| [py-d2](https://github.com/MrBlenny/py-d2) | repository checked 2026-08-13 | typed python builder for d2 source | evidence for a builder layer; does not solve markdown interpretation |
| [remark-d2](https://github.com/mech-a/remark-d2), [pandoc d2 filter](https://github.com/ram02z/d2-filter), and d2 editor integrations | listed by the d2 project | render d2 fences embedded inside markdown | direction is d2-in-markdown, while md22 derives d2 from markdown structure |

### result of the existing-tool search

existing tools cover four neighboring surfaces:

1. markdown parsing into events or ast nodes.
2. markdown repositories interpreted as link graphs.
3. typed or textual construction of d2 programs.
4. rendering d2 code fences found inside markdown.

the searched projects did not expose the same combined surface as `md22`: selectable markdown-to-graph algorithms, stable source identity, a common intermediate fact stream, and d2 emission as one replaceable sink.

## local grapht overlap

the following items are already stated in `projects/hafley-rxjs/packages/grapht/README.md`:

- capture d2, mermaid, and markdown from files, git revisions, fences, or cached output.
- retain immutable revisions, source spans, stable entity ids, topology, and provenance.
- keep source topology separate from human placement.
- normalize entities for svg, cytoscape, and xyflow projections.
- index markdown through headings, blocks, tables, lists, fences, links, authored ids, and byte spans.
- defer embeddings, semantic clustering, and inferred links until deterministic indexing exists.

`grapht` also has an implementation-agnostic json lines benchmark protocol. adapters receive one request on stdin, emit samples and one terminal result or error, and write hashed artifacts beneath a run directory. external wall time, cpu time, rss, output bytes, and diagnostics are recorded. this protocol can benchmark whole `md22` executables across typescript, rust, wasm, and worker implementations.

`grapht/src/7_identity.ts` is currently a benchmark identity adapter. its name concerns protocol pass-through rather than graph entity identity. md22 entity identity therefore needs an explicit contract instead of importing that file as an identity system.

## proposed boundary

```text
markdown bytes
  -> parser adapter
  -> markdown events
  -> fact extractors
  -> graph facts
  -> graph transforms
  -> d2 projection
  -> d2 text
  -> optional d2 render process
```

each arrow is a callable boundary. the streaming prefix supports fast paths. transforms that require global knowledge can collect the fact stream into an index.

## type signatures first

the names below describe contracts. exact rust ownership can be selected after measuring parser and allocation behavior.

```rust
type ByteOffset = u32;
type EntityId = u64;
type Symbol = u32;

struct SourceSpan {
    source: Symbol,
    start: ByteOffset,
    end: ByteOffset,
}

enum MarkdownEvent<'source> {
    Enter(BlockKind, SourceSpan),
    Exit(BlockKind, SourceSpan),
    Text(&'source str, SourceSpan),
    Link { destination: &'source str, span: SourceSpan },
    Code { language: Option<&'source str>, body: &'source str, span: SourceSpan },
}

enum GraphFact<'source> {
    Node(NodeFact<'source>),
    Edge(EdgeFact<'source>),
    Attribute(AttributeFact<'source>),
    Diagnostic(Diagnostic<'source>),
}

trait Extractor {
    fn push<'source>(
        &mut self,
        event: MarkdownEvent<'source>,
        emit: &mut dyn FnMut(GraphFact<'source>),
    );

    fn finish(&mut self, emit: &mut dyn FnMut(GraphFact<'static>));
}

trait Transform {
    fn apply(&mut self, graph: &mut GraphIndex);
}

trait Projection {
    type Output;
    fn project(&self, graph: &GraphIndex) -> Self::Output;
}

fn parse(source: &str, emit: impl FnMut(MarkdownEvent<'_>));
fn extract(events: impl Iterator<Item = MarkdownEvent<'_>>, algorithms: &mut [Box<dyn Extractor>]);
fn render_d2(graph: &GraphIndex, config: &D2Config) -> String;
```

an alternative static composition path avoids dynamic dispatch in measured hot loops:

```rust
fn run<E, P>(source: &str, extractor: E, projection: P) -> P::Output
where
    E: Extractor,
    P: Projection;
```

the cli can use dynamic registration while library callers use generic composition.

## instance timelines and lifetimes

### single-file batch

```text
source bytes alive
  parser alive
    borrowed markdown events
      extractor state
        owned graph index
          d2 string
```

borrowed text remains valid while the source buffer remains alive. facts promoted into the retained graph intern strings or store offsets into an owned source revision. temporary parser nodes do not enter durable state.

### watched file

```text
watch service lifetime
  artifact lifetime
    revision n source + index
    revision n+1 source + index
      identity reconciliation
      revision diff
      retained placement lookup
```

each parse creates a revision. publishing the next revision is a single state replacement. readers observe one complete revision. old revisions can remain addressable according to retention policy.

### repository

```text
workspace index lifetime
  file artifact lifetimes
    immutable revision lifetimes
  cross-file resolver lifetime
  output view lifetimes
```

file-local facts can be parsed in parallel. cross-file links resolve after the affected file facts are available. output views refer to entity ids and revision ids rather than parser allocations.

## storage, reads, writes, and uniqueness

### source storage

- one owned byte buffer per retained source revision.
- spans use byte offsets into that buffer.
- line and column values are computed through a newline index when requested.
- content hash identifies exact bytes and permits parse-cache lookup.

### graph storage

```rust
struct GraphIndex {
    nodes: Vec<Node>,
    edges: Vec<Edge>,
    node_by_id: HashMap<EntityId, NodeIndex>,
    edge_by_key: HashMap<EdgeKey, EdgeIndex>,
    outgoing: Vec<Vec<EdgeIndex>>,
    incoming: Vec<Vec<EdgeIndex>>,
    strings: StringInterner,
}
```

the vectors support traversal and serialization. the maps support identity lookup and deduplication. adjacency is materialized only when an algorithm or projection asks for graph navigation. an initial implementation can keep edges in one vector and add adjacency after benchmark evidence.

### identity sequence

1. prefer an authored id when the markdown extension supplies one.
2. otherwise construct a structural key from source identity, node kind, ancestor identity, and a normalized local discriminator.
3. hash the canonical key into the runtime entity id.
4. retain the full canonical key for collision checking and diagnostics.
5. reconcile moved or renamed nodes across revisions through an optional secondary matcher. this matcher emits identity proposals rather than silently rewriting ids.

heading text alone cannot be unique because headings may repeat. byte offsets alone cannot be stable because prior edits move later content. the canonical key needs both structural context and an explicit duplicate ordinal until authored ids exist.

### read and write sequence

```text
read source bytes
read parser events
write file-local facts
read file-local facts
write resolved graph index
read graph index
write d2 text
optional: read d2 text and write rendered artifact
```

extractors do not mutate source text. projections do not mutate graph identity. layout and placement write through separate stores, matching the existing `grapht` boundary.

## initial algorithms

| algorithm | nodes | edges or containment | state needed |
|---|---|---|---|
| `headings` | each heading | nearest lower-level ancestor contains heading | heading stack |
| `sections` | heading-delimited section | section belongs to heading | active heading |
| `lists` | each list item | nesting and sequence | list stack |
| `links` | local anchors, files, urls | source block references destination | destination resolver |
| `tasks` | task items | containment; later authored dependency syntax | list stack |
| `fences` | fenced blocks | block belongs to section | active heading |
| `tables` | table and optionally rows | row belongs to table | table state |
| `frontmatter` | declared entities | declared typed relations | front matter decoder |
| `d2-fences` | d2 block as source entity | block belongs to section | active heading |

algorithms emit facts. a preset selects algorithms:

```text
md22 read.md | md22 extract headings,links | md22 d2
md22 read.md | md22 preset outline | md22 d2
md22 read.md | md22 preset knowledge | grapht-adapter-render-cytoscape
```

the direct command remains:

```text
md22 input.md --preset outline --output output.d2
```

## source extension protocol

extensions need access to structural events and a way to emit facts. they should not receive the d2 builder unless their declared output is specifically d2 syntax.

```rust
struct AlgorithmDescriptor {
    name: &'static str,
    protocol_version: u16,
    input: InputKind,
    output: OutputKind,
    needs: CapabilitySet,
}
```

candidate capability flags:

```text
streaming
full-document
cross-file-index
frontmatter
source-text
resolved-links
mutable-graph
```

this makes buffering visible. a pipeline planner can keep streaming algorithms on the event path and schedule whole-graph transforms after collection.

## pure and process layers

```text
lib/0_source
lib/1_markdown_events
lib/2_graph_facts
lib/3_identity
lib/4_extractors
lib/5_graph_index
lib/6_transforms
lib/7_d2_projection
lib/8_pipeline
cli
```

lower numbered modules contain data and pure functions. parser libraries, file reads, process spawning, watching, and the `grapht` jsonl adapter remain at the boundary.

the physical filenames should follow the repository's author-driven numeric ordering after the implementation language and crate/package boundary are selected.

## high-speed path

### measurements

measure these phases separately:

```text
read
parse
extract
resolve
index
project-d2
d2-compile
d2-layout
d2-render
```

record wall time, cpu time, peak rss, input bytes, event count, node count, edge count, interned bytes, output bytes, and content hash. the existing `grapht-bench/0` envelope already records process measurements and artifacts. md22 samples can add phase counters.

### parser comparison fixture

implement the same `headings + links` extractor against:

- `pulldown-cmark` offset events.
- `comrak` ast traversal with source positions.

fixtures should include 1 kb, 100 kb, 1 mb, and repository-scale markdown; repeated headings; deep lists; gfm tables; unicode; links; malformed fences; and large code blocks. compare parse plus extraction, retained allocations, source-position precision, and extension coverage.

### incremental work

the first cache key is `(source_hash, parser_config_hash, algorithm_set_hash)`. repository resolution has a second key containing the relevant target-index revision. unchanged files retain their file-local facts. changed files invalidate their outgoing relations and any unresolved inbound target lookup affected by the path or anchor change.

### copies and allocations

- borrow source slices during parse and extraction.
- intern retained labels, kinds, paths, and relation names.
- retain offsets instead of copied source substrings where possible.
- serialize d2 into a pre-sized byte buffer.
- keep d2 compilation outside extraction benchmarks.
- permit process adapters for implementation comparison while retaining an in-process library path for production throughput.

## rust and wasm revision pipeline

yes. the graph can retain a typed relation from arbitrary agent text at message time `t - 1` to markdown entities first observed or changed at document time `t`, version `v`. the relation should carry its evidence, algorithm, score, and revision coordinates.

```text
agent message m(t-1)
  -> parse or chunk message
  -> message claims and spans

markdown document d(t, v-1)
markdown document d(t, v)
  -> byte or token diff
  -> changed source ranges
  -> structural parse
  -> entity reconciliation
  -> inserted, deleted, moved, renamed, and edited entities

message claims x changed entities
  -> candidate generation
  -> feature scoring
  -> temporal links with evidence
```

### revision types

```rust
type DocumentId = u64;
type RevisionId = u64;
type MessageId = u64;
type RelationId = u64;

struct DocumentRevision {
    document_id: DocumentId,
    revision_id: RevisionId,
    parent_revision_id: Option<RevisionId>,
    logical_time: u64,
    content_hash: [u8; 32],
    source: SourceBlobId,
}

struct MessageRevision {
    message_id: MessageId,
    logical_time: u64,
    content_hash: [u8; 32],
    source: SourceBlobId,
}

enum EntityChange {
    Insert { after: EntityId },
    Delete { before: EntityId },
    Retain { before: EntityId, after: EntityId },
    Move { before: EntityId, after: EntityId },
    Rename { before: EntityId, after: EntityId },
    Edit { before: EntityId, after: EntityId, changed: Vec<SourceSpan> },
}

struct TemporalLink {
    relation_id: RelationId,
    from: TemporalEntityRef,
    to: TemporalEntityRef,
    kind: RelationKind,
    algorithm: AlgorithmRef,
    score: f32,
    evidence: Vec<Evidence>,
    status: LinkStatus,
}

struct TemporalEntityRef {
    source_id: u64,
    revision_id: u64,
    entity_id: EntityId,
    span: SourceSpan,
}

enum LinkStatus {
    Candidate,
    Accepted,
    Rejected,
    Authored,
}
```

the graph edge addresses both revisions. a link to `document/entity` without a revision loses which wording and span produced the match.

### diff and parse sequence

```rust
fn diff_text(before: &str, after: &str) -> Vec<TextEdit>;

fn update_parse(
    previous: Option<&ParseState>,
    edits: &[TextEdit],
    after: &str,
) -> ParseState;

fn reconcile_entities(
    before: &EntityIndex,
    after: &EntityIndex,
    edits: &[TextEdit],
) -> Vec<EntityChange>;

fn propose_links(
    message: &MessageIndex,
    changes: &[EntityChange],
    documents: &RevisionStore,
) -> Vec<TemporalLink>;
```

the first implementation can reparse a whole file after using the diff to restrict reconciliation and candidate matching. this preserves parser correctness while making the semantic work proportional to changed entities. incremental parsing can be introduced as a measured replacement for `update_parse`.

### parser paths under wasm

| path | incremental parse | markdown correctness surface | wasm condition |
|---|---|---|---|
| `pulldown-cmark` full reparse | no retained parse tree | commonmark event stream with offsets | rust-only implementation and no required process boundary make it compatible with `wasm32-unknown-unknown` subject to selected crate features |
| `comrak` full reparse | no incremental api used here | commonmark/gfm ast and extensions with source positions | dependency feature audit and wasm build receipt required |
| tree-sitter markdown | edits old tree and reparses with structural sharing | grammar repository documents markdown inaccuracies and targets syntax information | grammar repository says web-tree-sitter does not work out of the box because required c functions are not exported; static linking is its documented workaround |

tree-sitter's incremental api accepts byte and row/column coordinates for the old and new edit bounds, adjusts the old tree, and reparses with the old tree so unchanged structure can be shared. [tree-sitter editing](https://tree-sitter.github.io/tree-sitter/using-parsers/3-advanced-parsing.html)

the current tree-sitter markdown repository release is `v0.5.3`, dated 2026-02-26. its readme explicitly warns against correctness-sensitive use and documents the wasm linking complication. [tree-sitter markdown](https://github.com/tree-sitter-grammars/tree-sitter-markdown)

### text diff

`imara-diff 0.2.0` supplies myers and histogram algorithms over arbitrary token sequences. it also supports reusing interned inputs when one file is compared against multiple versions. its docs report histogram as the faster algorithm across its benchmark corpus. [imara-diff](https://docs.rs/imara-diff/latest/imara_diff/)

for markdown revisions, run two diff granularities from the same api:

```text
line tokens
  -> locate changed line regions cheaply

word or syntax tokens inside changed regions
  -> evidence spans for edited entities
```

diff output describes changed bytes. entity reconciliation determines which structural entity owns each changed range.

### identity reconciliation across versions

use ordered evidence instead of one identity heuristic:

| evidence | example |
|---|---|
| authored id | `{#parser}` or extension metadata |
| exact retained span mapping | old entity lies outside edits and maps directly into the new source |
| exact canonical content hash | paragraph moved without text changes |
| structural path | heading ancestry and sibling kind |
| local neighborhood | preceding and following retained entities |
| normalized text similarity | heading or paragraph edited |
| duplicate ordinal | third equal heading beneath the same parent |

the reconciler emits `retain`, `move`, `rename`, `edit`, `insert`, or `delete`. it also records the evidence used for the classification. a later reconciler can be replayed against stored revisions because the raw revisions and prior identities remain present.

### mapping the previous agent message to changed markdown

candidate generation can use the temporal boundary first:

```text
candidates(message_(t-1), document_(t,v)) =
  entities inserted or edited since document_(t,v-1)
  union explicitly named unchanged entities
  union destinations of links added by the change
```

this bounds the comparison set before fuzzy or semantic work.

features for each `(message span, markdown entity revision)` pair:

```rust
struct LinkFeatures {
    exact_quote: bool,
    normalized_quote: bool,
    identifier_overlap: f32,
    token_overlap: f32,
    heading_path_overlap: f32,
    explicit_path_match: bool,
    explicit_anchor_match: bool,
    changed_token_coverage: f32,
    temporal_distance: u32,
    embedding_similarity: Option<f32>,
}
```

relation kinds can state what the evidence claims:

```text
mentions
requests
causes-change
supplies-text
renames
moves
contradicts
elaborates
```

an exact copied phrase can emit `supplies-text`. a message naming a path and heading can emit `mentions` or `requests`. `causes-change` remains a scored inference unless the capture system knows that the agent action produced the document write.

### confidence and ambiguity

store the feature vector and score rather than only a boolean edge:

```rust
struct Evidence {
    message_span: SourceSpan,
    document_span: SourceSpan,
    feature: EvidenceKind,
    weight: f32,
    excerpt_hash: [u8; 32],
}
```

one message may map to multiple markdown entities. one entity may derive from multiple messages. the model therefore has many-to-many temporal links. accepted and rejected candidate edges become labeled fixtures for later scoring algorithms.

### arbitrary text source adapter

markdown-specific parsing belongs only on the document side. the agent side can start with a generic chunker:

```rust
trait TextIndexer {
    fn index(&self, source: &SourceRevision, emit: &mut dyn FnMut(TextFact));
}

enum TextFact {
    Block(SourceSpan),
    Sentence(SourceSpan),
    Identifier { value: Symbol, span: SourceSpan },
    Path { value: Symbol, span: SourceSpan },
    Quote(SourceSpan),
    ExplicitRelation { from: SourceSpan, kind: Symbol, to: SourceSpan },
}
```

agent-message markdown can use the markdown adapter. raw model events, tool outputs, diffs, json, and terminal logs can provide their own `TextIndexer`. all adapters terminate in source-addressed facts.

### wasm boundary

the browser-facing core can expose immutable byte-oriented calls:

```rust
#[wasm_bindgen]
pub fn parse_markdown(source: &[u8], config: &[u8]) -> Vec<u8>;

#[wasm_bindgen]
pub fn update_markdown(previous: &[u8], source: &[u8], edits: &[u8]) -> Vec<u8>;

#[wasm_bindgen]
pub fn link_message(message: &[u8], revision_delta: &[u8], config: &[u8]) -> Vec<u8>;

#[wasm_bindgen]
pub fn project_d2(graph_delta: &[u8], config: &[u8]) -> Vec<u8>;
```

the byte payload can begin as versioned json for inspection. a binary encoding can replace it after serialization appears in measurements. parser state can also live behind a wasm handle for repeated edits, but persisted revision and relation records remain handle-free.

### revision pipeline modules

```text
lib/0_protocol
lib/1_source
lib/2_text_edit
lib/3_markdown_events
lib/4_graph_facts
lib/5_identity
lib/6_revision
lib/7_reconcile
lib/8_candidate
lib/9_score
lib/10_d2_projection
lib/11_pipeline
wasm
cli
```

dependency order places diff and source identity below markdown extraction. candidate generation depends on reconciled revision changes. scoring depends on candidates. d2 projection reads the resulting temporal graph.

### revision fixtures

each fixture needs a timeline rather than one input file:

```text
fixtures/0_heading_edit/
  0_message.md
  1_before.md
  2_after.md
  3_expected.yaml

fixtures/1_paragraph_move/
fixtures/2_duplicate_heading/
fixtures/3_agent_quote_copy/
fixtures/4_agent_paraphrase/
fixtures/5_unrelated_concurrent_edit/
fixtures/6_one_message_many_sections/
fixtures/7_many_messages_one_section/
```

the expected receipt contains text edits, entity changes, candidate edges, feature vectors, accepted edges, rejected edges, and d2 projection.

## d2 projection details

the d2 sink needs four mechanical operations:

1. quote labels and identifiers correctly.
2. map containment to nested maps or explicit edges according to projection configuration.
3. map node and edge kinds to d2 classes.
4. emit deterministic order for snapshots and content hashing.

d2 imports and variables can hold shared styles. generated topology can remain separate from authored style:

```d2
...@styles

document: {
  parser: Parser
  renderer: Renderer
  parser -> renderer: feeds
}
```

the d2 project documents imports as relative to the importing file and restricts imported files to the `.d2` extension. configuration variables can select layout engine, theme, padding, center, and sketch mode. [d2 imports](https://www.d2lang.com/tour/imports/) [d2 variables](https://d2lang.com/tour/vars/)

## delivery order

0. collect paired markdown and expected d2 fixtures.
1. define spans, events, facts, graph ids, diagnostics, and projection output types.
2. implement one parser adapter and dump its event stream for inspection.
3. implement `headings` and `links` extractors.
4. implement deterministic graph indexing and d2 projection.
5. snapshot complete event, fact, graph, and d2 outputs from shared fixtures.
6. wrap the pipeline in `md22 input.md --preset outline`.
7. add a `grapht-bench/0` adapter and phase samples.
8. implement the second parser adapter and compare measurements.
9. add watch mode with revision hashes and file-local invalidation.
10. add lists, tasks, fences, tables, and front matter as independent extractors.
11. expose an external algorithm protocol after two in-process algorithms demonstrate the required data surface.

## test record shape

one fixture should drive a single maximal snapshot:

```rust
#[test]
fn outline_fixture() {
    let receipt = run_fixture(include_str!("fixtures/0_outline.md"));
    insta::assert_yaml_snapshot!(receipt);
}
```

the receipt contains parser configuration, ordered events, facts, canonical identity keys, graph nodes, graph edges, diagnostics, and emitted d2. volatile timings stay in benchmark receipts instead of correctness snapshots.

## open decisions requiring fixtures or measurements

| decision | evidence required |
|---|---|
| `pulldown-cmark` or `comrak` first | identical extractor benchmark plus required extension list |
| nested d2 maps or flat ids | expected diagrams containing repeated headings and cross-container links |
| entity id key | edits that rename, move, duplicate, and reorder sections |
| event protocol ownership | one rust implementation and one typescript or process adapter |
| shared model with `grapht` | compare md22 fact requirements against the upcoming grapht markdown index types |
| external extension loading | two algorithms developed outside the core package |

## source inventory

- [d2 repository](https://github.com/terrastruct/d2)
- [d2 releases](https://github.com/terrastruct/d2/releases)
- [d2 documentation source](https://github.com/terrastruct/d2-docs)
- [d2 imports](https://www.d2lang.com/tour/imports/)
- [d2 variables and configuration](https://d2lang.com/tour/vars/)
- [comrak](https://github.com/kivikakk/comrak)
- [pulldown-cmark](https://github.com/raphlinus/pulldown-cmark)
- [markdown-rs](https://github.com/wooorm/markdown-rs)
- [mdast](https://github.com/syntax-tree/mdast)
- [markdown-rs-cli](https://docs.rs/crate/markdown-rs-cli/latest/source/README.md)
- [iwe](https://github.com/iwe-org/iwe)
- [okf rust implementation](https://github.com/W4G1/okf)
- [py-d2](https://github.com/MrBlenny/py-d2)
- [d2 official integration inventory](https://github.com/terrastruct/d2#related)
- `projects/hafley-rxjs/packages/grapht/README.md`
- `projects/hafley-rxjs/packages/grapht/PROTOCOL.md`
- `projects/hafley-rxjs/packages/grapht/src/0_benchProtocol.ts`
- `projects/hafley-rxjs/packages/grapht/src/4_process.ts`
- `projects/hafley-rxjs/packages/grapht/src/7_identity.ts`

## research gaps

- current github release metadata for `pulldown-cmark`, `markdown-rs`, and `iwe` was not confirmed from a release endpoint during this pass, so no latest-version claim is recorded for them.
- d2 exposes its compiler as a go library and through `d2.js`; rust-facing integration options need a focused api and process-overhead benchmark.
- the `grapht` markdown index is stated in its plan but corresponding implementation types were not present in the inspected source list.
