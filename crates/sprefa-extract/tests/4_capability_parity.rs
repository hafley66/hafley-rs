//! CAPABILITY PARITY: whatever the library can do, the binary can reach.
//!
//! WHY THIS TEST EXISTS. The CLI silently fell behind the library and nothing
//! caught it: `Resolve<TypeF>` and the whole SCIP path were library-reachable
//! and binary-unreachable for the entire life of `--resolve`, because the
//! project recipe lived in the binary's own private adapter and only ever
//! dispatched the `CallF` arm with `reader: None`. No test compared the two
//! surfaces, so the gap was invisible until someone read both files side by
//! side. The spelunk (`plans/2026-07-30-sprefa-extract-spelunk.md`) recorded
//! that the assertion "was unwritable because the bin never claimed phase 2".
//! It claims it now, so this is the assertion.
//!
//! TWO LEGS, TWO ENFORCEMENT MECHANISMS. Neither is a list someone has to
//! remember to update.
//!
//!  1. ROSTER LEG, enforced at RUNTIME off `sprefa_extract::sources()`. For
//!     every registered `Source`, the binary's default output over a fixture
//!     must equal the library's own `flatten` over the same bytes. Adding a
//!     language to the roster makes this test demand a fixture for it, so a
//!     new `Source` cannot land binary-unreachable.
//!
//!  2. CAPABILITY LEG, enforced by the COMPILER. `LibraryCapability` is an
//!     enum and `reach_of` matches on it exhaustively, so adding a library
//!     capability does not compile until someone states how the binary reaches
//!     it, or marks it `LibraryOnly` with a written reason. `ALL` is checked
//!     against a declared count so a variant cannot be added and left out of
//!     the run either.
//!
//! FOREIGN INDEXERS ARE NOT GATED. The SCIP legs invoke the real
//! scip-typescript, exactly like the existing ratchets in `golden_parity.rs`
//! ("the ratchet never fakes green"). A missing indexer fails this test loudly
//! rather than skipping to a green that means nothing.
//!
//! SABOTAGE RECEIPTS (all three run, all three red, then reverted). The first
//! two re-create the exact drift this test exists to catch:
//!  - deleting the `arms.types` dispatch from `project::resolve_project`
//!    -> "ResolveType: the binary reached no resolved_type_edge record".
//!  - putting back `reader: None` in the resolve context, the state the binary
//!    shipped in before this lane
//!    -> "ScipIndexLoad: the binary reached no resolved_edge record".
//!  - deleting the prolog row from `ROSTER_FIXTURES`
//!    -> "these Sources are in the roster with no capability-parity fixture:
//!    [\"prolog\"]".
//!  - deleting the `--package-deps` dispatch from the binary's main
//!    -> "PackageEdges: the binary reached no package_edge record".

use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sprefa_extract::{
    dispatch, flatten_jsonl, rehomes, renames, sources, CheckerTier, FamilyMask, CHECKER_TIERS,
    INDEXERS, RESOLVE_ARMS,
};

const BIN: &str = env!("CARGO_BIN_EXE_ryii");

// ════════════════════════════════════════════════════════════════════════════
// LEG 1: the roster. Runtime-enumerated, so a new language cannot skip it.
// ════════════════════════════════════════════════════════════════════════════

/// One fixture per registered `Source`, keyed by `Source::name()`. This table is
/// checked against the live roster in both directions below: a roster entry with
/// no row here fails, and a row here naming no roster entry fails.
const ROSTER_FIXTURES: &[(&str, &str)] = &[
    ("ts", "tests/fixtures/ts/sample.ts"),
    ("rust", "tests/fixtures/rust/sample.rs"),
    ("go", "tests/fixtures/go/sample.go"),
    ("kotlin", "tests/fixtures/kotlin/sample.kt"),
    ("prolog", "tests/fixtures/prolog/0_sample.pl"),
    ("python", "tests/fixtures/python/sample.py"),
    ("markdown", "tests/fixtures/markdown/0_sample.md"),
    ("gdscript", "tests/fixtures/gdscript/sample.gd"),
    ("commonlisp", "tests/fixtures/commonlisp/sample.lisp"),
    ("data", "tests/fixtures/data/nested.json"),
    ("fallback", "tests/fixtures/astgrep/sample.html"),
];

