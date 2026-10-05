//! Demand-scoped semantic rows from the stock tsgo API hosted by the LSP process.
use super::ts7_lsp_session::{file_uri, Ts7Rpc, TsSession, TS_SESSIONS};
use super::{CheckerAnswers, CheckerRef};
use crate::read::tsi::{Arg, FactOut};
use crate::read::types::RyiOutput;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub use super::ts_checker::{TsDemand, TsSite};

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

fn lsp_position(text: &str, byte: u32) -> Result<Value, String> {
    let before = text
        .get(..byte as usize)
        .ok_or("site offset is not a UTF-8 boundary")?;
    Ok(
        json!({"line": before.bytes().filter(|byte| *byte == b'\n').count(),
        "character": before.rsplit('\n').next().unwrap_or("").encode_utf16().count()}),
    )
}

fn byte_at(text: &str, position: &Value) -> Result<u32, String> {
    let line = position["line"].as_u64().ok_or("definition has no line")? as usize;
    let column = position["character"]
        .as_u64()
        .ok_or("definition has no character")? as usize;
    let mut base = 0;
    let row = text
        .split_inclusive('\n')
        .nth(line)
        .ok_or("definition line out of range")?;
    for preceding in text.split_inclusive('\n').take(line) {
        base += preceding.len();
    }
    let mut units = 0;
    for (byte, ch) in row.char_indices() {
        if units == column {
            return Ok((base + byte) as u32);
        }
        units += ch.len_utf16();
    }
    if units == column {
        return Ok((base + row.len()) as u32);
    }
    Err("definition column is not a UTF-16 boundary".into())
}

pub fn answer(
    root: &Path,
    files: &[(String, PathBuf, TsDemand)],
    tsi: bool,
) -> Result<CheckerAnswers, String> {
    let load = std::time::Instant::now();
    let sessions = TS_SESSIONS.get_or_init(Default::default);
    let mut sessions = sessions.lock().map_err(|_| "tsgo session lock poisoned")?;
    if !sessions.contains_key(root) {
        sessions.insert(root.to_owned(), TsSession::open(root)?);
    }
    let session = sessions.get_mut(root).unwrap();
    session.set_deadline(Some(load + std::time::Duration::from_secs(90)));
    let result = answer_session(session, files, tsi, load);
    session.set_deadline(None);
    if result.is_err() {
        sessions.remove(root);
    }
    result
}

