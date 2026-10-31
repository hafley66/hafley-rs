//! Demand-scoped slow TS answers from the TypeScript 7.1 API, served in-process by
//! tsrs (`tsrs_api::Session`): the API's own method names and parameters, no
//! subprocess. `TsApi` is the seam; `TsrsSession` is the one implementation.
use super::{CheckerAnswers, CheckerRef};
use crate::read::tsi::{Arg, FactOut};
use crate::read::types::RyiOutput;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub use super::ts_checker::{TsDemand, TsSite};

/// One API answer: JSON, or the binary encoding `getSourceFile` returns.
pub enum Reply {
    Json(Value),
    Binary(Vec<u8>),
}

/// The TypeScript 7.1 API as ryi calls it: a method name and its parameters,
/// spelled as the API client spells them.
pub trait TsApi {
    fn call(&self, method: &str, params: &Value) -> Result<Reply, String>;

    fn json(&self, method: &str, params: &Value) -> Result<Value, String> {
        match self.call(method, params)? {
            Reply::Json(value) => Ok(value),
            Reply::Binary(_) => Err(format!("{method}: binary answer where JSON was expected")),
        }
    }
}

/// tsrs's API session in this process, rooted at the corpus.
pub struct TsrsSession {
    session: std::sync::Arc<tsrs_api::Session>,
}

impl TsrsSession {
    pub fn open(root: &Path) -> Self {
        let fs: std::sync::Arc<dyn tsrs_vfs::FS> =
            std::sync::Arc::new(tsrs_vfs::bundled::wrap_fs(tsrs_vfs::osvfs::fs()));
        TsrsSession {
            session: tsrs_api::Session::new(tsrs_api::SessionOptions {
                cwd: root.to_string_lossy().into_owned(),
                default_library_path: tsrs_vfs::bundled::lib_path(),
                fs,
                binary_responses: true,
                run_external_code: false,
            }),
        }
    }
}

impl TsApi for TsrsSession {
    fn call(&self, method: &str, params: &Value) -> Result<Reply, String> {
        use tsrs_api::Handler;
        let body = serde_json::to_vec(params).map_err(|error| error.to_string())?;
        match self.session.handle_request(method, &body) {
            Ok(tsrs_api::Response::Json(text)) => serde_json::from_str(&text)
                .map(Reply::Json)
                .map_err(|error| format!("{method}: {error}")),
            Ok(tsrs_api::Response::Binary(bytes)) => Ok(Reply::Binary(bytes)),
            Err(error) => Err(format!("{method}: {error}")),
        }
    }
}

/// The version string run rows carry for this tier.
pub const VERSION: &str = "tsrs 0.20.1-ts7.1.0-beta";

/// Join the requested parse's names to identifier spans, without asking the server
/// about unrelated identifiers or opening documents.
pub fn demand(text: &str, path: &Path, output: &RyiOutput, tsi: bool) -> TsDemand {
    use oxc_ast::ast::{BindingIdentifier, IdentifierName, IdentifierReference};
    use oxc_ast_visit::Visit;
    #[derive(Default)]
    struct Names(Vec<(u32, u32, String)>);
    impl<'a> Visit<'a> for Names {
        fn visit_binding_identifier(&mut self, node: &BindingIdentifier<'a>) {
            self.0
                .push((node.span.start, node.span.end, node.name.to_string()));
        }
        fn visit_identifier_reference(&mut self, node: &IdentifierReference<'a>) {
            self.0
                .push((node.span.start, node.span.end, node.name.to_string()));
        }
        fn visit_identifier_name(&mut self, node: &IdentifierName<'a>) {
            self.0
                .push((node.span.start, node.span.end, node.name.to_string()));
        }
    }
    let allocator = oxc_allocator::Allocator::default();
    let source_type = oxc_span::SourceType::from_path(path).unwrap_or_default();
    let parsed = oxc_parser::Parser::new(&allocator, text, source_type).parse();
    let mut names = Names::default();
    names.visit_program(&parsed.program);
    names.0.sort();
    names.0.dedup();
    let mut sites: BTreeMap<(u32, u32), TsSite> = BTreeMap::new();
    if let Some(call) = &output.call {
        for site in &call.aux.sites {
            let name = output.strings.lookup(site.callee);
            if let Some((start, end, _)) = names.0.iter().rev().find(|(start, end, written)| {
                *start >= site.span.start && *end <= site.span.end() && written == name
            }) {
                sites.insert(
                    (*start, *end),
                    TsSite {
                        start: *start,
                        end: *end,
                        name: name.into(),
                        call: true,
                        type_ref: false,
                    },
                );
            }
        }
    }
    if let Some(types) = &output.types {
        for candidate in &types.aux.candidates {
            let name = output.strings.lookup(candidate.to);
            let tail = name.rsplit('.').next().unwrap_or(name);
            for (start, end, written) in &names.0 {
                if *start >= candidate.owner.start
                    && *end <= candidate.owner.end()
                    && written == tail
                {
                    sites
                        .entry((*start, *end))
                        .or_insert(TsSite {
                            start: *start,
                            end: *end,
                            name: name.into(),
                            call: false,
                            type_ref: true,
                        })
                        .type_ref = true;
                }
            }
        }
        if tsi {
            for node in &types.nodes {
                let Some(name) = node.name.map(|id| output.strings.lookup(id)) else {
                    continue;
                };
                if let Some((start, end, _)) = names.0.iter().find(|(start, end, written)| {
                    *start >= node.span.start && *end <= node.span.end() && written == name
                }) {
                    sites.entry((*start, *end)).or_insert(TsSite {
                        start: *start,
                        end: *end,
                        name: name.into(),
                        call: false,
                        type_ref: false,
                    });
                }
            }
            for fact in &types.aux.tsi {
                let span = match fact.relation.as_str() {
                    "tsi.has_type" => fact.args.first(),
                    "tsi.origin" => fact.args.get(2),
                    _ => None,
                };
                if let Some(Arg::Span(_, start, end)) = span {
                    if let Some(written) = text.get(*start as usize..*end as usize) {
                        sites.entry((*start, *end)).or_insert(TsSite {
                            start: *start,
                            end: *end,
                            name: written.into(),
                            call: false,
                            type_ref: false,
                        });
                    }
                }
            }
        }
    }
    let sites: Vec<TsSite> = sites.into_values().collect();
    let mut pairs = BTreeSet::new();
    if tsi {
        if let Some(types) = &output.types {
            for candidate in &types.aux.candidates {
                // Written heritage names the two endpoints of this question.
                if !matches!(candidate.kind, crate::read::types::TypeEdgeKind::Impl) {
                    continue;
                }
                let owner = types.nodes.iter().find(|node| node.span == candidate.owner);
                let source = owner.and_then(|node| node.name).and_then(|name| {
                    sites.iter().position(|site| {
                        site.name == output.strings.lookup(name)
                            && site.start >= candidate.owner.start
                            && site.end <= candidate.owner.end()
                    })
                });
                let target = sites.iter().position(|site| {
                    site.type_ref
                        && site.name == output.strings.lookup(candidate.to)
                        && site.start >= candidate.owner.start
                        && site.end <= candidate.owner.end()
                });
                if let (Some(source), Some(target)) = (source, target) {
                    pairs.insert((source, target));
                }
            }
        }
    }
    TsDemand {
        sites,
        pairs: pairs.into_iter().collect(),
    }
}