/// Every `Source` in the live roster produces the same facts through the binary
/// as through the library. This is the phase-1 half of parity, and it covers
/// every family a language emits at once: the comparison is the whole flattened
/// stream, not a sampled record kind. The families are named explicitly
/// because the no-flag default is `FamilyMask::DEFAULT` (types.rs), not ALL:
/// parity needs the same mask on both sides, and `cst,type,call,df,data` is
/// `FamilyMask::ALL` spelled out.
#[test]
fn every_roster_source_is_reachable_through_the_binary() {
    let roster: Vec<&'static str> = sources().iter().map(|source| source.name()).collect();

    let uncovered: Vec<&str> = roster
        .iter()
        .copied()
        .filter(|name| !ROSTER_FIXTURES.iter().any(|(key, _)| key == name))
        .collect();
    assert!(
        uncovered.is_empty(),
        "these Sources are in the roster with no capability-parity fixture: {uncovered:?}. \
         A new language must bring a fixture here, or nothing proves the binary reaches it."
    );
    let stale: Vec<&str> = ROSTER_FIXTURES
        .iter()
        .map(|(key, _)| *key)
        .filter(|key| !roster.contains(key))
        .collect();
    assert!(
        stale.is_empty(),
        "ROSTER_FIXTURES names Sources the roster no longer has: {stale:?}"
    );

    for (name, fixture) in ROSTER_FIXTURES {
        let content = std::fs::read(fixture).unwrap_or_else(|err| {
            panic!("capability-parity fixture for {name} is missing: {fixture}: {err}")
        });
        let output = dispatch(fixture, &content, FamilyMask::ALL)
            .unwrap_or_else(|| panic!("{fixture} routes to no Source, so it cannot cover {name}"));
        let source = sources()
            .iter()
            .find(|source| source.name() == *name)
            .expect("fixture source remains in the roster");
        let declared = source.planes();
        for (plane, enabled, mask) in [
            (
                "cst",
                declared.cst,
                FamilyMask {
                    cst: true,
                    ..FamilyMask::NONE
                },
            ),
            (
                "types",
                declared.types,
                FamilyMask {
                    types: true,
                    ..FamilyMask::NONE
                },
            ),
            (
                "call",
                declared.call,
                FamilyMask {
                    call: true,
                    ..FamilyMask::NONE
                },
            ),
            (
                "df",
                declared.df,
                FamilyMask {
                    df: true,
                    ..FamilyMask::NONE
                },
            ),
            (
                "data",
                declared.data,
                FamilyMask {
                    data: true,
                    ..FamilyMask::NONE
                },
            ),
        ] {
            let projected = dispatch(fixture, &content, mask).expect("fixture routes to a Source");
            let present = match plane {
                "cst" => projected.cst.is_some(),
                "types" => projected.types.is_some(),
                "call" => projected.call.is_some(),
                "df" => projected.df.is_some(),
                "data" => projected.data.is_some(),
                _ => unreachable!(),
            };
            assert_eq!(present, enabled, "{name} {plane} plane");
        }
        let from_library = flatten_jsonl(&output);
        assert!(
            !from_library.is_empty(),
            "{name}'s fixture {fixture} produces no facts, so it proves nothing"
        );

        let mut from_binary = run(&["--kinds", "cst,type,call,df,data", fixture]);
        from_binary.sort();
        assert_eq!(
            from_binary, from_library,
            "{name}: the binary's stream and the library's flatten disagree over {fixture}"
        );
    }
}