fn answer_session(
    session: &mut TsSession,
    files: &[(String, PathBuf, TsDemand)],
    tsi: bool,
    load: std::time::Instant,
) -> Result<CheckerAnswers, String> {
    let mut answers = CheckerAnswers {
        version: session.version.clone(),
        ..CheckerAnswers::default()
    };
    let mut definitions = BTreeMap::new();
    for (supplied, absolute, demand) in files {
        let text = std::fs::read_to_string(absolute).map_err(|error| error.to_string())?;
        for site in &demand.sites {
            let response = session.lsp.request("textDocument/definition", &json!({
                "textDocument": {"uri": file_uri(absolute)?.as_str()}, "position": lsp_position(&text, site.start)? }))?;
            if let Some(error) = response.error {
                return Err(format!("definition: {error:?}"));
            }
            let value = response.result.unwrap_or(Value::Null);
            let locations: Vec<&Value> = match &value {
                Value::Array(rows) => rows.iter().collect(),
                Value::Null => Vec::new(),
                _ => vec![&value],
            };
            // Ambiguous definitions remain for the syntax leg.
            if locations.len() != 1 {
                continue;
            }
            let location = locations[0];
            let uri = location["uri"]
                .as_str()
                .or_else(|| location["targetUri"].as_str())
                .ok_or("definition has no URI")?;
            let destination = url::Url::parse(uri)
                .map_err(|error| error.to_string())?
                .to_file_path()
                .map_err(|_| "definition URI is not a file")?;
            let source =
                std::fs::read_to_string(&destination).map_err(|error| error.to_string())?;
            let range = if location["range"].is_object() {
                &location["range"]
            } else {
                &location["targetSelectionRange"]
            };
            let start = byte_at(&source, &range["start"])?;
            let end = byte_at(&source, &range["end"])?;
            let name = source
                .get(start as usize..end as usize)
                .ok_or("definition span out of range")?
                .to_string();
            let corpus_path = files
                .iter()
                .find(|(_, path, _)| *path == destination)
                .map(|(supplied, _, _)| supplied.clone());
            let dst_path = corpus_path.clone().unwrap_or_default();
            let dst_name = if corpus_path.is_some() {
                name
            } else {
                format!(
                    "{}:{name}",
                    destination
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                )
            };
            let reference = CheckerRef {
                start: site.start,
                end: site.end,
                name: site.name.clone(),
                dst_path,
                dst_name,
                dst_offset: start,
            };
            if site.call {
                answers
                    .calls
                    .entry(supplied.clone())
                    .or_default()
                    .push(reference.clone());
            }
            if site.type_ref {
                answers
                    .types
                    .entry(supplied.clone())
                    .or_default()
                    .push(reference);
            }
            definitions.insert(
                (supplied.clone(), site.start, site.end),
                (
                    corpus_path.unwrap_or_else(|| destination.to_string_lossy().into_owned()),
                    start,
                    end,
                ),
            );
        }
    }
    answers.load = load.elapsed();
    answers.files_answered = files
        .iter()
        .filter(|(_, _, demand)| !demand.sites.is_empty())
        .count();
    if !tsi {
        return Ok(answers);
    }
    let walk = std::time::Instant::now();
    let needed: Vec<_> = files
        .iter()
        .filter(|(_, _, demand)| !demand.sites.is_empty())
        .map(|(_, file, _)| file)
        .collect();
    if needed.is_empty() {
        return Ok(answers);
    }
    let api = session.initialize_api()?;
    let snapshot = api.call("updateSnapshot", &json!({"openFiles": needed}))?["snapshot"].clone();
    let result = collect_semantic_rows(api, &snapshot, files, &definitions, &mut answers);
    // Snapshot handles never escape this request. Release opens even after failure.
    let close = api.call("updateSnapshot", &json!({"closeFiles": needed}));
    let dispose = api.call("release", &json!({"snapshot": snapshot}));
    if let Ok(value) = &close {
        api.call("release", &json!({"snapshot": value["snapshot"]}))?;
    }
    result?;
    close?;
    dispose?;
    answers.walk = walk.elapsed();
    Ok(answers)
}

