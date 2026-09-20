//! `extract rename` on the Rust arm: the item ident, the `use` trailing segment,
//! the `ExprPath`/`TypePath` trailing segment, the glob stop, and this crate's
//! own tree renamed and handed to rustc.
//!
//! @comment-ok: fail-first receipt, repo law keeps these in TEST headers.
//! FAIL-FIRST, against the arc-3 binary (`RustSource` absent from `renames()`):
//!     rust_rename_matches_the_hand_written_after ... exited exit status: 2:
//!         no rename arm for src/util.rs (extract rename renames ts)
//!     glob_importer_is_a_dynamic_stop ... left: Some(2), right: Some(6)
//!     self_rename_is_judged_by_rustc ... exited exit status: 2:
//!         no rename arm for src/rename_cx.rs (extract rename renames ts)
//! FAIL-FIRST (root wins), against the arc-8 binary:
//!     shadowed_items_need_no_at ... exited 3: ambiguous Helper in src/util.rs
//!     renamed_fixture_crate_passes_cargo_check ... passes before: the check
//!         judges rustc's view of the after tree, which is what makes the diff
//!         assertion mean something

use std::path::{Path, PathBuf};
use std::process::Command;

use sprefa_extract::{ScipRust, ScipSource};

const ANCHOR: &str = "src/util.rs";

struct Fixture {
    root: PathBuf,
    state: PathBuf,
}

fn scratch(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "extract_rename_rust_{label}_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|since| since.as_nanos())
            .unwrap_or_default()
    ))
}

fn fixture(case: &str, label: &str) -> Fixture {
    let base = scratch(&format!("{case}_{label}"));
    let root = base.join("repo");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    copy_tree(&tree(case, "before"), &root);
    Fixture {
        root: root.canonicalize().expect("canonicalize fixture root"),
        state,
    }
}

fn tree(case: &str, side: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/rust_rename/{case}/{side}"))
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("fixture entry");
        let to = target.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy fixture file");
        }
    }
}

fn rename_verb(fixture: &Fixture, target: &str, new: &str, extra: &[&str]) -> String {
    let output = run_rename(&fixture.root, &fixture.state, target, new, extra);
    assert!(
        output.status.success(),
        "extract rename {extra:?} exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

fn run_rename(
    root: &Path,
    state: &Path,
    target: &str,
    new: &str,
    extra: &[&str],
) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ryi"))
        .arg("rename")
        .arg(target)
        .arg(new)
        .arg("--root")
        .arg(root)
        .arg("--state")
        .arg(state)
        .args(extra)
        .output()
        .expect("extract binary runs")
}

/// `diff -rq left right`, as the arc-5 receipt spells it. Returns the entries.
fn diff_rq(left: &Path, right: &Path) -> Vec<String> {
    let output = Command::new("diff")
        .arg("-rq")
        .arg(left)
        .arg(right)
        .output()
        .expect("diff runs");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_string)
        .collect()
}