/// One source file as the API encoded it. A node handle's index is a record
/// index in this encoding (JSDoc and NodeList records included), so lookups
/// read the records; the decoded tree supplies only the text and file name.
struct Decoded {
    bytes: Vec<u8>,
    nodes: usize,
    count: u32,
    text: String,
    file_name: String,
    map: tsrs_ast::positionmap::PositionMap,
}

/// One node record: kind (`None` for a NodeList), UTF-16 pos/end, parent index.
#[derive(Clone, Copy)]
struct Record {
    kind: Option<tsrs_ast::Kind>,
    raw_kind: u32,
    pos: u32,
    end: u32,
    parent: u32,
}

impl Decoded {
    fn new(bytes: Vec<u8>) -> Result<Self, String> {
        use tsrs_api_codec::format::{HEADER_OFFSET_NODES, NODE_SIZE};
        let decoded = tsrs_api_codec::decode_source_file(&bytes).map_err(|error| format!("getSourceFile: {error}"))?;
        let source = decoded.root().as_source_file_p();
        let (text, file_name) = (source.text().to_string(), source.file_name().to_string());
        let nodes = u32::from_le_bytes(bytes[HEADER_OFFSET_NODES..HEADER_OFFSET_NODES + 4].try_into().unwrap()) as usize;
        let count = ((bytes.len() - nodes) / NODE_SIZE) as u32;
        let map = tsrs_ast::positionmap::compute_position_map(&text);
        Ok(Decoded { bytes, nodes, count, text, file_name, map })
    }

    fn record(&self, index: u32) -> Option<Record> {
        use tsrs_api_codec::format::{NODE_OFFSET_END, NODE_OFFSET_KIND, NODE_OFFSET_PARENT, NODE_OFFSET_POS, NODE_SIZE};
        if index == 0 || index >= self.count {
            return None;
        }
        let base = self.nodes + index as usize * NODE_SIZE;
        let field = |offset: usize| u32::from_le_bytes(self.bytes[base + offset..base + offset + 4].try_into().unwrap());
        let raw_kind = field(NODE_OFFSET_KIND);
        Some(Record {
            kind: tsrs_api_codec::decoder::kind_from_u32(raw_kind),
            raw_kind,
            pos: field(NODE_OFFSET_POS),
            end: field(NODE_OFFSET_END),
            parent: field(NODE_OFFSET_PARENT),
        })
    }

    /// Whether `ancestor` is on `index`'s parent chain.
    fn descends(&self, mut index: u32, ancestor: u32) -> bool {
        while let Some(record) = self.record(index) {
            if record.parent == ancestor {
                return true;
            }
            if record.parent == 0 || record.parent >= index {
                return false;
            }
            index = record.parent;
        }
        false
    }

