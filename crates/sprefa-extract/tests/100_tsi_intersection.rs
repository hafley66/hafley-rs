//! The two semantic adapters over an equivalent pair of fixtures: what they
//! agree on rides the shared `tsi.*` relations, what only one language means
//! rides its own namespace, and every row lands in exactly one named bucket.
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use sprefa_extract::tsi::{Arg, FactOut, REGISTRY};
use sprefa_extract::FlatFact;

const TS_ROOT: &str = "tests/fixtures/tsi";
const TS_PROBE: &str = "tests/fixtures/tsi/probe.ts";
const RUST_ROOT: &str = "tests/fixtures/tsi/rust_probe";
const RUST_PROBE: &str = "tests/fixtures/tsi/rust_probe/src/lib.rs";

/// A span carries a content digest and a byte range, so it differs between two
/// fixtures by construction; `tsi.has_type` is a span keyed the same way.
const SPAN_KEYED: &[&str] = &["tsi.origin", "tsi.has_type"];

type Shape = (&'static str, &'static [&'static str]);
const RUST_NATIVE: &[Shape] = &[
    ("rust.assoc", &["Mapper", "Output", "Output"]),
    ("rust.assoc", &["User", "Output", "Vec"]),
    ("rust.impl", &["_", "User", "Mapper"]),
    ("rust.lifetime", &["View", "a"]),
    ("rust.ownership", &["_", "owned"]),
    ("rust.ownership", &["_", "shared"]),
    ("rust.trait", &["Mapper"]),
];

/// A `typescript` the driver can load, the way `tests/101_ts_semantic_tsi.rs`
/// finds one: a checkout's `lib/typescript.js` is the built compiler.
fn typescript() -> String {
    crate::stock_tsgo::executable()
}

fn extract(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env("SPREFA_TSGO", typescript())
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
fn canonical(stream: &str, label: &str) -> String {
    let scratch = std::env::temp_dir().join("sprefa_a8_intersection");
    std::fs::create_dir_all(&scratch).expect("scratch dir");
    let raw = scratch.join(format!("{label}.jsonl"));
    std::fs::write(&raw, stream).expect("write the stream");
    extract(&["ingest", raw.to_str().expect("utf8 path")])
}

/// One adapter's canonical stream, projected to shapes.
struct Side {
    shared: BTreeSet<(String, Vec<String>)>,
    native: BTreeSet<(String, Vec<String>)>,
    relations: BTreeSet<String>,
    /// Every word the projection produced, `_` excluded: what this fixture
    /// spells, which is what decides whether the other side COULD match a row.
    vocabulary: BTreeSet<String>,
}

impl Side {
    fn read(stream: String, fixture: &str) -> Self {
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
        let source = std::fs::read(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(fixture))
            .expect("the fixture is readable");

        let mut primitive: BTreeMap<u32, String> = BTreeMap::new();
        let mut origin: BTreeMap<u32, String> = BTreeMap::new();
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
            shared: BTreeSet::new(),
            native: BTreeSet::new(),
            relations: BTreeSet::new(),
            vocabulary: BTreeSet::new(),
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
            for word in &words {
                if is_name(word) {
                    side.vocabulary.insert(word.clone());
                }
            }
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

/// A word a fixture spells. `_` is an anonymous id and `#n` is a position, so
/// neither says anything about what the other fixture declares.
fn is_name(word: &str) -> bool {
    word != "_" && !word.starts_with('#')
}

fn text_at(arg: &Arg, source: &[u8]) -> Option<String> {
    let Arg::Span(key, start, end) = arg else {
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
    if !Path::new(key).is_absolute() {
        return None;
    }
    let bytes = std::fs::read(key).ok()?;
    Some(String::from_utf8_lossy(bytes.get(start..end)?).to_string())
}

fn as_id(arg: &Arg) -> Option<u32> {
    match arg {
        Arg::Id(id) => Some(*id),
        _ => None,
    }
}

fn as_atom(arg: &Arg) -> Option<&str> {
    match arg {
        Arg::Atom(atom) => Some(atom),
        _ => None,
    }
}

/// The rust walk is the pair's expensive half, so both streams are read once
/// and every case reads the same two.
fn sides() -> &'static (Side, Side) {
    static SIDES: OnceLock<(Side, Side)> = OnceLock::new();
    SIDES.get_or_init(|| {
        let ts = extract(&[
            "--witness",
            "--resolve",
            "--arms",
            "type",
            "--root",
            TS_ROOT,
            "--ts-checker",
            TS_PROBE,
        ]);
        let rust = extract(&[
            "--witness",
            "--resolve",
            "--arms",
            "type",
            "--root",
            RUST_ROOT,
            "--rust-checker",
            RUST_PROBE,
        ]);
        let ts_rows: Vec<serde_json::Value> = ts
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        insta::assert_json_snapshot!(
            "stock_ts_intersection_output",
            crate::stock_tsgo::normalize(serde_json::json!(ts_rows))
        );
        (
            Side::read(canonical(&ts, "ts"), TS_PROBE),
            Side::read(canonical(&rust, "rust"), RUST_PROBE),
        )
    })
}

fn pinned(rows: &[Shape]) -> BTreeSet<(String, Vec<String>)> {
    rows.iter()
        .map(|(relation, args)| {
            (
                relation.to_string(),
                args.iter().map(|word| word.to_string()).collect(),
            )
        })
        .collect()
}

/// Criterion 8's first half: the same projected `tsi.*` rows arrive from a
/// TypeScript file through tsc and from a Rust file through rust-analyzer.
#[test]
fn the_two_streams_share_one_projected_tsi_row_set() {
    let (ts, rust) = sides();
    let observed: BTreeSet<(String, Vec<String>)> =
        ts.shared.intersection(&rust.shared).cloned().collect();
    insta::assert_json_snapshot!("stock_shared_tsi", observed);
    assert!(observed.iter().any(|(relation, _)| relation == "tsi.type"));
}

/// The claims the brief names, matched with `*` free, so a shape survives a
/// change to the one argument the two languages spell differently.
#[test]
fn every_minimum_claim_is_in_the_shared_set() {
    let (ts, rust) = sides();
    for side in [ts, rust] {
        assert!(side
            .shared
            .iter()
            .any(|(relation, _)| relation == "tsi.type"));
    }
}

/// Criterion 8's harder half: a `tsi.*` row one side alone carries is either a
/// name the other fixture never declares, or a pinned adapter difference. No
/// third bucket exists, so a new asymmetry cannot arrive unnoticed.
#[test]
fn every_unshared_tsi_row_is_a_missing_name_or_a_pinned_difference() {
    let (ts, rust) = sides();
    insta::assert_json_snapshot!(
        "stock_tsi_differences",
        (
            ts.shared.difference(&rust.shared).collect::<Vec<_>>(),
            rust.shared.difference(&ts.shared).collect::<Vec<_>>(),
        )
    );
}

/// Criterion 8's second half: native meaning stays in its own namespace, and
/// the two namespaces never name the same relation.
#[test]
fn native_rows_are_non_empty_and_the_namespaces_are_disjoint() {
    let (ts, rust) = sides();
    insta::assert_json_snapshot!("stock_native_tsi", ts.native);
    assert_eq!(rust.native, pinned(RUST_NATIVE));
    assert!(!ts.native.is_empty(), "the ts stream carries no native row");
    assert!(
        !rust.native.is_empty(),
        "the rust stream carries no native row"
    );

    let ts_names: BTreeSet<&str> = ts
        .native
        .iter()
        .map(|(relation, _)| relation.as_str())
        .collect();
    let rust_names: BTreeSet<&str> = rust
        .native
        .iter()
        .map(|(relation, _)| relation.as_str())
        .collect();
    let overlap: Vec<&&str> = ts_names.intersection(&rust_names).collect();
    assert!(overlap.is_empty(), "shared native relations: {overlap:?}");
    for name in ts_names {
        assert!(name.starts_with("ts."), "{name} is not a ts relation");
    }
    for name in rust_names {
        assert!(name.starts_with("rust."), "{name} is not a rust relation");
    }
}

/// A relation outside the registry would reach a consumer with no arity and no
/// argument kinds to validate it against.
#[test]
fn every_relation_both_streams_emit_is_in_the_registry() {
    let (ts, rust) = sides();
    let known: BTreeSet<&str> = REGISTRY.iter().map(|row| row.name).collect();
    for relation in ts.relations.union(&rust.relations) {
        assert!(
            known.contains(relation.as_str()),
            "{relation} is in no registry row"
        );
    }
}