type Definitions = BTreeMap<(String, u32, u32), (String, u32, u32)>;
struct Rows<'a> {
    api: &'a mut Ts7Rpc,
    snapshot: &'a Value,
    facts: Vec<FactOut>,
    next: u32,
    types: BTreeMap<(String, u64), u32>,
    symbols: BTreeMap<u64, u32>,
    expanded: BTreeSet<(String, u64)>,
    corpus: Vec<String>,
}
impl Rows<'_> {
    fn emit(&mut self, relation: &str, args: Vec<Arg>) -> Result<(), String> {
        crate::read::tsi::registry::check(relation, &args)?;
        self.facts.push(FactOut {
            fact: 0,
            relation: relation.into(),
            args,
        });
        Ok(())
    }
    fn id(&mut self) -> u32 {
        let id = self.next;
        self.next += 1;
        id
    }
    fn call(&mut self, project: &str, method: &str, fields: Value) -> Result<Value, String> {
        let mut params = fields;
        params["snapshot"] = self.snapshot.clone();
        params["project"] = json!(project);
        self.api.call(method, &params)
    }
    fn symbol(&mut self, symbol: &Value) -> Result<u32, String> {
        let handle = symbol["id"].as_u64().ok_or("symbol has no handle")?;
        if let Some(id) = self.symbols.get(&handle) {
            return Ok(*id);
        }
        let id = self.id();
        self.symbols.insert(handle, id);
        self.emit("tsi.symbol", vec![Arg::Id(id)])?;
        self.emit(
            "tsi.name",
            vec![
                Arg::Id(id),
                Arg::Text(symbol["name"].as_str().unwrap_or("").into()),
            ],
        )?;
        Ok(id)
    }
    fn ty(&mut self, project: &str, ty: &Value, expand: bool, depth: usize) -> Result<u32, String> {
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
            let rendered = self.call(project, "typeToString", json!({"type": handle}))?;
            self.emit(
                "tsi.name",
                vec![
                    Arg::Id(id),
                    Arg::Text(
                        rendered
                            .as_str()
                            .ok_or("typeToString did not return text")?
                            .into(),
                    ),
                ],
            )?;
            id
        };
        if expand {
            self.expanded.insert(key);
        }
        let flags = ty["flags"].as_u64().unwrap_or(0);
        let primitive = [
            (1, "any"),
            (2, "unknown"),
            (4, "undefined"),
            (8, "null"),
            (16, "void"),
            (32 | 1024, "string"),
            (64 | 2048, "number"),
            (128 | 4096, "bigint"),
            (256 | 8192, "boolean"),
            (512 | 16384, "symbol"),
            (262144, "never"),
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
        if flags & (134217728 | 268435456) != 0 {
            if flags & 134217728 != 0 {
                self.emit("tsi.sum", vec![Arg::Id(id)])?;
            } else {
                self.emit("tsi.product", vec![Arg::Id(id)])?;
            }
            let parts = self.call(project, "getTypesOfType", json!({"objectId": handle}))?;
            for (position, part) in parts
                .as_array()
                .ok_or("type constituents are not an array")?
                .iter()
                .enumerate()
            {
                let target = self.ty(project, part, true, depth + 1)?;
                let edge = self.id();
                self.emit(
                    "tsi.edge",
                    vec![
                        Arg::Id(edge),
                        Arg::Id(id),
                        Arg::Text(position.to_string()),
                        Arg::Id(target),
                        Arg::Int(position as i64),
                    ],
                )?;
            }
        }
        if flags & 1048576 != 0 {
            self.emit("tsi.product", vec![Arg::Id(id)])?;
            let object_flags = ty["objectFlags"].as_u64().unwrap_or(0);
            if object_flags & 2 != 0 {
                self.emit("ts.interface", vec![Arg::Id(id)])?;
            }
            let application = if ty["aliasTypeArguments"]
                .as_array()
                .is_some_and(|arguments| !arguments.is_empty())
            {
                let symbol =
                    self.call(project, "getAliasSymbolOfType", json!({"objectId": handle}))?;
                let generic = self.call(
                    project,
                    "getDeclaredTypeOfSymbol",
                    json!({"symbol": symbol["id"]}),
                )?;
                let arguments = self.call(
                    project,
                    "getAliasTypeArgumentsOfType",
                    json!({"objectId": handle}),
                )?;
                Some((generic, arguments))
            } else if object_flags & 4 != 0
                && ty["target"].as_u64().is_some_and(|target| target != handle)
            {
                let generic = self.call(project, "getTargetOfType", json!({"objectId": handle}))?;
                let arguments = self.call(project, "getTypeArguments", json!({"type": handle}))?;
                Some((generic, arguments))
            } else {
                None
            };
            if let Some((generic, arguments)) = application {
                let callee = self.ty(project, &generic, false, depth + 1)?;
                let list = self.id();
                self.emit(
                    "tsi.called",
                    vec![Arg::Id(id), Arg::Id(callee), Arg::Id(list)],
                )?;
                for (position, argument) in arguments
                    .as_array()
                    .ok_or("type arguments are not an array")?
                    .iter()
                    .enumerate()
                {
                    let argument = self.ty(project, argument, true, depth + 1)?;
                    self.emit(
                        "tsi.argument",
                        vec![Arg::Id(list), Arg::Int(position as i64), Arg::Id(argument)],
                    )?;
                }
            }
            let properties = self.call(project, "getPropertiesOfType", json!({"type": handle}))?;
            for (position, property) in properties
                .as_array()
                .ok_or("properties are not an array")?
                .iter()
                .enumerate()
            {
                let prop_type = self.call(
                    project,
                    "getTypeOfSymbol",
                    json!({"symbol": property["id"]}),
                )?;
                if prop_type.is_null() {
                    continue;
                }
                // Expand structural callables; nominal library/dependency owners remain leaves.
                let structural = if prop_type["symbol"].is_null() {
                    true
                } else {
                    let owner = self.call(
                        project,
                        "getSymbolOfType",
                        json!({"objectId": prop_type["id"]}),
                    )?;
                    owner["declarations"]
                        .as_array()
                        .is_some_and(|declarations| {
                            declarations.iter().any(|declaration| {
                                declaration.as_str().is_some_and(|handle| {
                                    handle.splitn(3, '.').nth(2).is_some_and(|path| {
                                        self.corpus
                                            .iter()
                                            .any(|file| file.eq_ignore_ascii_case(path))
                                    })
                                })
                            })
                        })
                };
                let target = self.ty(project, &prop_type, structural, depth + 1)?;
                let edge = self.id();
                self.emit(
                    "tsi.edge",
                    vec![
                        Arg::Id(edge),
                        Arg::Id(id),
                        Arg::Text(property["name"].as_str().unwrap_or("").into()),
                        Arg::Id(target),
                        Arg::Int(position as i64),
                    ],
                )?;
                if property["flags"].as_u64().unwrap_or(0) & 16777216 != 0 {
                    self.emit("ts.optional", vec![Arg::Id(edge)])?;
                }
            }
            if object_flags & 3 != 0 {
                let bases = self.call(project, "getBaseTypes", json!({"type": handle}))?;
                for base in bases.as_array().ok_or("base types are not an array")? {
                    let target = self.ty(project, base, false, depth + 1)?;
                    self.emit(
                        "tsi.conforms",
                        vec![Arg::Id(id), Arg::Id(target), Arg::Atom("declared".into())],
                    )?;
                }
                let parameters = self.call(
                    project,
                    "getTypeParametersOfType",
                    json!({"objectId": handle}),
                )?;
                for (position, parameter) in parameters
                    .as_array()
                    .ok_or("type parameters are not an array")?
                    .iter()
                    .enumerate()
                {
                    let target = self.ty(project, parameter, false, depth + 1)?;
                    self.emit(
                        "tsi.parameter",
                        vec![
                            Arg::Id(target),
                            Arg::Id(id),
                            Arg::Int(position as i64),
                            Arg::Atom("unspecified".into()),
                        ],
                    )?;
                }
            }
            let signatures = self.call(
                project,
                "getSignaturesOfType",
                json!({"type": handle, "kind": 0}),
            )?;
            if !signatures
                .as_array()
                .ok_or("signatures are not an array")?
                .is_empty()
            {
                self.emit("tsi.callable", vec![Arg::Id(id)])?;
            }
            for (signature_index, signature) in signatures.as_array().unwrap().iter().enumerate() {
                let type_parameters = self.call(
                    project,
                    "getTypeParametersOfSignature",
                    json!({"objectId": signature["id"]}),
                )?;
                for (position, parameter) in type_parameters
                    .as_array()
                    .ok_or("signature type parameters are not an array")?
                    .iter()
                    .enumerate()
                {
                    let parameter = self.ty(project, parameter, false, depth + 1)?;
                    self.emit(
                        "tsi.parameter",
                        vec![
                            Arg::Id(parameter),
                            Arg::Id(id),
                            Arg::Int((signature_index * 1000 + position) as i64),
                            Arg::Atom("unspecified".into()),
                        ],
                    )?;
                }
                let parameters = self.call(
                    project,
                    "getParametersOfSignature",
                    json!({"objectId": signature["id"]}),
                )?;
                let parameters = parameters
                    .as_array()
                    .ok_or("signature parameters are not an array")?;
                if parameters.len() >= 1000 {
                    return Err("signature exceeds TSI position stride 1000".into());
                }
                for (position, parameter) in parameters.iter().enumerate() {
                    let ty = self.call(
                        project,
                        "getTypeOfSymbol",
                        json!({"symbol": parameter["id"]}),
                    )?;
                    let target = self.ty(project, &ty, true, depth + 1)?;
                    self.emit(
                        "tsi.input",
                        vec![
                            Arg::Id(id),
                            Arg::Int((signature_index * 1000 + position) as i64),
                            Arg::Id(target),
                        ],
                    )?;
                }
                let ty = self.call(
                    project,
                    "getReturnTypeOfSignature",
                    json!({"signature": signature["id"]}),
                )?;
                let target = self.ty(project, &ty, false, depth + 1)?;
                self.emit(
                    "tsi.output",
                    vec![
                        Arg::Id(id),
                        Arg::Int(signature_index as i64),
                        Arg::Id(target),
                    ],
                )?;
            }
        }
        Ok(id)
    }
}