    /// A declaration's name record: its first direct child that is a name.
    fn name_of(&self, index: u32) -> Option<u32> {
        use tsrs_ast::Kind;
        let mut child = index + 1;
        while self.descends(child, index) {
            let record = self.record(child)?;
            if record.parent == index
                && matches!(
                    record.kind,
                    Some(Kind::Identifier | Kind::PrivateIdentifier | Kind::StringLiteral | Kind::NumericLiteral
                        | Kind::ComputedPropertyName | Kind::NoSubstitutionTemplateLiteral)
                )
            {
                return Some(child);
            }
            child += 1;
        }
        None
    }

    /// A record's span in UTF-8 bytes, leading trivia skipped.
    fn span(&self, index: u32) -> Option<(u32, u32, String)> {
        let record = self.record(index)?;
        let start = tsrs_scanner::skip_trivia(&self.text, self.map.utf16_to_utf8(record.pos));
        let end = self.map.utf16_to_utf8(record.end);
        Some((start, end, self.text.get(start as usize..end as usize)?.to_string()))
    }

    /// A declaration's name span (or its own when it has no name child).
    fn name_span(&self, index: u32) -> Option<(u32, u32, String)> {
        self.span(self.name_of(index).unwrap_or(index))
    }

    /// The record indexes from the root down to `index`, NodeLists skipped.
    fn path_to(&self, index: u32) -> Vec<u32> {
        let mut path = Vec::new();
        let mut current = index;
        while let Some(record) = self.record(current) {
            if record.kind.is_some() {
                path.push(current);
            }
            if record.parent == 0 || record.parent >= current {
                break;
            }
            current = record.parent;
        }
        path.reverse();
        path
    }

    /// One SCIP descriptor for a container or the declaration itself; `None`
    /// for records that name nothing a symbol path carries.
    fn descriptor(&self, index: u32) -> Option<String> {
        use tsrs_ast::Kind;
        let record = self.record(index)?;
        let name = || self.name_of(index).and_then(|name| self.span(name)).map(|(_, _, text)| escape(&text));
        Some(match record.kind? {
            Kind::ModuleDeclaration => format!("{}/", name()?),
            Kind::ClassDeclaration | Kind::InterfaceDeclaration | Kind::EnumDeclaration | Kind::TypeAliasDeclaration => {
                format!("{}#", name()?)
            }
            Kind::FunctionDeclaration | Kind::MethodDeclaration | Kind::MethodSignature => format!("{}().", name()?),
            Kind::Constructor => "`<constructor>`().".to_string(),
            Kind::PropertyDeclaration | Kind::PropertySignature | Kind::EnumMember | Kind::VariableDeclaration
            | Kind::GetAccessor | Kind::SetAccessor | Kind::PropertyAssignment | Kind::ShorthandPropertyAssignment => {
                format!("{}.", name()?)
            }
            Kind::Parameter => format!("({})", name()?),
            Kind::TypeParameter => format!("[{}]", name()?),
            Kind::ClassExpression if self.name_of(index).is_some() => format!("{}#", name()?),
            // An anonymous function or class names its place by its first byte.
            Kind::ArrowFunction | Kind::FunctionExpression | Kind::ClassExpression => {
                format!("`@{}`/", self.span(index)?.0)
            }
            _ => return None,
        })
    }

    /// The innermost written type node (`TypeReference`, `ExpressionWithTypeArguments`)
    /// covering a UTF-16 position.
    fn written_type_at(&self, unit: u32) -> Option<Record> {
        use tsrs_ast::Kind;
        (1..self.count)
            .filter_map(|index| self.record(index).map(|record| (index, record)))
            .filter(|(_, record)| {
                matches!(record.kind, Some(Kind::TypeReference | Kind::ExpressionWithTypeArguments))
                    && record.pos <= unit
                    && unit < record.end
            })
            .min_by_key(|(_, record)| record.end - record.pos)
            .map(|(index, record)| Record { parent: index, ..record })
    }
}

/// SCIP's identifier escape: a name outside `[A-Za-z0-9_+$-]` goes in backticks.
fn escape(name: &str) -> String {
    if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || "_+$-".contains(c)) {
        name.to_string()
    } else {
        format!("`{}`", name.replace('`', "``"))
    }
}

/// The nearest package.json above `file`: (name, version, directory).
fn npm_package_of(file: &Path, cache: &mut HashMap<PathBuf, (String, String, PathBuf)>) -> (String, String, PathBuf) {
    for dir in file.ancestors().skip(1) {
        if let Some(found) = cache.get(dir) {
            return found.clone();
        }
        let manifest = dir.join("package.json");
        if let Some(value) = std::fs::read(crate::read::io_path(&manifest)).ok().and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok()) {
            let field = |key: &str| value[key].as_str().filter(|text| !text.is_empty()).unwrap_or(".").to_string();
            let found = (field("name"), field("version"), dir.to_path_buf());
            cache.insert(dir.to_path_buf(), found.clone());
            return found;
        }
    }
    (".".into(), ".".into(), file.parent().unwrap_or(file).to_path_buf())
}

/// One snapshot over the requested files and the source files it decoded.
struct Snapshot<'a> {
    api: &'a dyn TsApi,
    id: Value,
    files: HashMap<String, Decoded>,
    packages: HashMap<PathBuf, (String, String, PathBuf)>,
}

