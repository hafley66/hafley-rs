//! The two semantic adapters over an equivalent pair of fixtures: what they
//! agree on rides the shared `tsi.*` relations, what only one language means
//! rides its own namespace, and every row lands in exactly one named bucket.
//! Five tests folded into `tests/fixtures/tsi_intersection_cases/`; the api
//! step projects both canonical streams to the named-row buckets the originals
//! asserted, and the whole projection freezes in the snapshot. The raw streams
//! stay unfrozen by the same choice the originals made: the rust side names
//! std types through absolute rustup paths, while every projected word is
//! fixture-spelled text.
//!
//! SABOTAGE RECEIPT (fail-pre-fix, base sha 6e5b16a08, measured by deleting
//! every `record=fact` row whose relation starts with `tsi.` from the ts
//! stream before projecting): 4 of the 5 cases fail. The shared set drops from
//! 54 rows to 0, `the_two_streams_share_one_projected_tsi_row_set` prints all
//! 13 SHARED rows as missing, and
//! `every_unshared_tsi_row_is_a_missing_name_or_a_pinned_difference` prints
//! all 10 TS_ASYMMETRIC rows as missing. The 4 `ts.*` rows survive the cut and
//! still fail their own case, reading `ts.interface(_)` and
//! `ts.mapped(_, _, _, _)`: `tsi.origin` is the naming table, so a native row
//! without the shared rows beside it carries no name.
#![cfg(all(feature = "ts-checker", feature = "rust-checker"))]

#[test]
fn whole_output() {
    crate::fixture_runner::run("tsi_intersection_cases", |case| {
        crate::fixture_runner::commands(case, |step| {
            assert_eq!(step["api"], "projection", "unknown api step: {step}");
            projection(step)
        })
    });
}

const TS_ROOT: &str = "tests/fixtures/tsi";
const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RUST_ROOT: &str = "tests/fixtures/tsi/rust_probe";
const RUST_PROBE: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";

/// A span carries a content digest and a byte range, so it differs between two
/// fixtures by construction; `tsi.has_type` is a span keyed the same way.
const SPAN_KEYED: &[&str] = &["tsi.origin", "tsi.has_type"];