fn matrix_help_table(rows: &[Value]) -> String {
    let mut output = String::from(
        "LANGUAGE     PLANES                 RESOLVE  REHOME                 RENAME  CHECKER         SCIP\n",
    );
    for row in rows {
        let language = row["language"].as_str().unwrap_or("");
        let planes = row["planes"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(",");
        let resolve = format!(
            "{}{}",
            row["resolve"]["call"]
                .as_bool()
                .unwrap_or(false)
                .then_some("call")
                .unwrap_or(""),
            row["resolve"]["types"]
                .as_bool()
                .unwrap_or(false)
                .then_some("+types")
                .unwrap_or("")
        );
        let rehome = if row["rehome"].is_null() {
            String::new()
        } else {
            ["manifests", "shim", "text_spellings", "plan_check"]
                .into_iter()
                .filter(|key| row["rehome"][key].as_bool() == Some(true))
                .collect::<Vec<_>>()
                .join(",")
        };
        let checker = row["checker"].as_str().unwrap_or("-");
        let scip = row["scip_indexer"].as_str().unwrap_or("-");
        let rename = if row["rename"].as_bool().unwrap_or(false) {
            "yes"
        } else {
            "no"
        };
        output.push_str(&format!(
            "{language:<12} {planes:<22} {resolve:<8} {rehome:<22} {rename:<7} {checker:<15} {scip}\n"
        ));
    }
    output
}

#[test]
fn capabilities_matrix_matches_every_roster_and_help_table() {
    let rows: Vec<Value> = run(&["capabilities"])
        .iter()
        .map(|line| serde_json::from_str(line).expect("capability JSONL row"))
        .collect();
    let source_names: Vec<&str> = rows
        .iter()
        .filter(|row| row["source"] == true)
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    assert_eq!(
        source_names,
        sources()
            .iter()
            .map(|source| source.name())
            .collect::<Vec<_>>()
    );

    for source in sources() {
        let row = rows
            .iter()
            .find(|row| row["language"] == source.name())
            .expect("source has matrix row");
        let planes = source.planes();
        for (name, present) in [
            ("cst", planes.cst),
            ("types", planes.types),
            ("call", planes.call),
            ("df", planes.df),
            ("data", planes.data),
        ] {
            assert_eq!(
                row["planes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|plane| plane == name),
                present,
                "{} plane {name}",
                source.name()
            );
        }
        let resolve = RESOLVE_ARMS.iter().find(|arm| arm.name == source.name());
        assert_eq!(row["resolve"]["declared"], resolve.is_some());
        assert_eq!(
            row["resolve"]["call"],
            resolve.is_some_and(|arm| arm.call.is_some())
        );
        assert_eq!(
            row["resolve"]["types"],
            resolve.is_some_and(|arm| arm.types.is_some())
        );
        let rehome = rehomes().iter().find(|arm| arm.name() == source.name());
        assert_eq!(row["rehome"].is_null(), rehome.is_none());
        if let Some(rehome) = rehome {
            assert_eq!(row["rehome"]["core"], true);
            assert_eq!(row["rehome"]["manifests"], rehome.manifests.is_some());
            assert_eq!(row["rehome"]["shim"], rehome.shim.is_some());
            assert_eq!(
                row["rehome"]["text_spellings"],
                rehome.text_spellings.is_some()
            );
            assert_eq!(row["rehome"]["plan_check"], rehome.plan_check.is_some());
        }
        assert_eq!(
            row["rename"],
            renames().iter().any(|arm| arm.name() == source.name())
        );
        let checker: Option<&CheckerTier> = CHECKER_TIERS
            .iter()
            .find(|tier| tier.language == source.name());
        assert_eq!(
            row["checker"],
            checker
                .map(|tier| Value::from(tier.tool))
                .unwrap_or(Value::Null)
        );
        let indexer_name = match source.name() {
            "ts" => "typescript",
            "kotlin" => "kotlin/java",
            other => other,
        };
        let indexer = INDEXERS.iter().find(|indexer| indexer.lang == indexer_name);
        assert_eq!(
            row["scip_indexer"],
            indexer
                .map(|row| Value::from(row.lang))
                .unwrap_or(Value::Null)
        );
    }

    let mut declared_resolve: Vec<&str> = RESOLVE_ARMS.iter().map(|arm| arm.name).collect();
    let mut matrix_resolve: Vec<&str> = rows
        .iter()
        .filter(|row| row["resolve"]["declared"] == true)
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    declared_resolve.sort_unstable();
    matrix_resolve.sort_unstable();
    assert_eq!(matrix_resolve, declared_resolve);

    let mut declared_rehome: Vec<&str> = rehomes().iter().map(|arm| arm.name()).collect();
    let mut matrix_rehome: Vec<&str> = rows
        .iter()
        .filter(|row| !row["rehome"].is_null())
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    declared_rehome.sort_unstable();
    matrix_rehome.sort_unstable();
    assert_eq!(matrix_rehome, declared_rehome);

    let mut declared_rename: Vec<&str> = renames().iter().map(|arm| arm.name()).collect();
    let mut matrix_rename: Vec<&str> = rows
        .iter()
        .filter(|row| row["rename"] == true)
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    declared_rename.sort_unstable();
    matrix_rename.sort_unstable();
    assert_eq!(matrix_rename, declared_rename);

    let mut declared_checkers: Vec<&str> = CHECKER_TIERS.iter().map(|tier| tier.language).collect();
    let mut matrix_checkers: Vec<&str> = rows
        .iter()
        .filter(|row| !row["checker"].is_null())
        .map(|row| row["language"].as_str().unwrap())
        .collect();
    declared_checkers.sort_unstable();
    matrix_checkers.sort_unstable();
    assert_eq!(matrix_checkers, declared_checkers);

    let mut declared_indexers: Vec<&str> = INDEXERS.iter().map(|indexer| indexer.lang).collect();
    let mut matrix_indexers: Vec<&str> = rows
        .iter()
        .filter(|row| !row["scip_indexer"].is_null())
        .map(|row| row["scip_indexer"].as_str().unwrap())
        .collect();
    declared_indexers.sort_unstable();
    matrix_indexers.sort_unstable();
    assert_eq!(matrix_indexers, declared_indexers);

    let help = Command::new(env!("CARGO_BIN_EXE_ryii"))
        .arg("--help")
        .output()
        .expect("ryii help runs");
    assert!(help.status.success());
    let help = String::from_utf8(help.stdout).unwrap();
    assert!(help.contains(&matrix_help_table(&rows)));
}

// ════════════════════════════════════════════════════════════════════════════
// LEG 2: named capabilities. Compiler-enumerated.
// ════════════════════════════════════════════════════════════════════════════

/// Every fact-producing capability the library exposes. Adding a variant here
/// does not compile until `reach_of` states how the binary reaches it.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum LibraryCapability {
    /// `flatten` / `flatten_jsonl` over a dispatched file.
    Phase1Flatten,
    /// `FamilyMask` selection of a family subset.
    Phase1FamilyMask,
    /// `Resolve<CallF>`: resolved caller-to-callee edges.
    ResolveCall,
    /// `Resolve<TypeF>`: resolved type reference edges.
    ResolveType,
    /// `ScipSource::load` plus a reader placed in `ProjectCx`, which is what
    /// turns on the resolve arms' SCIP leg.
    ScipIndexLoad,
    /// `ScipSource::build`: run the language's own indexer.
    ScipIndexBuild,
    /// `wire::SCHEMA`: the JSONL contract text.
    WireSchema,
    /// `wire::flatten_project_type`: the span-addressed project-edge wire.
    FlattenProjectType,
    /// `wire::flatten_scip` / `project::scip_facts`: the loaded SCIP index as
    /// raw occurrence, symbol and relationship rows.
    ScipFacts,
    /// `wire::file_fact`: one file's digest, byte count and line count.
    FileFact,
    /// `wire::scip_file_edges`: the module graph folded out of a SCIP index.
    ScipFileEdges,
    /// `deps::fold_edges`: the same module graph resolved syntactically.
    DietFileEdges,
    /// `deps::fold_unresolved`: the specifiers that resolved to nothing.
    DietFileUnresolved,
    /// `manifests::fold_package_edges`: workspace-internal manifest edges.
    PackageEdges,
    /// `slow::slow_project`: a SCIP index projected onto fast's tables.
    SlowProject,
}

/// Kept in step with the enum by the length assertion in the test below.
const ALL: &[LibraryCapability] = &[
    LibraryCapability::Phase1Flatten,
    LibraryCapability::Phase1FamilyMask,
    LibraryCapability::ResolveCall,
    LibraryCapability::ResolveType,
    LibraryCapability::ScipIndexLoad,
    LibraryCapability::ScipIndexBuild,
    LibraryCapability::WireSchema,
    LibraryCapability::FlattenProjectType,
    LibraryCapability::ScipFacts,
    LibraryCapability::FileFact,
    LibraryCapability::ScipFileEdges,
    LibraryCapability::DietFileEdges,
    LibraryCapability::DietFileUnresolved,
    LibraryCapability::PackageEdges,
    LibraryCapability::SlowProject,
];
const DECLARED_CAPABILITIES: usize = 15;

/// How the binary reaches one library capability.
enum CliReach {
    /// Run the binary with these arguments and require at least one line
    /// carrying this `record` tag. `absent_without` names an argument list that
    /// must NOT produce that tag, so the flag is proven to be what caused it
    /// rather than something the binary already did.
    Emits {
        args: Vec<String>,
        record: &'static str,
        /// Narrows the witness past the record tag, for a record a second
        /// capability also produces. Matched as a substring of the same line.
        field: Option<&'static str>,
        absent_without: Option<Vec<String>>,
    },
    /// Run the binary with these arguments and require this text in stdout.
    Prints {
        args: Vec<String>,
        contains: &'static str,
    },
    /// Deliberately not on the CLI, with the reason and the surface that does
    /// carry the same information. Adding one of these is a decision that shows
    /// up in the diff; it is not a way to make a gap quiet.
    LibraryOnly { reason: &'static str },
}

fn ts_scip_root() -> PathBuf {
    PathBuf::from("tests/fixtures/ts")
}

fn ts_scip_files() -> Vec<String> {
    let mut files: Vec<String> = std::fs::read_dir(ts_scip_root().join("scip"))
        .expect("the ts scip fixture trio")
        .flatten()
        .map(|entry| entry.path().to_string_lossy().to_string())
        .filter(|path| path.ends_with(".ts"))
        .collect();
    files.sort();
    files
}

fn strings(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|part| part.to_string()).collect()
}

/// THE EXHAUSTIVE MATCH. This is the whole enforcement mechanism of leg 2.
fn reach_of(capability: LibraryCapability, scip_index: &Path) -> CliReach {
    use LibraryCapability::*;
    match capability {
        Phase1Flatten => CliReach::Emits {
            args: strings(&["tests/fixtures/ts/sample.ts"]),
            field: None,
            record: "node",
            absent_without: None,
        },
        Phase1FamilyMask => CliReach::Emits {
            args: strings(&["--kinds", "df", "tests/fixtures/ts/sample.ts"]),
            field: None,
            record: "param",
            // Without the mask the default is every family, so `param` rows
            // appear anyway; the mask's effect is proven by the roster leg's
            // whole-stream equality and by 2_df_aux_cli.rs.
            absent_without: None,
        },
        ResolveCall => CliReach::Emits {
            args: strings(&[
                "--resolve",
                "tests/fixtures/resolve/0_caller.ts",
                "tests/fixtures/resolve/1_callee.ts",
            ]),
            field: None,
            record: "resolved_edge",
            absent_without: Some(strings(&["tests/fixtures/resolve/0_caller.ts"])),
        },
        ResolveType => CliReach::Emits {
            args: strings(&[
                "--resolve",
                "--arms",
                "type",
                "tests/fixtures/ts/sample.ts",
                "tests/fixtures/ts/consts.ts",
            ]),
            field: None,
            record: "resolved_type_edge",
            // The same paths under the default arm emit call edges only, so the
            // `type` arm is proven to be what produced these rows.
            absent_without: Some(strings(&[
                "--resolve",
                "tests/fixtures/ts/sample.ts",
                "tests/fixtures/ts/consts.ts",
            ])),
        },
        ScipIndexLoad => {
            let mut args = strings(&[
                "--resolve",
                "--root",
                &ts_scip_root().to_string_lossy(),
                "--scip-index",
                &scip_index.to_string_lossy(),
            ]);
            args.extend(ts_scip_files());
            CliReach::Emits {
                args,
                // A scip_override row exists ONLY because the indexer's answer
                // displaced the name match, so it is proof the index was really
                // consumed. The record tag alone is not: the module plane binds
                // gamma.ts's import without any indexer.
                field: Some("\"kind\":\"scip_override\""),
                record: "resolved_edge",
                absent_without: {
                    let mut bare = strings(&["--resolve"]);
                    bare.extend(ts_scip_files());
                    Some(bare)
                },
            }
        }
        ScipIndexBuild => {
            let mut args = strings(&[
                "--resolve",
                "--root",
                &ts_scip_root().to_string_lossy(),
                "--scip-build",
            ]);
            args.extend(ts_scip_files());
            CliReach::Emits {
                args,
                field: Some("\"kind\":\"scip_override\""),
                record: "resolved_edge",
                absent_without: {
                    let mut bare = strings(&["--resolve"]);
                    bare.extend(ts_scip_files());
                    Some(bare)
                },
            }
        }
        WireSchema => CliReach::Prints {
            args: strings(&["schema"]),
            contains: "record=resolved_type_edge",
        },
        // Built over its own root rather than the shared ts index: the
        // relationship rows this reach proves only exist in the implements
        // fixture, and a reach that could pass on an index without them would
        // not be proving retention.
        ScipFacts => CliReach::Emits {
            args: strings(&[
                "scip",
                "--raw",
                "--root",
                "tests/fixtures/scip_rel",
                "--scip-build",
                "tests/fixtures/scip_rel/animal.ts",
            ]),
            field: None,
            record: "scip_relationship",
            absent_without: Some(strings(&["tests/fixtures/scip_rel/animal.ts"])),
        },
        SlowProject => CliReach::Emits {
            args: strings(&[
                "slow",
                "tests/fixtures/ratchet_soopy/src",
                "--root",
                "tests/fixtures/ratchet_soopy",
                "--scip-index",
                "tests/fixtures/ratchet_soopy/index.scip",
                "--no-checker",
            ]),
            field: Some("\"resolution_origin\":\"scip\""),
            record: "resolved_edge",
            absent_without: Some(strings(&["fast", "tests/fixtures/ratchet_soopy/src"])),
        },
        FileFact => CliReach::Emits {
            args: strings(&["--file-fact", "tests/fixtures/ts/sample.ts"]),
            field: None,
            record: "file",
            absent_without: Some(strings(&["tests/fixtures/ts/sample.ts"])),
        },
        ScipFileEdges => CliReach::Emits {
            args: strings(&[
                "--scip-deps",
                "--root",
                "tests/fixtures/ts",
                "--scip-build",
                "tests/fixtures/ts/scip/alpha.ts",
            ]),
            field: None,
            record: "file_edge",
            absent_without: Some(strings(&["tests/fixtures/ts/scip/gamma.ts"])),
        },
        DietFileEdges => CliReach::Emits {
            args: strings(&[
                "--deps",
                "--root",
                "tests/fixtures/deps",
                "tests/fixtures/deps/app.ts",
                "tests/fixtures/deps/lib/util.ts",
            ]),
            field: None,
            record: "file_edge",
            absent_without: Some(strings(&["tests/fixtures/deps/app.ts"])),
        },
        DietFileUnresolved => CliReach::Emits {
            args: strings(&[
                "--deps",
                "--root",
                "tests/fixtures/deps",
                "tests/fixtures/deps/app.ts",
            ]),
            field: None,
            record: "file_unresolved",
            absent_without: Some(strings(&["tests/fixtures/deps/app.ts"])),
        },
        PackageEdges => CliReach::Emits {
            args: strings(&[
                "--package-deps",
                "--root",
                "tests/fixtures/packages",
                "tests/fixtures/packages/crates/alpha/Cargo.toml",
                "tests/fixtures/packages/crates/beta/Cargo.toml",
            ]),
            field: None,
            record: "package_edge",
            absent_without: Some(strings(&[
                "--deps",
                "--root",
                "tests/fixtures/deps",
                "tests/fixtures/deps/app.ts",
            ])),
        },
        FlattenProjectType => CliReach::LibraryOnly {
            reason: "the span-and-blob project-edge shape predates the flat-fields \
                     rule for host decoding. The CLI carries the same edges as \
                     record=resolved_type_edge with paths and names, which a \
                     line-oriented consumer can decode without a span join; \
                     flatten_project_type stays for golden_parity's normalize.",
        },
    }
}

#[test]
fn every_library_capability_is_reachable_through_the_binary() {
    assert_eq!(
        ALL.len(),
        DECLARED_CAPABILITIES,
        "a LibraryCapability variant was added without adding it to ALL, so it \
         would never be exercised"
    );

    // One real scip-typescript index, shared by the two SCIP capabilities.
    // Built through the library seam, which is also the capability under test
    // for ScipIndexBuild's library half.
    let scip_index = build_ts_scip_index();

    for capability in ALL {
        match reach_of(*capability, &scip_index) {
            CliReach::Emits {
                args,
                record,
                field,
                absent_without,
            } => {
                let tag = format!("\"record\":\"{record}\"");
                let matches = |line: &String| {
                    line.contains(&tag) && field.map_or(true, |field| line.contains(field))
                };
                let lines = run_ok(&args, *capability);
                assert!(
                    lines.iter().any(matches),
                    "{capability:?}: the binary reached no {record} record with {args:?}"
                );
                if let Some(bare) = absent_without {
                    let without = run_ok(&bare, *capability);
                    assert!(
                        !without.iter().any(matches),
                        "{capability:?}: {record} records appear WITHOUT {args:?} too, so \
                         this reach proves nothing about the capability"
                    );
                }
            }
            CliReach::Prints { args, contains } => {
                let stdout = run_ok(&args, *capability).join("\n");
                assert!(
                    stdout.contains(contains),
                    "{capability:?}: `extract {args:?}` printed no {contains:?}"
                );
            }
            CliReach::LibraryOnly { reason } => {
                assert!(
                    reason.len() > 40,
                    "{capability:?}: a LibraryOnly capability needs a real written \
                     reason naming what carries the same information"
                );
            }
        }
    }
}

/// The library-side SCIP build. Loud on a missing indexer, never skipped: the
/// same law the golden_parity ratchets run under.
fn build_ts_scip_index() -> PathBuf {
    use sprefa_extract::{ScipSource, ScipTypescript};
    ScipTypescript.build(&ts_scip_root()).expect(
        "scip-typescript build failed. This test does not gate on the indexer being \
         installed: a skipped SCIP leg is a green that means nothing.",
    )
}

fn run(args: &[&str]) -> Vec<String> {
    let owned: Vec<String> = args.iter().map(|arg| arg.to_string()).collect();
    run_ok(&owned, LibraryCapability::Phase1Flatten)
}

fn run_ok(args: &[String], capability: LibraryCapability) -> Vec<String> {
    let output = Command::new(BIN)
        .args(args)
        .output()
        .expect("extract binary runs");
    assert!(
        output.status.success(),
        "{capability:?}: `extract {args:?}` exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("stdout is UTF-8")
        .lines()
        .map(str::to_string)
        .collect()
}