impl Snapshot<'_> {
    fn with(&self, project: &Value, fields: Value) -> Value {
        let mut params = fields;
        params["snapshot"] = self.id.clone();
        params["project"] = project.clone();
        params
    }

    fn json(&self, project: &Value, method: &str, fields: Value) -> Result<Value, String> {
        self.api.json(method, &self.with(project, fields))
    }

    fn decoded(&mut self, project: &Value, path: &str) -> Result<&Decoded, String> {
        if !self.files.contains_key(path) {
            let bytes = match self.api.call("getSourceFile", &self.with(project, json!({"file": path})))? {
                Reply::Binary(bytes) => bytes,
                Reply::Json(_) => return Err("getSourceFile: JSON answer where the encoded file was expected".into()),
            };
            self.files.insert(path.to_string(), Decoded::new(bytes)?);
        }
        Ok(&self.files[path])
    }

    /// The one declaration a symbol names: (real-case file path, name start, end, name).
    fn declaration(&mut self, project: &Value, symbol: &Value) -> Result<Option<(String, u32, u32, String)>, String> {
        let declarations = symbol["declarations"].as_array().map(Vec::as_slice).unwrap_or_default();
        let [handle] = declarations else { return Ok(None) };
        let Some(parsed) = handle.as_str().and_then(tsrs_api_codec::parse_node_handle) else { return Ok(None) };
        let (path, index) = (parsed.path.to_string(), parsed.index);
        let decoded = self.decoded(project, &path)?;
        Ok(decoded.name_span(index).map(|(start, end, name)| (decoded.file_name.clone(), start, end, name)))
    }

    /// The symbol's stable id in SCIP grammar, from its first declaration:
    /// `ryi-ts71 npm <package> <version> `<file>`/<descriptors>`. The same
    /// declaration names the same text in every run.
    fn scip_symbol(&mut self, project: &Value, symbol: &Value) -> Result<Option<String>, String> {
        let Some(parsed) = symbol["declarations"][0].as_str().and_then(tsrs_api_codec::parse_node_handle) else {
            return Ok(None);
        };
        let (path, index) = (parsed.path.to_string(), parsed.index);
        let decoded = self.decoded(project, &path)?;
        let descriptors: String = decoded.path_to(index).into_iter().filter_map(|step| decoded.descriptor(step)).collect();
        if descriptors.is_empty() {
            return Ok(None);
        }
        let file = PathBuf::from(&decoded.file_name);
        let (name, version, root) = npm_package_of(&file, &mut self.packages);
        let relative = file.strip_prefix(&root).unwrap_or(&file).to_string_lossy().replace('`', "``");
        Ok(Some(format!("ryi-ts71 npm {name} {version} `{relative}`/{descriptors}")))
    }

    /// The written type node a type-reference token sits in (`Mapper<T>` in
    /// `implements Mapper<T>`); the API answers `any` at the token itself.
    fn written_type(&mut self, project: &Value, file: &str, start: u32) -> Result<Option<Value>, String> {
        // Handle paths are the API's path keys: lowercased where the file system folds case.
        let key = if cfg!(any(target_os = "macos", target_os = "windows")) { file.to_lowercase() } else { file.to_string() };
        let decoded = self.decoded(project, file)?;
        let Some(found) = decoded.written_type_at(decoded.map.utf8_to_utf16(start)) else { return Ok(None) };
        let location = format!("{}.{}.{}", found.parent, found.raw_kind, key);
        let method = if found.kind == Some(tsrs_ast::Kind::TypeReference) { "getTypeFromTypeNode" } else { "getTypeAtLocation" };
        let ty = self.json(project, method, json!({"location": location}))?;
        Ok((!ty.is_null()).then_some(ty))
    }
}

type Definitions = BTreeMap<(String, u32, u32), (String, u32, u32)>;

/// Every reference to the symbol at `seed` (absolute path, UTF-8 byte) inside
/// `files` (supplied path, absolute path), as (supplied path, start, end) in
/// UTF-8 bytes, file order then source order. The seed's own declaration is
/// one of them. `None`: no symbol at the seed.
pub fn references(
    api: &dyn TsApi,
    files: &[(String, PathBuf)],
    seed: (&Path, u32),
) -> Result<Option<Vec<(String, u32, u32)>>, String> {
    let created = api.json("createSnapshot", &json!({"openFiles": files.iter().map(|(_, absolute)| absolute).collect::<Vec<_>>()}))?;
    let mut snapshot = Snapshot { api, id: created["snapshot"].clone(), files: HashMap::new(), packages: HashMap::new() };
    let result = references_in(&mut snapshot, files, seed);
    let released = api.json("release", &json!({"snapshot": snapshot.id}));
    let found = result?;
    released?;
    Ok(found)
}