fn collect_semantic_rows(
    api: &mut Ts7Rpc,
    snapshot: &Value,
    files: &[(String, PathBuf, TsDemand)],
    definitions: &Definitions,
    answers: &mut CheckerAnswers,
) -> Result<(), String> {
    let mut rows = Rows {
        api,
        snapshot,
        facts: Vec::new(),
        next: 0,
        types: BTreeMap::new(),
        symbols: BTreeMap::new(),
        expanded: BTreeSet::new(),
        corpus: files
            .iter()
            .map(|(_, path, _)| path.to_string_lossy().into_owned())
            .collect(),
    };
    for (supplied, file, demand) in files {
        if demand.sites.is_empty() {
            continue;
        }
        let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
        let project = rows.api.call(
            "getDefaultProjectForFile",
            &json!({"snapshot": snapshot,"file": file}),
        )?;
        let project = project["id"]
            .as_str()
            .ok_or("no default project for requested file")?;
        let positions: Vec<usize> = demand
            .sites
            .iter()
            .map(|site| text[..site.start as usize].encode_utf16().count())
            .collect();
        let types = rows.call(
            project,
            "getTypesAtPositions",
            json!({"file": file,"positions": positions}),
        )?;
        let symbols = rows.call(
            project,
            "getSymbolsAtPositions",
            json!({"file": file,"positions": positions}),
        )?;
        let types = types
            .as_array()
            .ok_or("types at positions are not an array")?;
        let symbols = symbols
            .as_array()
            .ok_or("symbols at positions are not an array")?;
        if types.len() != demand.sites.len() || symbols.len() != demand.sites.len() {
            return Err("stock API position answer cardinality mismatch".into());
        }
        let mut handles = Vec::new();
        for ((site, ty), symbol) in demand.sites.iter().zip(types).zip(symbols) {
            if ty.is_null() {
                handles.push(None);
                continue;
            }
            let mut ty = ty.clone();
            let mut symbol = symbol.clone();
            if !symbol.is_null() && symbol["flags"].as_u64().unwrap_or(0) & 2097152 != 0 {
                symbol = rows.call(project, "getAliasedSymbol", json!({"symbol": symbol["id"]}))?;
            }
            let declaration = definitions
                .get(&(supplied.clone(), site.start, site.end))
                .is_some_and(|(path, start, _)| path == supplied && *start == site.start);
            if declaration
                && !symbol.is_null()
                && symbol["flags"].as_u64().unwrap_or(0) & (32 | 64 | 262144 | 524288) != 0
            {
                ty = rows.call(
                    project,
                    "getDeclaredTypeOfSymbol",
                    json!({"symbol": symbol["id"]}),
                )?;
            }
            let id = rows.ty(project, &ty, true, 0)?;
            handles.push(Some((ty["id"].clone(), id)));
            rows.emit(
                "tsi.has_type",
                vec![
                    Arg::Span(supplied.clone(), site.start, site.end),
                    Arg::Id(id),
                ],
            )?;
            if let Some((path, start, end)) = definitions
                .get(&(supplied.clone(), site.start, site.end))
                .filter(|_| declaration)
            {
                rows.emit(
                    "tsi.origin",
                    vec![
                        Arg::Id(id),
                        Arg::Atom("ts".into()),
                        Arg::Span(path.clone(), *start, *end),
                    ],
                )?;
            }
            if !symbol.is_null() {
                let symbol_id = rows.symbol(&symbol)?;
                rows.emit("tsi.denotes", vec![Arg::Id(symbol_id), Arg::Id(id)])?;
                if let Some((path, start, end)) =
                    definitions.get(&(supplied.clone(), site.start, site.end))
                {
                    rows.emit(
                        "tsi.origin",
                        vec![
                            Arg::Id(symbol_id),
                            Arg::Atom("ts".into()),
                            Arg::Span(path.clone(), *start, *end),
                        ],
                    )?;
                }
            }
        }
        for &(source, target) in &demand.pairs {
            let source = handles
                .get(source)
                .ok_or("assignability source site out of range")?;
            let target = handles
                .get(target)
                .ok_or("assignability target site out of range")?;
            if let (Some((source_handle, source_id)), Some((target_handle, target_id))) =
                (source, target)
            {
                let assignable = rows.call(
                    project,
                    "isTypeAssignableTo",
                    json!({"source": source_handle,"target": target_handle}),
                )?;
                if assignable
                    .as_bool()
                    .ok_or("assignability did not return a boolean")?
                {
                    rows.emit(
                        "tsi.assignable",
                        vec![
                            Arg::Id(*source_id),
                            Arg::Id(*target_id),
                            Arg::Atom("structural".into()),
                        ],
                    )?;
                }
            }
        }
    }
    // Same handle can be encountered through multiple sites; identical rows are one fact.
    let mut seen = BTreeSet::new();
    rows.facts
        .retain(|fact| seen.insert(serde_json::to_string(fact).unwrap()));
    let relations: BTreeSet<String> = rows
        .facts
        .iter()
        .map(|fact| fact.relation.clone())
        .chain(
            [
                "tsi.assignable",
                "tsi.conforms",
                "tsi.subtype",
                "tsi.equivalent",
                "ts.mapped",
                "ts.conditional",
                "ts.readonly",
            ]
            .into_iter()
            .map(str::to_string),
        )
        .collect();
    answers.coverage = relations.into_iter().map(|relation| {
        let detail = match relation.as_str() {
            "tsi.subtype" => "stock tsgo exposes no subtype or strict_subtype API; no rows produced",
            "tsi.equivalent" => "stock tsgo exposes no identical or comparable API; no rows produced",
            "tsi.assignable" => "stock per-pair assignability for requested site pairs only",
            "tsi.has_type" => "stock getTypesAtPositions for requested spans only; type-reference token queries can return any; application structure comes from returned type handles",
            "ts.mapped" => "stock type API exposes computed properties but no mapped key/constraint/template decomposition; no rows produced",
            "ts.conditional" => "conditional type decomposition is not requested by this tier; no rows produced",
            "ts.readonly" => "stock property symbol responses carry no declaration readonly modifier; no rows produced",
            "tsi.conforms" => "stock getBaseTypes for requested class and interface types only; implements clauses and structural conformance are not enumerated",
            _ => "stock API type structure for requested sites only; dependency owners remain leaves; expansion stops after 32 levels",
        };
        (relation, false, Some(detail.to_string()))
    }).collect();
    answers.tsi = rows.facts;
    Ok(())
}
