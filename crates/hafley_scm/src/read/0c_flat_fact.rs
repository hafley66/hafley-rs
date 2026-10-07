use super::*;

/// One flat fact. The `record` tag discriminates the shape; `family` carries the
/// plane. Serialized as JSONL (`{"record":"node",...}` etc.).
/// A foreign producer spelling this wire is only usable if the rows decode
/// back, so `Deserialize` is as much of the contract as `Serialize`.
#[derive(Serialize, serde::Deserialize, Debug)]
#[serde(tag = "record", rename_all = "lowercase")]
pub enum FlatFact {
    /// The stream's own version. First row of every witnessed stream.
    Protocol {
        version: u32,
    },
    Run(crate::read::tsi::types::RunOut),
    Fact(crate::read::tsi::types::FactOut),
    Witness(crate::read::tsi::types::WitnessOut),
    Coverage(crate::read::tsi::types::CoverageOut),
    Diagnostic(crate::read::tsi::types::DiagnosticOut),
    Node {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        kind: String,
        name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        function: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        named: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        is_async: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        owner_kind: Option<String>,
    },
    /// `from_kind`/`to_kind` spell the endpoints' node kinds, so a consumer
    /// keyed on the wire alone carries the whole `(span, kind)` node identity.
    Edge {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        kind: String,
        from: SpanOut,
        #[serde(skip_serializing_if = "Option::is_none")]
        from_kind: Option<String>,
        to: SpanOut,
        #[serde(skip_serializing_if = "Option::is_none")]
        to_kind: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        field: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        index: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        named_index: Option<u32>,
    },
    /// DfF parameter slot bridge: one parameter node and its typed-parameter
    /// position. The receiver/self is omitted from the position count.
    #[serde(rename = "param")]
    DfParam {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        pos: u32,
    },
    /// DfF argument slot bridge: one call/new node, signed slot, and argument
    /// node. Method receivers use slot `-1`.
    #[serde(rename = "arg")]
    DfArg {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        call: SpanOut,
        pos: i64,
        arg: SpanOut,
    },
    /// DfF named value-into-composite bridge: composite node, field/property/
    /// named-argument name, value node. The pseudo field `..` is a spread /
    /// functional-update base.
    #[serde(rename = "df_field")]
    DfField {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        name: String,
        value: SpanOut,
    },
    /// DfF string-carrying value node: node, kind lit|template|concat, text
    /// (cooked literal or raw source slice).
    #[serde(rename = "df_lit")]
    DfLit {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        node: SpanOut,
        kind: String,
        text: String,
    },
    /// DfF loop: the loop's own span, its variable, and the iterated collection
    /// as written. `var`/`collection` are null where the form names neither.
    #[serde(rename = "df_loop")]
    DfLoop {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        var: Option<String>,
        collection: Option<String>,
    },
    /// DfF loop nest: one call/new node, an enclosing loop, and that loop's rank
    /// in the nest (1 = outermost).
    #[serde(rename = "df_nest")]
    DfNest {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        call: SpanOut,
        #[serde(rename = "loop")]
        loop_span: SpanOut,
        depth: u32,
        collection: Option<String>,
    },
    /// DfF allocating callable: the fn/method/closure whose body builds a
    /// collection. Rust only.
    #[serde(rename = "df_allocates")]
    DfAllocates {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
    },
    /// TypeF arrow-type sig: owner = callable span, slot = param/ret, pos, ty.
    Sig {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        owner_start: u32,
        owner_end: u32,
        slot: String,
        pos: u32,
        ty: String,
    },
    /// CallF call site (phase-1 unresolved): span, callee as written, optional path.
    Site {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        callee: String,
        callee_path: Option<String>,
    },
    /// TypeF const value: owner, optional field path, text, kind = lit|template.
    Const {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        field: Option<String>,
        text: String,
        kind: String,
    },
    /// TypeF doc block: the owning entity's span, its impl owner when it has
    /// one, and the cleaned text.
    Doc {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        parent: Option<String>,
        text: String,
    },
    /// TypeF doc tag: one structured tag off the block at `owner`.
    #[serde(rename = "doc_tag")]
    DocTagOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        tag: String,
        arg: Option<String>,
        text: String,
    },
    /// DataF document row: one json/jsonl/yaml/toml document of the file.
    /// `doc` is the whole document as a json VALUE, the column `decode/2` reads.
    #[serde(rename = "data_doc")]
    DataDocOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        ordinal: u32,
        span: SpanOut,
        format: String,
        doc: serde_json::Value,
    },
    /// DataF value row: one value inside a document, addressed by its dotted
    /// path. `text` is null for objects and arrays.
    #[serde(rename = "data_value")]
    DataValueOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        ordinal: u32,
        path: String,
        kind: String,
        text: Option<String>,
        span: SpanOut,
    },
    /// TypeF doc structure row: heading, code block, link or image. `target`
    /// and `title` ride link and image rows, `body` a code_block with content.
    #[serde(rename = "doc_node")]
    DocNodeOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        kind: String,
        name: String,
        parent: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        target: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        body: Option<SpanOut>,
    },
    /// CallF module specifier (phase-1, as written): span, bound name, kind.
    /// v6-ONLY rows (no v5 oracle facet) — the parity golden reports them,
    /// never asserts them.
    Specifier {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        name: String,
        kind: String,
        /// The source module as written, null when the language puts the
        /// module in `name` (path-only forms).
        module: Option<String>,
        /// The source module's own name for the binding when it differs from
        /// `name`; null when they agree. v5's `module_binding` imported seat.
        imported: Option<String>,
        #[serde(default, skip_serializing_if = "is_false")]
        type_only: bool,
    },
    /// CallF method owner: the declaration a `method` def node belongs to,
    /// joined to it by `owner`. v6-ONLY, no v5 oracle facet.
    #[serde(rename = "method_owner")]
    MethodOwnerOut {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        owner: SpanOut,
        self_type: Option<String>,
        #[serde(rename = "trait")]
        trait_name: Option<String>,
    },
    /// CallF macro site: the invocation `span` whose expansion minted a
    /// def/site elsewhere in this file, and which arm found it.
    #[serde(rename = "macro_site")]
    MacroSiteOut {
        family: FamilyTag,
        span: SpanOut,
        macro_name: String,
        source: String,
    },
    /// A Prolog term-occurrence reference: a compound in argument position,
    /// tagged goal | head_arg | term_arg. Deliberately exceeds the LSP/SCIP
    /// reference set (a data term in argument position is a reference nowhere
    /// else emits one).
    Reference {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        family: FamilyTag,
        span: SpanOut,
        /// The interned `functor/arity` key, e.g. `relplan/5`.
        functor: String,
        /// goal | head_arg | term_arg
        position: String,
    },
    /// CallF runtime-computed edge marker: `detail` is the source text at
    /// `span`. v6-ONLY, no v5 oracle facet.
    #[serde(rename = "unresolved")]
    Unresolved {
        family: FamilyTag,
        /// The file the site sits in. ABSENT from a per-file run, where the
        /// caller already knows it; a resolve run spans files and must say.
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<String>,
        span: SpanOut,
        /// dynamic-import | computed-member-call | spread-call-args |
        /// no_corpus_def | ambiguous
        reason: String,
        detail: String,
    },
    /// A project-phase (cross-file) resolved edge: `to` lives in ANOTHER blob,
    /// content-keyed by `to_blob` (hex). The 4a wire ruling: ONE arm carries
    /// TypeEdgeKind + CallEdgeKind as strings (never per-family arms, never a
    /// side channel). Emitted only when a `Resolve<F>` result is flattened —
    /// `flatten_jsonl` (the CLI stream) stays phase-1 and never produces these.
    ProjectEdge {
        family: FamilyTag,
        kind: String,
        from: SpanOut,
        to_blob: String,
        to: SpanOut,
    },
    /// One cross-function value-flow edge (FlowF), BOTH endpoints content-keyed
    /// because flow crosses files. Flattened by `flatten_flow`.
    #[serde(rename = "flow_edge")]
    FlowEdgeOut {
        family: FamilyTag,
        kind: String,
        from_blob: String,
        from: SpanOut,
        to_blob: String,
        to: SpanOut,
    },
    /// A project-mode CLI call edge. Paths and names are top-level fields so
    /// line-oriented consumers can decode the record without span joins.
    #[serde(rename = "resolved_edge")]
    ResolvedEdge {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        caller_path: String,
        caller_name: Option<String>,
        callee_path: String,
        callee_name: Option<String>,
        caller_site_start: u32,
        caller_site_end: u32,
        callee_start: u32,
        callee_end: u32,
        kind: String,
        /// Which resolver leg answered (`ResolutionOrigin::as_str`).
        resolution_origin: String,
    },
    #[serde(rename = "graph_node")]
    GraphNode {
        path: String,
        name: Option<String>,
        depth: u32,
        grade: String,
        line: Option<u32>,
    },
    #[serde(rename = "seed_unmatched")]
    SeedUnmatched {
        seed: String,
        reason: String,
    },
    #[serde(rename = "graph_edge")]
    GraphEdge {
        from_path: String,
        from_name: Option<String>,
        to_path: String,
        to_name: Option<String>,
        kind: String,
        grade: String,
        /// 1-based line of the edge's source site, when its file reads.
        from_line: Option<u32>,
        /// 1-based line of the edge's target declaration, when a span exists.
        to_line: Option<u32>,
    },
    #[serde(rename = "external_crate_decline")]
    ExternalCrateDecline {
        from_path: String,
        from_name: Option<String>,
        type_name: String,
        crate_name: String,
        reason: String,
        kind: String,
    },
    #[serde(rename = "graph_decline")]
    GraphDecline {
        from_path: String,
        from_name: Option<String>,
        type_name: String,
        crate_name: String,
        reason: String,
        kind: String,
    },
    #[serde(rename = "graph_path")]
    GraphPath {
        plane: String,
        from_path: String,
        from_name: Option<String>,
        to_path: String,
        to_name: Option<String>,
        depth: u32,
        /// Export row ids of the ordered edges that witness this path.
        witness: Vec<u64>,
        /// Worktree-relative source path mapped from the endpoint digest.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        from_file: Option<String>,
        /// 1-based line and byte column at the endpoint span start.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        from_line: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        from_col: Option<u32>,
        /// Exact source bytes of the endpoint span, decoded as UTF-8.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        from_text: Option<String>,
        /// Why endpoint location could not be supplied.
        #[serde(skip_serializing_if = "Option::is_none", default)]
        from_reason: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        to_file: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        to_line: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        to_col: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        to_text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none", default)]
        to_reason: Option<String>,
    },
    #[serde(rename = "graph_path_change")]
    GraphPathChange {
        change: String,
        revision: String,
        plane: String,
        from_path: String,
        from_name: Option<String>,
        to_path: String,
        to_name: Option<String>,
        depth: u32,
    },
    #[serde(rename = "graph_root")]
    GraphRoot {
        path: String,
        name: Option<String>,
        #[serde(deserialize_with = "deserialize_graph_root_span")]
        span: Option<SpanOut>,
        found: bool,
    },
    /// A project-mode `Resolve<TypeF>` edge: one type reference resolved to the
    /// declaration it names. The flat twin of `ProjectEdge`, for the same reason
    /// as `ResolvedEdge` above: the v6 host decodes top-level keys, so the
    /// target coordinate travels as a path plus a name, never a nested span
    /// join. `owner` is the referencing declaration, `target` what it resolved
    /// to.
    #[serde(rename = "resolved_type_edge")]
    ResolvedTypeEdge {
        #[serde(skip_serializing_if = "Option::is_none", default)]
        fact: Option<u32>,
        owner_path: String,
        owner_name: Option<String>,
        owner_start: u32,
        owner_end: u32,
        target_path: String,
        target_name: Option<String>,
        kind: String,
        /// Which resolver leg answered (`ResolutionOrigin::as_str`).
        resolution_origin: String,
    },
    // ── the `scip` family: v5's scip_* relation shapes ──────────────────────
    //
    // These eight rows ARE v5's `scip_*` relations (repo-root src/rels/scip.rs
    // decls), projected here rather than left as joins over the passthrough
    // rows. The passthrough rows above and these are two answers to different
    // questions and both ship: `ryi scip --raw` is every field the protobuf
    // carries, unjoined; `--family scip` is the v5 relation vocabulary a
    // program already knows how to read.
    //
    // v5's `scip_occurrence` and `scip_binding` are NOT among them, and the
    // reason is a wire collision, not a gap in the port: `scip_occurrence` is
    // ALREADY a record tag on this wire (the byte-span passthrough row above),
    // with different fields. Two shapes under one tag is the silent-drift
    // hazard every golden here exists to stop. Both v5 rows are one consumer
    // join off `ryi scip --raw --records scip_occurrence`, which carries the
    // spans and every role bit; `scip_binding`'s source-slice need is answered
    // by that row's optional `text` field under --occurrence-text (issue
    // extract-scip-vocab-occurrence-binding).
    /// v5 `scip_def(symbol, file, repo)`: a symbol's defining document.
    #[serde(rename = "scip_def")]
    ScipDefRow {
        symbol: String,
        file: String,
        repo: String,
    },
    /// v5 `scip_name(symbol, name)`: the trailing identifier run of a moniker.
    /// Computed here because it needs the moniker grammar's `[`/`]`/`#`
    /// separators, which a single-separator string split cannot all honor.
    #[serde(rename = "scip_name")]
    ScipNameRow {
        symbol: String,
        name: String,
    },
    /// v5 `scip_ref(file, symbol, def_file, repo)`: a non-definition occurrence
    /// of a symbol this index also defines.
    #[serde(rename = "scip_ref")]
    ScipRefRow {
        file: String,
        symbol: String,
        def_file: String,
        repo: String,
    },
    /// A reference to a symbol declared external by the SCIP index. `symbol`
    /// is the complete target identity; `origin` is its SCIP scheme, package
    /// manager, package, and version prefix.
    #[serde(rename = "scip_external_ref")]
    ScipExternalRefRow {
        file: String,
        symbol: String,
        origin: String,
        repo: String,
    },
    /// v5 `scip_edge(src, dst, repo)`: file-to-file dependency, one row per
    /// distinct pair. The same graph `--scip-deps` folds, in v5's column names.
    #[serde(rename = "scip_edge")]
    ScipEdgeRow {
        src: String,
        dst: String,
        repo: String,
    },
    /// v5 `scip_fn_edge(caller, callee)`: the function-level call graph, the
    /// caller being the innermost enclosing callable definition.
    #[serde(rename = "scip_fn_edge")]
    ScipFnEdgeRow {
        caller: String,
        callee: String,
    },
    /// v5 `scip_callee_type(sym, type)`: the receiver type parsed out of a
    /// method moniker's `impl#[T]` / `for#[T]` segment.
    #[serde(rename = "scip_callee_type")]
    ScipCalleeTypeRow {
        sym: String,
        #[serde(rename = "type")]
        receiver_type: String,
    },
    /// v5 `scip_local(fn, name)`: a local binding or parameter attributed to
    /// its enclosing callable.
    #[serde(rename = "scip_local")]
    ScipLocalRow {
        #[serde(rename = "fn")]
        enclosing_fn: String,
        name: String,
    },
    /// v5 `scip_impl(impl, iface)`: the implements / overrides edge, from a
    /// SymbolInformation relationship with `is_implementation`.
    #[serde(rename = "scip_impl")]
    ScipImplRow {
        #[serde(rename = "impl")]
        implementor: String,
        iface: String,
    },
    /// `symbol(symbol, path, kind)`: one definition a fast `.scm` query
    /// captured, `kind` being the capture label's tail (`function`, `variable`).
    #[serde(rename = "symbol")]
    SymbolRow {
        symbol: String,
        path: String,
        kind: String,
    },
    /// `occurrence(symbol, path, start, end, role, exported, decl_start,
    /// decl_end)`. A `ref` row is never exported and repeats its own span.
    #[serde(rename = "occurrence")]
    OccurrenceRow {
        symbol: String,
        path: String,
        start: u32,
        end: u32,
        role: String,
        exported: bool,
        decl_start: u32,
        decl_end: u32,
    },
    /// `free_name(path, owner_start, owner_end, name, start, end)`: a name the
    /// owning top-level item needs from outside itself. Owner = file when none.
    #[serde(rename = "free_name")]
    FreeNameRow {
        path: String,
        owner_start: u32,
        owner_end: u32,
        name: String,
        start: u32,
        end: u32,
    },
    /// Written callee text and its innermost callable, without resolution.
    #[serde(rename = "call_site")]
    CallSiteRow {
        callee: String,
        path: String,
        line: u32,
        #[serde(rename = "fn")]
        enclosing_fn: String,
        start: u32,
        end: u32,
    },
    /// Element identity is (path, start); fragments use `<fragment>`.
    #[serde(rename = "jsx_element")]
    JsxElementRow {
        name: String,
        path: String,
        line: u32,
        #[serde(rename = "fn")]
        enclosing_fn: String,
        start: u32,
        end: u32,
        parent_start: Option<u32>,
    },
    /// Attributes join their element by (path, element_start). A boolean
    /// attribute has no value; a spread uses name `..` and its written text.
    #[serde(rename = "jsx_attribute")]
    JsxAttributeRow {
        path: String,
        element_start: u32,
        name: String,
        value: Option<String>,
        start: u32,
        end: u32,
    },
    /// `local(fn, name, path, start, end)`: a binding the document does not
    /// export, attributed to its enclosing callable. File-private fns too.
    #[serde(rename = "local")]
    LocalRow {
        #[serde(rename = "fn")]
        enclosing_fn: String,
        name: String,
        path: String,
        start: u32,
        end: u32,
    },
    /// The `scip` family's index header: which tool answered, and whether an
    /// index already on disk was reused or one was built. Self-diagnosis on the
    /// wire, so a caller never has to ask why a stream is the size it is.
    /// The index PATH is deliberately absent: it is machine-dependent and would
    /// pin a checkout location into every golden. It goes to stderr instead.
    #[serde(rename = "scip_index")]
    ScipIndexRow {
        reused: bool,
        tool_name: String,
        tool_version: String,
        documents: u32,
        /// Index file mtime as milliseconds since the Unix epoch. Null when
        /// the filesystem does not provide a readable post-epoch mtime.
        index_mtime_unix_ms: Option<u64>,
        /// Filesystem evidence only: stale, uncertain, or no_newer_sources.
        /// Equal/older mtimes do not establish semantic freshness.
        staleness: String,
    },
    /// A NAMED SKIP: one detected indexer produced no index, and why. This is a
    /// row rather than an exit code on purpose. A root with no toolchain must
    /// not kill its caller (v5's law: a missing indexer skips the repo, it never
    /// fails the tick), and it must not produce a silently empty stream either,
    /// which reads as "this project has no symbols". `reason` is the stable
    /// slug to match on, `detail` the human half.
    #[serde(rename = "scip_skip")]
    ScipSkipRow {
        lang: String,
        bin: String,
        reason: String,
        detail: String,
    },
    /// A NAMED SKIP on SIZE, `scip_skip`'s per-file twin: one input was over the
    /// byte ceiling, so it was not parsed. `limit` rides the row: it is a flag.
    #[serde(rename = "size_skip")]
    SizeSkipRow {
        path: String,
        bytes: u64,
        limit: u64,
        reason: String,
    },
    /// One SCIP occurrence: a symbol mentioned at a byte span in one document.
    /// RAW index fact, deliberately unjoined. v5's `scip_def` is this row with
    /// `definition` true, `scip_ref` is it with `definition` false, and
    /// `scip_local` is it with a `local `-prefixed symbol; those splits are one
    /// filter each in the dl layer, which is where the machines live.
    #[serde(rename = "scip_occurrence")]
    ScipOccurrenceRow {
        path: String,
        symbol: String,
        start: u32,
        end: u32,
        /// The raw scip.proto SymbolRole bitfield, kept whole so no role is
        /// lost in projection.
        roles: i32,
        /// The seven SymbolRole bits, one column each. Hoisted for the same
        /// reason `definition` always was: bit arithmetic in a dl rule is
        /// worse than a column, and `roles` alone left six of the seven roles
        /// unreachable from the language.
        definition: bool,
        import: bool,
        write_access: bool,
        read_access: bool,
        generated: bool,
        test: bool,
        forward_definition: bool,
        /// The raw scip.proto SyntaxKind ordinal (0 = unspecified).
        syntax_kind: i32,
        /// The nearest enclosing AST node's byte span, null when the indexer
        /// emitted no enclosing range or the range did not convert.
        enclosing_start: Option<u32>,
        enclosing_end: Option<u32>,
        /// The source slice at the occurrence's byte span, lossy-utf8. Absent
        /// (not null, not empty) unless `--occurrence-text` asked for it and
        /// the span fits the corpus bytes; the answer to v5 scip_binding's
        /// `local_name` (issue extract-scip-vocab-occurrence-binding).
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
    },
    /// One range-specific documentation string on an occurrence
    /// (scip.proto `Occurrence.override_documentation`). `pos` is its index in
    /// the repeated field, so a multi-paragraph doc keeps its order.
    #[serde(rename = "scip_occurrence_doc")]
    ScipOccurrenceDocRow {
        path: String,
        start: u32,
        end: u32,
        pos: u32,
        text: String,
    },
    /// One compiler diagnostic the indexer reported at an occurrence's range
    /// (scip.proto `Occurrence.diagnostics`). `severity` and `tags` are raw
    /// enum ordinals; `tags` is a JSON array because the field is repeated.
    #[serde(rename = "scip_diagnostic")]
    ScipDiagnosticRow {
        path: String,
        start: u32,
        end: u32,
        severity: i32,
        code: String,
        message: String,
        source: String,
        tags: Vec<i32>,
    },
    /// One SCIP symbol information row: v5's `scip_name`. `path` is the
    /// document that declared it, or null for an index's external symbols.
    #[serde(rename = "scip_symbol")]
    ScipSymbolRow {
        path: Option<String>,
        symbol: String,
        display_name: String,
        /// The raw scip.proto SymbolInformation.Kind enum value.
        kind: i32,
        /// The owning symbol of a local symbol; empty for global symbols.
        enclosing_symbol: String,
    },
    /// One markdown docstring entry on a symbol
    /// (scip.proto `SymbolInformation.documentation`). `pos` is its index in
    /// the repeated field.
    #[serde(rename = "scip_documentation")]
    ScipDocumentationRow {
        symbol: String,
        pos: u32,
        text: String,
    },
    /// One rendered type signature (scip.proto
    /// `SymbolInformation.signature_documentation`).
    #[serde(rename = "scip_signature")]
    ScipSignatureRow {
        symbol: String,
        language: String,
        text: String,
    },
    /// One reference inside a signature's text. `start`/`end` are byte offsets
    /// into the SIGNATURE TEXT, never into a document, which is why this is
    /// its own record instead of another `scip_occurrence`.
    #[serde(rename = "scip_signature_occurrence")]
    ScipSignatureOccurrenceRow {
        symbol: String,
        ref_symbol: String,
        start: u32,
        end: u32,
        roles: i32,
    },
    /// One index's metadata (scip.proto `Metadata` + `ToolInfo`), one row per
    /// index. `project_root` is the only place an index states what corpus it
    /// describes, and the tool identity is what a ledger entry needs to say
    /// which indexer release produced a fact.
    #[serde(rename = "scip_metadata")]
    ScipMetadataRow {
        version: i32,
        tool_name: String,
        tool_version: String,
        tool_arguments: Vec<String>,
        project_root: String,
        text_document_encoding: i32,
    },
    /// One indexed document's own header (scip.proto `Document` minus its
    /// repeated children). `text` is null unless the indexer inlined the
    /// document's contents, which it does only for virtual documents.
    #[serde(rename = "scip_document")]
    ScipDocumentRow {
        path: String,
        language: String,
        position_encoding: i32,
        text: Option<String>,
    },
    /// One SCIP relationship between two symbols: v5's `scip_impl` and the
    /// symbol half of `scip_edge`. The four flags are not exclusive; scip.proto
    /// sets several at once for an overriding method.
    #[serde(rename = "scip_relationship")]
    ScipRelationshipRow {
        symbol: String,
        related_symbol: String,
        is_reference: bool,
        is_implementation: bool,
        is_type_definition: bool,
        is_definition: bool,
    },
    /// One file-to-file dependency edge, derived from a SCIP index: `src_path`
    /// contains a non-definition occurrence of a symbol whose definition lives
    /// in `dst_path`. `symbols` is how many distinct symbols cross that edge.
    ///
    /// This is the ONE derived relation the extractor projects rather than
    /// leaving to the dl layer, and the reason is measured, not stylistic: over
    /// v6/tsv2 (212 TypeScript files) the raw occurrence rows are 122,317 and
    /// the edges they fold to are 755. Shipping the occurrences to compute the
    /// edges above the wire is a 160x amplification of a fact one pass over a
    /// hashmap produces here. The raw rows stay available under `ryi scip --raw`
    /// for every other join.
    ///
    /// It is v5's `module_edge` by another name, and it exists because v6 has no
    /// TypeScript module resolver; SCIP bypasses the resolver entirely.
    ///
    /// `kind` is the `SpecifierKind` slug that bound the crossing, so one
    /// (src, dst) pair carries one row per import form and `symbols` counts the
    /// distinct names of THAT form. `--scip-deps` fills it `unknown`: an index
    /// records resolved occurrences, never the statement that bound the name.
    #[serde(rename = "file_edge")]
    FileEdgeRow {
        src_path: String,
        dst_path: String,
        kind: String,
        symbols: u32,
    },
    /// One specifier `--deps` could not turn into an edge, with the resolution
    /// policy that stopped it. v5 called it `module_unresolved`.
    ///
    /// A stop is a FACT, not an absence: `rxjs` stopping at the node_modules
    /// boundary and `./gone.ts` naming nothing are different answers, and
    /// without this row both read as silence.
    #[serde(rename = "file_unresolved")]
    FileUnresolvedRow {
        src_path: String,
        module: String,
        reason: String,
    },
    /// One workspace-internal manifest-to-manifest dependency edge, keyed on
    /// the two manifest paths rather than package names.
    ///
    /// v5's `crate_edge` (`src/graph/modgraph/rust.rs:468`) was Cargo-only and
    /// keyed on crate names. The path key is the same key `file_edge` uses, so
    /// the two grains join without a name dictionary.
    #[serde(rename = "package_edge")]
    PackageEdgeRow {
        src_manifest: String,
        dst_manifest: String,
        kind: String,
    },
    /// One import binding, resolved through the LANGUAGE'S OWN module plane
    /// (ECMAScript ResolveExport for ts/js); all five arms emit it, see
    /// `src/lang/*_modules.rs` and `ts_resolve.rs`. Column meanings live at `--schema`.
    /// @comment-ok: the cross-language contract a second arm has to honor
    #[serde(rename = "resolved_import")]
    ResolvedImportRow {
        src_path: String,
        name: String,
        local: String,
        target_path: String,
        target_name: Option<String>,
        kind: String,
        hops: u32,
    },
    /// One file, once: its byte length and line count. v5's `file_lines` and
    /// the size half of `content`. `digest` is the same ContentId the phase-2
    /// cache and every resolved edge key on, so this row is what lets a
    /// consumer join a path to the content key without hashing the file again.
    #[serde(rename = "file")]
    FileRow {
        path: String,
        digest: String,
        bytes: u32,
        lines: u32,
    },
    /// Gated on `--lines`: every newline byte offset, in order, keyed by the
    /// same `digest` `file` carries. `src/lang/ts.rs:2832`: byte->line index only.
    #[serde(rename = "line_start")]
    LineStartRow {
        path: String,
        digest: String,
        offsets: Vec<u32>,
    },
}