fn references_in(
    snapshot: &mut Snapshot,
    files: &[(String, PathBuf)],
    (seed_file, seed_byte): (&Path, u32),
) -> Result<Option<Vec<(String, u32, u32)>>, String> {
    let seed_path = seed_file.to_string_lossy().into_owned();
    let project = snapshot.api.json("getDefaultProjectForFile", &json!({"snapshot": snapshot.id, "file": seed_path}))?["id"].clone();
    let text = std::fs::read_to_string(seed_file).map_err(|error| error.to_string())?;
    let unit = tsrs_ast::positionmap::compute_position_map(&text).utf8_to_utf16(seed_byte);
    let symbols = snapshot.json(&project, "getSymbolsAtPositions", json!({"file": seed_path, "positions": [unit]}))?;
    let symbol = &symbols[0];
    if symbol.is_null() {
        return Ok(None);
    }
    let symbol = if symbol["flags"].as_u64().unwrap_or(0) & 2097152 != 0 {
        snapshot.json(&project, "getAliasedSymbol", json!({"symbol": symbol["reference"]}))?
    } else {
        symbol.clone()
    };
    let mut found = Vec::new();
    for (supplied, absolute) in files {
        let file = absolute.to_string_lossy().into_owned();
        let handles = snapshot.json(&project, "getReferencesToSymbolInFile", json!({"file": file, "symbol": symbol["reference"]}))?;
        let mut spans = Vec::new();
        for handle in handles.as_array().map(Vec::as_slice).unwrap_or_default() {
            let Some(parsed) = handle.as_str().and_then(tsrs_api_codec::parse_node_handle) else { continue };
            let (path, index) = (parsed.path.to_string(), parsed.index);
            let decoded = snapshot.decoded(&project, &path)?;
            if let Some((start, end, _)) = decoded.span(index) {
                spans.push((supplied.clone(), start, end));
            }
        }
        spans.sort();
        found.extend(spans);
    }
    Ok(Some(found))
}

/// Answer every site in `files` from one snapshot of an in-process tsrs session:
/// definitions always, the TSI walk when `tsi`.
pub fn ask(root: &Path, files: &[(String, PathBuf, TsDemand)], tsi: bool) -> Result<CheckerAnswers, String> {
    ask_with(&TsrsSession::open(root), files, tsi)
}

pub fn ask_with(api: &dyn TsApi, files: &[(String, PathBuf, TsDemand)], tsi: bool) -> Result<CheckerAnswers, String> {
    let load = Instant::now();
    let needed: Vec<&(String, PathBuf, TsDemand)> = files.iter().filter(|(_, _, demand)| !demand.sites.is_empty()).collect();
    let mut answers = CheckerAnswers { version: VERSION.into(), files_answered: needed.len(), ..CheckerAnswers::default() };
    if needed.is_empty() {
        return Ok(answers);
    }
    let corpus: HashMap<String, String> = files
        .iter()
        .map(|(supplied, absolute, _)| (absolute.to_string_lossy().to_lowercase(), supplied.clone()))
        .collect();
    let created = api.json("createSnapshot", &json!({"openFiles": needed.iter().map(|(_, absolute, _)| absolute).collect::<Vec<_>>()}))?;
    let mut snapshot = Snapshot { api, id: created["snapshot"].clone(), files: HashMap::new(), packages: HashMap::new() };
    let result = answer_files(&mut snapshot, &needed, &corpus, tsi, &mut answers);
    // The snapshot's programs and checkers free with it, also after a failure.
    let released = api.json("release", &json!({"snapshot": snapshot.id}));
    result?;
    released?;
    answers.load = load.elapsed();
    Ok(answers)
}

