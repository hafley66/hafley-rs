//! Demand-scoped slow TS answers from the TypeScript 7.1 API: `node` runs
//! `ts71_driver.mjs` on `tsrs/sync`, served by the `tsrs --api` binary.
use super::{CheckerAnswers, CheckerRef};
use crate::read::tsi::{Arg, FactOut};
use crate::read::types::RyiOutput;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub use super::ts_checker::{TsDemand, TsSite};

/// The driver, staged next to the temp dir under its content hash on first use.
const DRIVER: &str = include_str!("ts71_driver.mjs");
/// One slow TS run's wall budget, the same the LSP tier had.
const BUDGET: Duration = Duration::from_secs(90);

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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    version: String,
    calls: HashMap<String, Vec<(u32, u32, String, String, String, u32)>>,
    types: HashMap<String, Vec<(u32, u32, String, String, String, u32)>>,
    tsi: Vec<FactOut>,
    coverage: Vec<(String, bool, Option<String>)>,
    files_answered: usize,
}

fn references(rows: Vec<(u32, u32, String, String, String, u32)>) -> Vec<CheckerRef> {
    rows.into_iter()
        .map(|(start, end, name, dst_path, dst_name, dst_offset)| CheckerRef {
            start,
            end,
            name,
            dst_path,
            dst_name,
            dst_offset,
        })
        .collect()
}

/// The driver file on disk; its name carries the content hash, so a newer build
/// never runs an older staged copy.
fn driver() -> Result<PathBuf, String> {
    let digest = crate::read::types::content_id_of(DRIVER.as_bytes()).to_string();
    let digest = digest.rsplit(':').next().unwrap_or(&digest);
    let path = std::env::temp_dir().join(format!("ryi-ts71-driver-{}.mjs", &digest[..16.min(digest.len())]));
    if !path.is_file() {
        let staging = path.with_extension(format!("{}.tmp", std::process::id()));
        std::fs::write(&staging, DRIVER).map_err(|error| format!("stage ts71 driver: {error}"))?;
        std::fs::rename(&staging, &path).map_err(|error| format!("stage ts71 driver: {error}"))?;
    }
    Ok(path)
}

/// Answer every site in `files` through one driver process: definitions always,
/// the TSI walk when `tsi`.
pub fn ask(
    root: &Path,
    files: &[(String, PathBuf, TsDemand)],
    tsi: bool,
) -> Result<CheckerAnswers, String> {
    let load = Instant::now();
    let package = super::ts_lib::tsrs_package(Some(root))
        .ok_or("no tsrs package: install tsrs beside sprefa-extract (ts7/) or in the project")?;
    let server = super::ts_lib::tsrs_executable(&package)
        .ok_or_else(|| format!("no tsrs binary for this platform under {}", package.display()))?;
    let request = serde_json::json!({
        "root": root,
        "package": package,
        "server": server,
        "tsi": tsi,
        "files": files.iter().map(|(supplied, absolute, demand)| serde_json::json!({
            "supplied": supplied,
            "absolute": absolute,
            "sites": demand.sites.iter()
                .map(|site| (site.start, site.end, &site.name, site.call, site.type_ref))
                .collect::<Vec<_>>(),
            "pairs": demand.pairs,
        })).collect::<Vec<_>>(),
    });
    let node = std::env::var_os("SPREFA_NODE").unwrap_or_else(|| "node".into());
    let mut child = Command::new(&node)
        .arg(driver()?)
        .current_dir(root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("start node: {error}"))?;
    let mut stdin = child.stdin.take().ok_or("ts71 driver stdin unavailable")?;
    let mut stdout = child.stdout.take().ok_or("ts71 driver stdout unavailable")?;
    let mut stderr = child.stderr.take().ok_or("ts71 driver stderr unavailable")?;
    let body = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
    let writer = std::thread::spawn(move || stdin.write_all(&body));
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).map(|_| out)
    });
    let errors = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stderr.read_to_string(&mut out);
        out
    });
    let status = loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            break status;
        }
        if load.elapsed() > BUDGET {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!("ts71 driver exceeded {}s", BUDGET.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let _ = writer.join();
    let out = reader
        .join()
        .map_err(|_| "ts71 driver reader panicked")?
        .map_err(|error| error.to_string())?;
    let errors = errors.join().unwrap_or_default();
    if !status.success() {
        let last = errors.lines().rev().find(|line| !line.trim().is_empty()).unwrap_or("no stderr");
        return Err(format!("ts71 driver exited {status}: {last}"));
    }
    let response: Response =
        serde_json::from_slice(&out).map_err(|error| format!("ts71 driver output: {error}"))?;
    for fact in &response.tsi {
        crate::read::tsi::registry::check(&fact.relation, &fact.args)?;
    }
    let elapsed = load.elapsed();
    Ok(CheckerAnswers {
        version: response.version,
        calls: response.calls.into_iter().map(|(path, rows)| (path, references(rows))).collect(),
        types: response.types.into_iter().map(|(path, rows)| (path, references(rows))).collect(),
        tsi: response.tsi,
        coverage: response.coverage,
        load: elapsed,
        walk: Duration::ZERO,
        files_answered: response.files_answered,
    })
}