/// The committed tree is the hand-written `after/` tree, byte for byte, which
/// pins what stays too: the `format!("Helper")` string, the `mod other` struct of
/// the same name, and the `H` alias's own body uses.
/// @comment-ok: the after/ tree is the assertion, so the case list lives here
#[test]
fn rust_rename_matches_the_hand_written_after() {
    let fixture = fixture("local", "commit");
    let stdout = rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    for line in [
        format!("plan {ANCHOR} Helper -> Tool"),
        "  src/lib.rs  5 uses".to_string(),
        "  src/util.rs  4 uses".to_string(),
    ] {
        assert!(stdout.contains(&line), "missing {line}:\n{stdout}");
    }
    let entries = diff_rq(&fixture.root, &tree("local", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `use crate::util::*;` puts the symbol in a scope that writes the bare name
/// with no clause naming it: exit 6 at the `use` item, and the tree keeps its bytes.
#[test]
fn glob_importer_is_a_dynamic_stop() {
    let fixture = fixture("glob", "stop");
    let output = run_rename(
        &fixture.root,
        &fixture.state,
        &format!("{ANCHOR}#Helper"),
        "Tool",
        &["--commit"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(6), "Dynamic exits 6:\n{stderr}");
    let text = std::fs::read_to_string(tree("glob", "before").join("src/lib.rs"))
        .expect("glob fixture text");
    let glob_offset = text.find("use crate::util::*;").expect("glob in fixture");
    let glob_line = text[..glob_offset].matches('\n').count() + 1;
    assert!(
        stderr.contains(&format!("src/lib.rs:{glob_line}: ")),
        "the stop names the use item's line:\n{stderr}"
    );
    assert!(
        stderr.contains("glob import"),
        "the stop names the form:\n{stderr}"
    );
    let entries = diff_rq(&fixture.root, &tree("glob", "before"));
    assert!(
        entries.is_empty(),
        "the stopped run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// A root item plus a function-local struct and a `mod nested` struct of the
/// same name: the root item wins without `--at`; the other two keep their
/// spelling and their uses.
#[test]
fn shadowed_items_need_no_at() {
    let fixture = fixture("shadow", "commit");
    rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("shadow", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// rustc judges the renamed fixture crate: `cargo check` on the committed
/// `local` tree exits 0. The crate has no dependencies, so the check is the
/// fixture's own two files and nothing else.
#[test]
fn renamed_fixture_crate_passes_cargo_check() {
    let fixture = fixture("local", "check");
    rename_verb(&fixture, &format!("{ANCHOR}#Helper"), "Tool", &["--commit"]);
    let check = Command::new("cargo")
        .args(["check", "--offline"])
        .env("CARGO_TARGET_DIR", fixture.root.join("target"))
        .current_dir(&fixture.root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the renamed fixture: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

/// A module path inside an ordinary macro (`check!(ground::decide())`, the
/// boop 2026-09-11 report-8 shape) is a bound reference and renames; the
/// same-spelled field, local, loop binding and string in the same macro stay.
/// FAIL-FIRST, against the report-8 binary:
///     macro_body_paths_rename_and_locals_stay ... exited 6:
///         src/lib.rs byte 214: macro body reaches the symbol at runtime
/// @comment-ok: fail-first receipt, repo law keeps these in TEST headers
#[test]
fn macro_body_paths_rename_and_locals_stay() {
    let fixture = fixture("macro", "commit");
    rename_verb(&fixture, "src/lib.rs#ground", "_1b_ground", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("macro", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// rustc judges the macro fixture crate: the module declared through
/// `#[path = "_1b_ground.rs"]` still compiles once the ident moves.
#[test]
fn renamed_macro_fixture_crate_passes_cargo_check() {
    let fixture = fixture("macro", "check");
    rename_verb(&fixture, "src/lib.rs#ground", "_1b_ground", &["--commit"]);
    let check = Command::new("cargo")
        .args(["check", "--offline"])
        .env("CARGO_TARGET_DIR", fixture.root.join("target"))
        .current_dir(&fixture.root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the renamed macro fixture: {}",
        String::from_utf8_lossy(&check.stderr)
    );
}

/// `--list --commit` is all-or-zero: the valid `ground` row applies nothing
/// when the later `Wyll` row stops at the twin declarations (exit 3).
#[test]
fn list_commit_is_atomic_across_rows() {
    let fixture = fixture("macro", "list");
    let list = fixture.state.join("renames.tsv");
    std::fs::write(
        &list,
        "src/lib.rs\tground\t_1b_ground\nsrc/twins.rs\tWyll\tVyle\n",
    )
    .expect("write rename list");
    let output = Command::new(env!("CARGO_BIN_EXE_ryi"))
        .args(["rename", "--list"])
        .arg(&list)
        .arg("--root")
        .arg(&fixture.root)
        .arg("--state")
        .arg(&fixture.state)
        .arg("--commit")
        .output()
        .expect("extract binary runs");
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert!(
        stderr.contains("src/twins.rs declares Wyll more than once"),
        "the stop names the twin row:\n{stderr}"
    );
    assert_eq!(output.status.code(), Some(3), "Ambiguous exits 3:\n{stderr}");
    let entries = diff_rq(&fixture.root, &tree("macro", "before"));
    assert!(
        entries.is_empty(),
        "a stopped row applied earlier rows:\n{}",
        entries.join("\n")
    );
}

/// The verb run against this crate's own tree, judged by rustc, not by an
/// assertion. MEASURED 2026-08-27: 25.2 s, over the 10-second cap, so it runs by
/// hand: `cargo test --features cli --test 5_rename_rust -- --ignored`.
/// @comment-ok: fail-first/measured receipt, repo law keeps these on the test
#[test]
#[ignore]
fn self_rename_is_judged_by_rustc() {
    let base = scratch("self");
    let root = base.join("sprefa-extract");
    let state = base.join("state");
    std::fs::create_dir_all(&state).expect("create state dir");
    let source = Path::new(env!("CARGO_MANIFEST_DIR"));
    copy_crate(source, &root);
    re_aim_path_deps(source, &root.join("Cargo.toml"));
    let root = root.canonicalize().expect("canonicalize crate copy");

    let output = run_rename(
        &root,
        &state,
        "src/rename_cx.rs#RenameCx",
        "SymbolCx",
        &["--commit"],
    );
    assert!(
        output.status.success(),
        "extract rename exited {}: {}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("read the copy's lib.rs");
    assert!(
        lib.contains("pub use rename_cx::{SymbolCx, RenameRequest};"),
        "the re-export moved:\n{lib}"
    );

    let check = Command::new("cargo")
        .args(["check", "--features", "cli"])
        .current_dir(&root)
        .output()
        .expect("cargo runs");
    assert!(
        check.status.success(),
        "cargo check on the renamed tree: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let _ = std::fs::remove_dir_all(&base);
}

/// The crate's sources, minus the build output and the git store.
fn copy_crate(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).expect("create target dir");
    for entry in std::fs::read_dir(source).expect("read crate dir") {
        let entry = entry.expect("crate entry");
        let name = entry.file_name();
        if matches!(name.to_string_lossy().as_ref(), ".git" | "target") {
            continue;
        }
        let to = target.join(&name);
        if entry.file_type().expect("file type").is_dir() {
            copy_crate(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), &to).expect("copy crate file");
        }
    }
}

/// A copy sits at a different depth, so its two sibling path dependencies are
/// re-pointed at the originals before cargo reads them.
fn re_aim_path_deps(source: &Path, manifest: &Path) {
    let text = std::fs::read_to_string(manifest).expect("read manifest");
    let mut out = text;
    for (rel, name) in [
        ("../../../hafley-rs/crates/soopy", "soopy"),
        ("../../../hafley-rs/crates/hafley-observe", "hafley-observe"),
    ] {
        let absolute = source.join(rel).canonicalize().expect(name);
        out = out.replace(rel, &absolute.to_string_lossy());
    }
    std::fs::write(manifest, out).expect("write manifest");
}

// ── E.1 `#[path]` placement ─────────────────────────────────────────────────

/// `#[path = "elsewhere/impl.rs"] mod util;` places the file at the module the
/// attr names, not the layout guess: the decl, the `use`, and every path in
/// `lib.rs` rename, and `src/other.rs`'s unrelated `Helper` stays.
#[test]
fn path_attr_places_the_file_and_renames_its_seats() {
    let fixture = fixture("path", "commit");
    rename_verb(&fixture, "src/elsewhere/impl.rs#Helper", "Tool", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("path", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

// ── E.2 field and variant seats ─────────────────────────────────────────────

/// A field anchor renames through the receiver plane: the decl, `self.size` in
/// an `impl` method, `h.size` typed off a param annotation, the struct-literal
/// key, and the destructuring-pattern key (which respells shorthand, `size` ->
/// `width: size`, so the local binding keeps its name). `Other.size` and the
/// param typed `&Other` stay untouched, same field name, different owner.
#[test]
fn field_seats_rename_through_the_receiver_plane() {
    let fixture = fixture("field", "commit");
    let text = std::fs::read_to_string(tree("field", "before").join("src/util.rs"))
        .expect("field fixture text");
    let at = text.find("size").expect("size field in fixture").to_string();
    rename_verb(
        &fixture,
        "src/util.rs#size",
        "width",
        &["--at", &at, "--commit"],
    );
    let entries = diff_rq(&fixture.root, &tree("field", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `let v = make(); v.size` where `make` is declared in another file sits
/// outside the one-hop same-file return-type rule, so the receiver types
/// Unknown: a `Dynamic` stop, one seat, form `untyped field`, tree untouched.
#[test]
fn untyped_field_access_is_a_dynamic_stop() {
    let fixture = fixture("field_stop", "stop");
    let output = run_rename(
        &fixture.root,
        &fixture.state,
        "src/util.rs#size",
        "width",
        &["--commit"],
    );
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    assert_eq!(output.status.code(), Some(6), "Dynamic exits 6:\n{stderr}");
    assert!(
        stderr.contains("untyped field"),
        "the stop names the form:\n{stderr}"
    );
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 1, "one seat, one line:\n{stderr}");
    let entries = diff_rq(&fixture.root, &tree("field_stop", "before"));
    assert!(
        entries.is_empty(),
        "the stopped run edited the tree:\n{}",
        entries.join("\n")
    );
}

/// A variant anchor renames every path whose segments end `[Kind, Old]` once
/// `Kind` resolves to the anchor's module: the bare path in `make()`, the match
/// arm, the `use Kind::Old;` clause, and the bare `Old` it binds inside
/// `via_use()`. `mod other`'s own `Kind::Old` is a different enum and stays.
#[test]
fn variant_seats_rename_through_owner_and_use() {
    let fixture = fixture("variant", "commit");
    rename_verb(&fixture, "src/lib.rs#Old", "Prior", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("variant", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

// ── E.3 serde and string spellings ──────────────────────────────────────────

/// `#[serde(rename = "size")]` sits beside `struct Helper { size: u32 }`: the
/// field rename touches the decl and `h.size`, and leaves the literal, the
/// derive, and the `len` field it renames untouched, byte for byte.
#[test]
fn serde_field_seat_renames_and_leaves_the_literal() {
    let fixture = fixture("serde", "commit");
    rename_verb(&fixture, "src/util.rs#size", "width", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("serde", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

/// `--text-refs` reports the `#[serde(rename = "size")]` literal once: it is a
/// string, never a symbol, so the rename NEVER touches it, and the report is
/// the only place it surfaces.
#[test]
fn serde_literal_is_reported_as_a_text_ref() {
    let fixture = fixture("serde", "textrefs");
    let stdout = rename_verb(&fixture, "src/util.rs#size", "width", &["--text-refs"]);
    let text = std::fs::read_to_string(tree("serde", "before").join("src/util.rs"))
        .expect("serde fixture text");
    let line = 1 + text
        .lines()
        .position(|line| line.contains("\"size\""))
        .expect("serde rename literal in fixture");
    let rows: Vec<String> = stdout
        .lines()
        .filter(|line| line.starts_with("text-ref "))
        .map(str::to_string)
        .collect();
    assert_eq!(
        rows,
        vec![format!("text-ref src/util.rs:{line} \"size\" -> \"width\"")],
        "exactly the one literal carrier:\n{stdout}"
    );
    let entries = diff_rq(&fixture.root, &tree("serde", "before"));
    assert!(
        entries.is_empty(),
        "the report changed the tree:\n{}",
        entries.join("\n")
    );
}

// ── E.4 fn-body `use` ───────────────────────────────────────────────────────

/// `use crate::util::Helper;` inside `fn a`'s body binds the name for that
/// block only: the clause and `Helper::new()` inside `a` rename, while `fn b`'s
/// module-scope `struct Helper;` and its own two spellings stay, a same-named
/// item the fn-body `use` never shadows outside its block.
#[test]
fn fn_body_use_scopes_the_bare_name_to_its_block() {
    let fixture = fixture("fnuse", "commit");
    rename_verb(&fixture, "src/util.rs#Helper", "Tool", &["--commit"]);
    let entries = diff_rq(&fixture.root, &tree("fnuse", "after"));
    assert!(
        entries.is_empty(),
        "committed tree differs from after/:\n{}",
        entries.join("\n")
    );
}

// ── cargo check on every new after/ crate ───────────────────────────────────

/// rustc judges each new fixture's committed tree: `#[path]` placement, field
/// and variant receiver typing, the serde literal staying inert, and fn-body
/// `use` scoping all still compile once the plan lands.
#[test]
fn new_fixture_crates_pass_cargo_check() {
    let cases: [(&str, &str, &str); 5] = [
        ("path", "src/elsewhere/impl.rs#Helper", "Tool"),
        ("field", "src/util.rs#size", "width"),
        ("variant", "src/lib.rs#Old", "Prior"),
        ("serde", "src/util.rs#size", "width"),
        ("fnuse", "src/util.rs#Helper", "Tool"),
    ];
    for (case, target, new) in cases {
        let fixture = fixture(case, "check");
        if case == "field" {
            let text = std::fs::read_to_string(fixture.root.join("src/util.rs"))
                .expect("field fixture text");
            let at = text.find("size").expect("size field in fixture").to_string();
            rename_verb(&fixture, target, new, &["--at", &at, "--commit"]);
        } else {
            rename_verb(&fixture, target, new, &["--commit"]);
        }
        let check = Command::new("cargo")
            .args(["check", "--offline"])
            .env("CARGO_TARGET_DIR", fixture.root.join("target"))
            .current_dir(&fixture.root)
            .output()
            .expect("cargo runs");
        assert!(
            check.status.success(),
            "cargo check on {case}/after: {}",
            String::from_utf8_lossy(&check.stderr)
        );
    }
}

// ── --verify-scip, rust-analyzer as the E.2 oracle ──────────────────────────

fn scip_rows(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.starts_with("scip-verify "))
        .map(str::to_string)
        .collect()
}

/// rust-analyzer binds fields, so it is the oracle for the field fixture: the
/// plan's decl/self/param/literal/pattern seats are exactly the index's
/// occurrences of `Helper::size`. MEASURED 2026-09-18: 2.8 s on this fixture,
/// under the 10 s cap, so no `#[ignore]`.
#[test]
fn scip_verify_agrees_on_the_field_fixture() {
    let fixture = fixture("field", "scip");
    let index = ScipRust.build(&fixture.root).expect("rust-analyzer scip index");
    let text = std::fs::read_to_string(tree("field", "before").join("src/util.rs"))
        .expect("field fixture text");
    let at = text.find("size").expect("size field in fixture").to_string();
    let stdout = rename_verb(
        &fixture,
        "src/util.rs#size",
        "width",
        &[
            "--at",
            &at,
            "--verify-scip",
            index.to_str().expect("index path is UTF-8"),
        ],
    );
    assert_eq!(
        scip_rows(&stdout),
        vec!["scip-verify disagreements=0".to_string()],
        "the index and the plan agree on this fixture:\n{stdout}"
    );
}

/// rust-analyzer binds enum variants too: the oracle for the variant fixture,
/// same claim. MEASURED 2026-09-18: 2.2 s on this fixture, under the 10 s cap,
/// so no `#[ignore]`.
#[test]
fn scip_verify_agrees_on_the_variant_fixture() {
    let fixture = fixture("variant", "scip");
    let index = ScipRust.build(&fixture.root).expect("rust-analyzer scip index");
    let stdout = rename_verb(
        &fixture,
        "src/lib.rs#Old",
        "Prior",
        &[
            "--verify-scip",
            index.to_str().expect("index path is UTF-8"),
        ],
    );
    assert_eq!(
        scip_rows(&stdout),
        vec!["scip-verify disagreements=0".to_string()],
        "the index and the plan agree on this fixture:\n{stdout}"
    );
}