fn answer_files(
    snapshot: &mut Snapshot,
    needed: &[&(String, PathBuf, TsDemand)],
    corpus: &HashMap<String, String>,
    tsi: bool,
    answers: &mut CheckerAnswers,
) -> Result<(), String> {
    let mut definitions = Definitions::new();
    let mut per_file = Vec::new();
    for (supplied, absolute, demand) in needed {
        let file = absolute.to_string_lossy().into_owned();
        let project = snapshot.api.json("getDefaultProjectForFile", &json!({"snapshot": snapshot.id, "file": file}))?["id"].clone();
        if project.is_null() {
            continue;
        }
        let text = std::fs::read_to_string(absolute).map_err(|error| error.to_string())?;
        let map = tsrs_ast::positionmap::compute_position_map(&text);
        let positions: Vec<u32> = demand.sites.iter().map(|site| map.utf8_to_utf16(site.start)).collect();
        let symbols = snapshot.json(&project, "getSymbolsAtPositions", json!({"file": file, "positions": positions}))?;
        let symbols = symbols.as_array().cloned().ok_or("symbols at positions are not an array")?;
        if symbols.len() != demand.sites.len() {
            return Err("getSymbolsAtPositions answer cardinality mismatch".into());
        }
        let mut aliased = Vec::with_capacity(symbols.len());
        for (site, symbol) in demand.sites.iter().zip(&symbols) {
            let symbol = if symbol["flags"].as_u64().unwrap_or(0) & 2097152 != 0 {
                snapshot.json(&project, "getAliasedSymbol", json!({"symbol": symbol["reference"]}))?
            } else {
                symbol.clone()
            };
            if let Some((path, start, end, name)) = snapshot.declaration(&project, &symbol)? {
                let in_corpus = corpus.get(&path.to_lowercase()).cloned();
                let file_name = Path::new(&path).file_name().unwrap_or_default().to_string_lossy();
                let reference = CheckerRef {
                    start: site.start,
                    end: site.end,
                    name: site.name.clone(),
                    dst_path: in_corpus.clone().unwrap_or_default(),
                    dst_name: if in_corpus.is_some() { name } else { format!("{file_name}:{name}") },
                    dst_offset: start,
                };
                if site.call {
                    answers.calls.entry(supplied.clone()).or_default().push(reference.clone());
                }
                if site.type_ref {
                    answers.types.entry(supplied.clone()).or_default().push(reference);
                }
                definitions.insert((supplied.clone(), site.start, site.end), (in_corpus.unwrap_or(path), start, end));
            }
            aliased.push(symbol);
        }
        per_file.push((supplied, file, demand, project, positions, aliased));
    }
    if !tsi {
        return Ok(());
    }
    let mut rows = Rows { facts: Vec::new(), next: 0, types: BTreeMap::new(), symbols: BTreeMap::new(), expanded: BTreeSet::new(), corpus };
    for (supplied, file, demand, project, positions, symbols) in per_file {
        let types = snapshot.json(&project, "getTypesAtPositions", json!({"file": file, "positions": positions}))?;
        let types = types.as_array().cloned().ok_or("types at positions are not an array")?;
        if types.len() != demand.sites.len() {
            return Err("getTypesAtPositions answer cardinality mismatch".into());
        }
        let mut handles = Vec::new();
        for ((site, ty), symbol) in demand.sites.iter().zip(types).zip(&symbols) {
            let key = (supplied.clone(), site.start, site.end);
            let declaration = definitions.get(&key).is_some_and(|(path, start, _)| path == supplied && *start == site.start);
            let declared = symbol["flags"].as_u64().unwrap_or(0) & (32 | 64 | 384 | 262144 | 524288) != 0;
            let mut ty = ty;
            if declared && declaration {
                ty = snapshot.json(&project, "getDeclaredTypeOfSymbol", json!({"symbol": symbol["reference"]}))?;
            } else if declared && (ty.is_null() || ty["flags"].as_u64() == Some(1)) {
                ty = match snapshot.written_type(&project, &file, site.start)? {
                    Some(written) => written,
                    None => snapshot.json(&project, "getDeclaredTypeOfSymbol", json!({"symbol": symbol["reference"]}))?,
                };
            }
            if ty.is_null() {
                handles.push(None);
                continue;
            }
            let id = rows.ty(snapshot, &project, &ty, true, 0)?;
            handles.push(Some((ty["id"].clone(), id)));
            rows.emit("tsi.has_type", vec![Arg::Span(supplied.clone(), site.start, site.end), Arg::Id(id)])?;
            if let Some((path, start, end)) = definitions.get(&key).filter(|_| declaration) {
                rows.emit("tsi.origin", vec![Arg::Id(id), Arg::Atom("ts".into()), Arg::Span(path.clone(), *start, *end)])?;
            }
            if !symbol.is_null() {
                let symbol_id = rows.symbol(snapshot, &project, symbol)?;
                rows.emit("tsi.denotes", vec![Arg::Id(symbol_id), Arg::Id(id)])?;
                if let Some((path, start, end)) = definitions.get(&key) {
                    rows.emit("tsi.origin", vec![Arg::Id(symbol_id), Arg::Atom("ts".into()), Arg::Span(path.clone(), *start, *end)])?;
                }
            }
        }
        for &(source, target) in &demand.pairs {
            let source = handles.get(source).ok_or("assignability source site out of range")?;
            let target = handles.get(target).ok_or("assignability target site out of range")?;
            if let (Some((source_handle, source_id)), Some((target_handle, target_id))) = (source, target) {
                let assignable = snapshot.json(&project, "isTypeAssignableTo", json!({"source": source_handle, "target": target_handle}))?;
                if assignable.as_bool().ok_or("assignability did not return a boolean")? {
                    rows.emit("tsi.assignable", vec![Arg::Id(*source_id), Arg::Id(*target_id), Arg::Atom("structural".into())])?;
                }
            }
        }
    }
    let mut seen = BTreeSet::new();
    rows.facts.retain(|fact| seen.insert(serde_json::to_string(fact).unwrap()));
    let relations: BTreeSet<String> = rows
        .facts
        .iter()
        .map(|fact| fact.relation.clone())
        .chain(["tsi.assignable", "tsi.conforms", "tsi.subtype", "tsi.equivalent", "ts.mapped", "ts.conditional", "ts.readonly"].map(str::to_string))
        .collect();
    answers.coverage = relations
        .into_iter()
        .map(|relation| {
            let detail = match relation.as_str() {
                "tsi.subtype" => "the TS 7.1 API exposes no subtype or strict_subtype query; no rows produced",
                "tsi.equivalent" => "the TS 7.1 API exposes no identical or comparable query; no rows produced",
                "tsi.assignable" => "isTypeAssignableTo for requested site pairs only",
                "tsi.has_type" => "getTypeAtPosition for requested spans only; a type-reference token answers the written type node around it",
                "ts.mapped" => "mapped key/constraint/template decomposition is not requested by this tier; no rows produced",
                "ts.conditional" => "conditional type decomposition is not requested by this tier; no rows produced",
                "ts.readonly" => "readonly modifiers are not requested by this tier; no rows produced",
                "tsi.conforms" => "getBaseTypes for requested class and interface types only; implements clauses and structural conformance are not enumerated",
                _ => "TS 7.1 API type structure for requested sites only; dependency owners remain leaves; expansion stops after 32 levels",
            };
            (relation, false, Some(detail.to_string()))
        })
        .collect();
    answers.tsi = rows.facts;
    Ok(())
}