type Shape = (&'static str, &'static [&'static str]);

/// One adapter's canonical stream, projected to shapes.
struct Side {
    shared: std::collections::BTreeSet<(String, Vec<String>)>,
    native: std::collections::BTreeSet<(String, Vec<String>)>,
    relations: std::collections::BTreeSet<String>,
}

impl Side {
    fn read(stream: String, fixture: &str) -> Self {
        use sprefa_extract::tsi::{Arg, FactOut};
        use sprefa_extract::FlatFact;
        let facts: Vec<FactOut> = stream
            .lines()
            .filter(|line| !line.trim().is_empty())
            .map(|line| {
                serde_json::from_str::<FlatFact>(line)
                    .unwrap_or_else(|error| panic!("line does not decode: {line}\n{error}"))
            })
            .filter_map(|row| match row {
                FlatFact::Fact(fact) => Some(fact),
                _ => None,
            })
            .collect();
        let source = std::fs::read(std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture))
            .expect("the fixture is readable");

        let mut primitive: std::collections::BTreeMap<u32, String> = Default::default();
        let mut origin: std::collections::BTreeMap<u32, String> = Default::default();
        for fact in &facts {
            match fact.relation.as_str() {
                "tsi.primitive" => {
                    if let (Some(id), Some(class)) = (as_id(&fact.args[0]), as_atom(&fact.args[1]))
                    {
                        primitive.insert(id, class.to_string());
                    }
                }
                "tsi.origin" => {
                    if let (Some(id), Some(text)) =
                        (as_id(&fact.args[0]), text_at(&fact.args[2], &source))
                    {
                        origin.insert(id, text);
                    }
                }
                _ => {}
            }
        }

        let mut side = Self {
            shared: Default::default(),
            native: Default::default(),
            relations: Default::default(),
        };
        for fact in &facts {
            side.relations.insert(fact.relation.clone());
            if SPAN_KEYED.contains(&fact.relation.as_str()) {
                continue;
            }
            let words: Vec<String> = fact
                .args
                .iter()
                .map(|arg| match arg {
                    // A primitive class names the leaf a type system owns; an
                    // origin range names what the fixture wrote; an id with
                    // neither is anonymous and compares only by position.
                    Arg::Id(id) => primitive
                        .get(id)
                        .or_else(|| origin.get(id))
                        .cloned()
                        .unwrap_or_else(|| "_".to_string()),
                    Arg::Text(text) => text.clone(),
                    Arg::Atom(atom) => atom.clone(),
                    Arg::Int(value) => format!("#{value}"),
                    Arg::Span(_, _, _) => "_".to_string(),
                })
                .collect();
            let row = (fact.relation.clone(), words);
            if fact.relation.starts_with("tsi.") {
                side.shared.insert(row);
            } else {
                side.native.insert(row);
            }
        }
        side
    }
}

fn text_at(arg: &sprefa_extract::tsi::Arg, source: &[u8]) -> Option<String> {
    let sprefa_extract::tsi::Arg::Span(key, start, end) = arg else {
        return None;
    };
    let (start, end) = (*start as usize, *end as usize);
    if end <= start {
        return None;
    }
    if key.starts_with("blake3:") {
        return Some(String::from_utf8_lossy(source.get(start..end)?).to_string());
    }
    // A declaration outside the supplied files keeps its own path where the
    // digest goes, which is how a std or lib type gets named.
    if !std::path::Path::new(key).is_absolute() {
        return None;
    }
    let bytes = std::fs::read(key).ok()?;
    Some(String::from_utf8_lossy(bytes.get(start..end)?).to_string())
}

fn as_id(arg: &sprefa_extract::tsi::Arg) -> Option<u32> {
    match arg {
        sprefa_extract::tsi::Arg::Id(id) => Some(*id),
        _ => None,
    }
}
fn as_atom(arg: &sprefa_extract::tsi::Arg) -> Option<&str> {
    match arg {
        sprefa_extract::tsi::Arg::Atom(atom) => Some(atom),
        _ => None,
    }
}

static SIDES: std::sync::LazyLock<(Side, Side)> = std::sync::LazyLock::new(|| {
    (
        Side::read(canonical(&extract(&[
            "--witness",
            "--resolve",
            "--arms",
            "type",
            "--root",
            TS_ROOT,
            "--ts-checker",
            TS_PROBE,
        ])), TS_PROBE),
        Side::read(canonical(&extract(&[
            "--witness",
            "--resolve",
            "--arms",
            "type",
            "--root",
            RUST_ROOT,
            "--rust-checker",
            RUST_PROBE,
        ])), RUST_PROBE),
    )
});

fn sides() -> &'static (Side, Side) {
    &SIDES
}

/// A `typescript` the driver can load, the way `tests/101_ts_semantic_tsi.rs`
/// finds one: a checkout's `lib/typescript.js` is the built compiler.
fn extract(args: &[&str]) -> String {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SPREFA_TSGO", crate::stock_tsgo::executable())
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{args:?} stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("stdout is UTF-8")
}

/// The reverse door renumbers ids and sorts, so both streams are compared in
/// the one canonical form a consumer would import.
fn canonical(stream: &str) -> String {
    let scratch = tempfile::tempdir().expect("scratch dir");
    let raw = scratch.path().join("stream.jsonl");
    std::fs::write(&raw, stream).expect("write the stream");
    extract(&["ingest", raw.to_str().expect("utf8 path")])
}

fn projection(step: &serde_json::Value) -> serde_json::Value {
    let (ts, rust) = sides();
    let mut rows: Vec<String> = Vec::new();
    let mut push = |kind: &str, row: &(String, Vec<String>)| {
        rows.push(format!(
            "{{\"kind\":\"{kind}\",\"relation\":\"{}\",\"args\":{}}}",
            row.0,
            serde_json::to_string(&row.1).unwrap(),
        ));
    };
    match step["bucket"].as_str().unwrap() {
        "shared" => for row in ts.shared.intersection(&rust.shared) { push("shared", row) },
        "ts_only" => for row in ts.shared.difference(&rust.shared) { push("ts_only", row) },
        "rust_only" => for row in rust.shared.difference(&ts.shared) { push("rust_only", row) },
        "ts_native" => for row in &ts.native { push("ts_native", row) },
        "rust_native" => for row in &rust.native { push("rust_native", row) },
        "relations" => {
            for name in ts.relations.union(&rust.relations) {
                rows.push(format!("{{\"kind\":\"relation\",\"name\":\"{name}\"}}"));
            }
        }
        other => panic!("unknown projection bucket: {other}"),
    }
    serde_json::Value::String(rows.join("\n"))
}
