//! Every ladder symbol, plus rename_merge with and without the SCIP merge, renamed with `--commit`
//! in a temp copy, then `cargo check --offline`: one row per rename, `uses scip_diff check`.

#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;

const SYMBOLS: [(&str, &str); 21] = [
    ("_0_types.rs", "A"),
    ("_0_types.rs", "B"),
    ("_0_types.rs", "C"),
    ("_0_types.rs", "T"),
    ("_0_types.rs", "U"),
    ("_0_types.rs", "V"),
    ("_0_types.rs", "W"),
    ("_1_none.rs", "NoField"),
    ("_1_none.rs", "no_sig"),
    ("_2_one.rs", "OneField"),
    ("_2_one.rs", "OneVariant"),
    ("_2_one.rs", "OneAlias"),
    ("_2_one.rs", "one_param"),
    ("_2_one.rs", "one_bound"),
    ("_2_one.rs", "OneBound"),
    ("_3_many.rs", "ManyFields"),
    ("_3_many.rs", "ManyVariants"),
    ("_3_many.rs", "many_params"),
    ("_3_many.rs", "method"),
    ("_4_nested.rs", "Nest"),
    ("_4_nested.rs", "projection"),
];

fn run(program: &str, args: &[&str], dir: &Path, envs: &[(&str, &Path)]) -> (bool, String) {
    let output = Command::new(program).args(args).current_dir(dir).envs(envs.iter().copied()).env("RUST_LOG", "off").output().unwrap();
    let text = String::from_utf8_lossy(&output.stdout).into_owned() + &String::from_utf8_lossy(&output.stderr);
    (output.status.success(), text)
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        match entry.file_type().unwrap().is_dir() {
            true => copy_tree(&entry.path(), &to.join(entry.file_name())),
            false => drop(std::fs::copy(entry.path(), to.join(entry.file_name())).unwrap()),
        }
    }
}

#[test]
fn every_ladder_rename_compiles() {
    let scratch = tempfile::tempdir().unwrap();
    let target = scratch.path().join("target");
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let rows = SYMBOLS
        .iter()
        .map(|(file, symbol)| ("type_ladder", *file, *symbol, None))
        .chain([
            ("rename_merge", "_0_types.rs", "A", None),
            ("rename_merge", "_0_types.rs", "A", Some("--no-scip-merge")),
        ]);
    let mut table = Vec::new();
    for (index, (fixture, file, symbol, flag)) in rows.enumerate() {
        let root = scratch.path().join(format!("{index}"));
        let state = scratch.path().join(format!("{index}.state"));
        copy_tree(&fixtures.join(fixture), &root);
        for args in [&["init", "-q", "."][..], &["add", "-A"], &["-c", "user.email=l@l", "-c", "user.name=l", "commit", "-qm", "l"]] {
            assert!(run("git", args, &root, &[]).0);
        }
        let anchor = format!("{}/src/{file}#{symbol}", root.display());
        let renamed = format!("{symbol}Zz");
        let (root_arg, state_arg) = (root.to_string_lossy(), state.to_string_lossy());
        let mut args = vec!["rename", &anchor, &renamed, "--root", &root_arg, "--state", &state_arg, "--commit"];
        args.extend(flag);
        let (_, plan) = run(env!("CARGO_BIN_EXE_ryi"), &args, &root, &[]);
        let uses: usize = plan
            .lines()
            .filter_map(|line| line.strip_suffix(" uses")?.rsplit(' ').next()?.parse::<usize>().ok())
            .sum();
        let diff = plan.lines().find_map(|line| line.strip_prefix("scip-verify disagreements=")).unwrap_or("-").to_string();
        let (ok, check) = run("cargo", &["check", "--offline", "-q"], &root, &[("CARGO_TARGET_DIR", &target)]);
        let check = match ok {
            true => "ok".to_string(),
            false => check.lines().find(|line| line.starts_with("error")).unwrap_or("FAIL").to_string(),
        };
        let label = format!("{fixture}/{file}#{symbol} {}", flag.unwrap_or(""));
        table.push(format!("{:<44} {uses:>4} {diff:>4}  {check}", label.trim_end()));
    }
    assert_eq!(
        table.join("\n"),
        r#"type_ladder/_0_types.rs#A                      21    0  ok
type_ladder/_0_types.rs#B                      12    0  ok
type_ladder/_0_types.rs#C                      10    0  ok
type_ladder/_0_types.rs#T                      11    0  ok
type_ladder/_0_types.rs#U                       5    0  ok
type_ladder/_0_types.rs#V                       6    0  ok
type_ladder/_0_types.rs#W                       4    0  ok
type_ladder/_1_none.rs#NoField                  3    0  ok
type_ladder/_1_none.rs#no_sig                   1    0  ok
type_ladder/_2_one.rs#OneField                  4    0  ok
type_ladder/_2_one.rs#OneVariant                1    0  ok
type_ladder/_2_one.rs#OneAlias                  1    0  ok
type_ladder/_2_one.rs#one_param                 1    0  ok
type_ladder/_2_one.rs#one_bound                 1    0  ok
type_ladder/_2_one.rs#OneBound                  1    0  ok
type_ladder/_3_many.rs#ManyFields               4    0  ok
type_ladder/_3_many.rs#ManyVariants             1    0  ok
type_ladder/_3_many.rs#many_params              1    0  ok
type_ladder/_3_many.rs#method                   1    0  ok
type_ladder/_4_nested.rs#Nest                   3    0  ok
type_ladder/_4_nested.rs#projection             1    0  ok
rename_merge/_0_types.rs#A                      6    3  ok
rename_merge/_0_types.rs#A --no-scip-merge      4    3  error[E0425]: cannot find value `A` in module `types`"#
    );
}