struct Rows<'a> {
    facts: Vec<FactOut>,
    next: u32,
    types: BTreeMap<(String, u64), u32>,
    symbols: BTreeMap<String, u32>,
    expanded: BTreeSet<(String, u64)>,
    corpus: &'a HashMap<String, String>,
}

impl Rows<'_> {
    fn emit(&mut self, relation: &str, args: Vec<Arg>) -> Result<(), String> {
        crate::read::tsi::registry::check(relation, &args)?;
        self.facts.push(FactOut { fact: 0, relation: relation.into(), args });
        Ok(())
    }

    fn id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }

    fn symbol(&mut self, snapshot: &mut Snapshot, project: &Value, symbol: &Value) -> Result<u32, String> {
        let handle = symbol["reference"].to_string();
        if let Some(id) = self.symbols.get(&handle) {
            return Ok(*id);
        }
        let id = self.id();
        self.symbols.insert(handle, id);
        self.emit("tsi.symbol", vec![Arg::Id(id)])?;
        self.emit("tsi.name", vec![Arg::Id(id), Arg::Text(symbol["name"].as_str().unwrap_or("").into())])?;
        if let Some(text) = snapshot.scip_symbol(project, symbol)? {
            self.emit("tsi.scip_symbol", vec![Arg::Id(id), Arg::Text(text)])?;
        }
        Ok(id)
    }

    fn in_corpus(&self, symbol: &Value) -> bool {
        symbol["declarations"].as_array().is_some_and(|declarations| {
            declarations.iter().any(|handle| {
                handle
                    .as_str()
                    .and_then(tsrs_api_codec::parse_node_handle)
                    .is_some_and(|parsed| self.corpus.contains_key(&parsed.path.to_lowercase()))
            })
        })
    }

    fn ty(&mut self, snapshot: &Snapshot, project: &Value, ty: &Value, expand: bool, depth: usize) -> Result<u32, String> {
        let expand = expand && depth < 32;
        let handle = ty["id"].as_u64().ok_or("type has no handle")?;
        let key = (project.to_string(), handle);
        let id = if let Some(id) = self.types.get(&key).copied() {
            if !expand || self.expanded.contains(&key) {
                return Ok(id);
            }
            id
        } else {
            let id = self.id();
            self.types.insert(key.clone(), id);
            self.emit("tsi.type", vec![Arg::Id(id)])?;
            let rendered = snapshot.json(project, "typeToString", json!({"type": handle}))?;
            self.emit("tsi.name", vec![Arg::Id(id), Arg::Text(rendered.as_str().ok_or("typeToString did not return text")?.into())])?;
            id
        };
        if expand {
            self.expanded.insert(key);
        }
        let flags = ty["flags"].as_u64().unwrap_or(0);
        let primitive = [
            (1, "any"), (2, "unknown"), (4, "undefined"), (8, "null"), (16, "void"),
            (32 | 1024, "string"), (64 | 2048, "number"), (128 | 4096, "bigint"),
            (256 | 8192, "boolean"), (512 | 16384, "symbol"), (262144, "never"),
        ]
        .into_iter()
        .find(|(mask, _)| flags & mask != 0);
        if let Some((_, name)) = primitive {
            self.emit("tsi.primitive", vec![Arg::Id(id), Arg::Atom(name.into())])?;
            return Ok(id);
        }
        if !expand {
            return Ok(id);
        }
        let list = |value: Value, what: &str| value.as_array().cloned().ok_or(format!("{what} are not an array"));
        if flags & (134217728 | 268435456) != 0 {
            self.emit(if flags & 134217728 != 0 { "tsi.sum" } else { "tsi.product" }, vec![Arg::Id(id)])?;
            let parts = list(snapshot.json(project, "getTypesOfType", json!({"objectId": handle}))?, "type constituents")?;
            for (position, part) in parts.iter().enumerate() {
                let target = self.ty(snapshot, project, part, true, depth + 1)?;
                let edge = self.id();
                self.emit("tsi.edge", vec![Arg::Id(edge), Arg::Id(id), Arg::Text(position.to_string()), Arg::Id(target), Arg::Int(position as i64)])?;
            }
        }
        if flags & 1048576 != 0 {
            self.emit("tsi.product", vec![Arg::Id(id)])?;
            let object_flags = ty["objectFlags"].as_u64().unwrap_or(0);
            if object_flags & 2 != 0 {
                self.emit("ts.interface", vec![Arg::Id(id)])?;
            }
            let application = if ty["aliasTypeArguments"].as_array().is_some_and(|arguments| !arguments.is_empty()) {
                let alias = snapshot.json(project, "getAliasSymbolOfType", json!({"objectId": handle}))?;
                let generic = snapshot.json(project, "getDeclaredTypeOfSymbol", json!({"symbol": alias["reference"]}))?;
                let arguments = snapshot.json(project, "getAliasTypeArgumentsOfType", json!({"objectId": handle}))?;
                (!generic.is_null()).then_some((generic, arguments))
            } else if object_flags & 4 != 0 && ty["target"].as_u64().is_some_and(|target| target != handle) {
                let generic = snapshot.json(project, "getTargetOfType", json!({"objectId": handle}))?;
                let arguments = snapshot.json(project, "getTypeArguments", json!({"type": handle}))?;
                Some((generic, arguments))
            } else {
                None
            };
            if let Some((generic, arguments)) = application {
                let callee = self.ty(snapshot, project, &generic, false, depth + 1)?;
                let list_id = self.id();
                self.emit("tsi.called", vec![Arg::Id(id), Arg::Id(callee), Arg::Id(list_id)])?;
                for (position, argument) in list(arguments, "type arguments")?.iter().enumerate() {
                    let argument = self.ty(snapshot, project, argument, true, depth + 1)?;
                    self.emit("tsi.argument", vec![Arg::Id(list_id), Arg::Int(position as i64), Arg::Id(argument)])?;
                }
            }
            let properties = list(snapshot.json(project, "getPropertiesOfType", json!({"type": handle}))?, "properties")?;
            for (position, property) in properties.iter().enumerate() {
                let prop_type = snapshot.json(project, "getTypeOfSymbol", json!({"symbol": property["reference"]}))?;
                if prop_type.is_null() {
                    continue;
                }
                // Expand structural callables; nominal library/dependency owners remain leaves.
                let structural = if prop_type["symbol"].is_null() {
                    true
                } else {
                    let owner = snapshot.json(project, "getSymbolOfType", json!({"objectId": prop_type["id"]}))?;
                    self.in_corpus(&owner)
                };
                let target = self.ty(snapshot, project, &prop_type, structural, depth + 1)?;
                let edge = self.id();
                self.emit("tsi.edge", vec![Arg::Id(edge), Arg::Id(id), Arg::Text(property["name"].as_str().unwrap_or("").into()), Arg::Id(target), Arg::Int(position as i64)])?;
                if property["flags"].as_u64().unwrap_or(0) & 16777216 != 0 {
                    self.emit("ts.optional", vec![Arg::Id(edge)])?;
                }
            }
            if object_flags & 3 != 0 {
                for base in list(snapshot.json(project, "getBaseTypes", json!({"type": handle}))?, "base types")? {
                    let target = self.ty(snapshot, project, &base, false, depth + 1)?;
                    self.emit("tsi.conforms", vec![Arg::Id(id), Arg::Id(target), Arg::Atom("declared".into())])?;
                }
                let parameters = list(snapshot.json(project, "getTypeParametersOfType", json!({"objectId": handle}))?, "type parameters")?;
                for (position, parameter) in parameters.iter().enumerate() {
                    let target = self.ty(snapshot, project, parameter, false, depth + 1)?;
                    self.emit("tsi.parameter", vec![Arg::Id(target), Arg::Id(id), Arg::Int(position as i64), Arg::Atom("unspecified".into())])?;
                }
            }
            let signatures = list(snapshot.json(project, "getSignaturesOfType", json!({"type": handle, "kind": 0}))?, "signatures")?;
            if !signatures.is_empty() {
                self.emit("tsi.callable", vec![Arg::Id(id)])?;
            }
            for (index, signature) in signatures.iter().enumerate() {
                let type_parameters = list(snapshot.json(project, "getTypeParametersOfSignature", json!({"objectId": signature["id"]}))?, "signature type parameters")?;
                for (position, parameter) in type_parameters.iter().enumerate() {
                    let parameter = self.ty(snapshot, project, parameter, false, depth + 1)?;
                    self.emit("tsi.parameter", vec![Arg::Id(parameter), Arg::Id(id), Arg::Int((index * 1000 + position) as i64), Arg::Atom("unspecified".into())])?;
                }
                let parameters = list(snapshot.json(project, "getParametersOfSignature", json!({"objectId": signature["id"]}))?, "signature parameters")?;
                if parameters.len() >= 1000 {
                    return Err("signature exceeds TSI position stride 1000".into());
                }
                for (position, parameter) in parameters.iter().enumerate() {
                    let parameter_type = snapshot.json(project, "getTypeOfSymbol", json!({"symbol": parameter["reference"]}))?;
                    if parameter_type.is_null() {
                        continue;
                    }
                    let target = self.ty(snapshot, project, &parameter_type, true, depth + 1)?;
                    self.emit("tsi.input", vec![Arg::Id(id), Arg::Int((index * 1000 + position) as i64), Arg::Id(target)])?;
                }
                let returned = snapshot.json(project, "getReturnTypeOfSignature", json!({"objectId": signature["id"]}))?;
                if !returned.is_null() {
                    let target = self.ty(snapshot, project, &returned, false, depth + 1)?;
                    self.emit("tsi.output", vec![Arg::Id(id), Arg::Int(index as i64), Arg::Id(target)])?;
                }
            }
        }
        Ok(id)
    }
}
