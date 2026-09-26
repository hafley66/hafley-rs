//! `fixtures/cleave_ladder` cleaved 0 / 1 / many ways, then as `--list` batches: each
//! case is its `git diff -U0` lines and `cargo check --all-targets` on the tree it leaves.

#![cfg(feature = "cli")]

use std::path::Path;
use std::process::Command;

const CASES: [(&str, &[&str]); 7] = [
    ("0 whole-file item to a new file", &["src/_2_dest.rs#Existing", "src/_3_new.rs"]),
    ("1 docs, derive, pub use, re-export importer", &["src/_1_src.rs#Documented", "src/_2_dest.rs"]),
    ("many impl, trait method, test crate importer", &["src/_1_src.rs#Plain", "src/_2_dest.rs"]),
    ("batch all three rows", &["--list", "LIST"]),
    ("batch glob re-export, inline super glob, nested test use", &["--list", "GLOB"]),
    ("batch drops SRC imports made unused", &["--list", "STALE"]),
    ("multiline use retains member layout", &["src/_1_src.rs#Documented", "src/_2_dest.rs"]),
];

const GLOB: &str = "src/_4_types.rs#Span4	src/_6_moved.rs
src/_4_types.rs#Req4	src/_6_moved.rs
";

const LIST: &str = "src/_2_dest.rs#Existing\tsrc/_3_new.rs\nsrc/_1_src.rs#Documented\tsrc/_2_dest.rs\nsrc/_1_src.rs#Plain\tsrc/_2_dest.rs\n";
const STALE: &str = "src/_1_src.rs#Documented\tsrc/_2_dest.rs\nsrc/_1_src.rs#Plain\tsrc/_2_dest.rs\n";

fn run(program: &str, args: &[&str], dir: &Path, target: &Path) -> (bool, String) {
    let output = Command::new(program).args(args).current_dir(dir).env("CARGO_TARGET_DIR", target).env("RUST_LOG", "off").output().unwrap();
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
fn cleave_ladder() {
    let scratch = tempfile::tempdir().unwrap();
    let target = scratch.path().join("target");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/cleave_ladder");
    let list = scratch.path().join("list.tsv");
    std::fs::write(&list, LIST).unwrap();
    let glob = scratch.path().join("glob.tsv");
    std::fs::write(&glob, GLOB).unwrap();
    let stale = scratch.path().join("stale.tsv");
    std::fs::write(&stale, STALE).unwrap();
    let mut out = Vec::new();
    for (index, (label, case)) in CASES.iter().enumerate() {
        let root = scratch.path().join(format!("case{index}"));
        let state = scratch.path().join(format!("state{index}"));
        copy_tree(&fixture, &root);
        if index == 5 {
            let src = root.join("src/_1_src.rs");
            let text = std::fs::read_to_string(&src).unwrap();
            std::fs::write(&src, text.split("pub fn keeps_using()").next().unwrap()).unwrap();
        }
        if index == 6 {
            let base = root.join("src/_0_base.rs");
            let mut text = std::fs::read_to_string(&base).unwrap();
            text.push_str("\npub struct Marker;\n");
            std::fs::write(base, text).unwrap();
            let src = root.join("src/_1_src.rs");
            let text = std::fs::read_to_string(&src).unwrap().replace("    Show,\n", "    Show,\n    Marker,\n");
            std::fs::write(src, format!("{text}\npub fn marker() -> Marker {{ Marker }}\n")).unwrap();
        }
        for args in [&["init", "-q", "."][..], &["add", "-A"], &["-c", "user.email=l@l", "-c", "user.name=l", "commit", "-qm", "l"]] {
            assert!(run("git", args, &root, &target).0);
        }
        let mut args = vec!["cleave".to_string()];
        for arg in *case {
            args.push(match *arg {
                "LIST" => list.to_string_lossy().into_owned(),
                "GLOB" => glob.to_string_lossy().into_owned(),
                "STALE" => stale.to_string_lossy().into_owned(),
                "--list" => arg.to_string(),
                _ => root.join(arg).to_string_lossy().into_owned(),
            });
        }
        let root_arg = root.to_string_lossy().into_owned();
        let state_arg = state.to_string_lossy().into_owned();
        args.extend(["--root", &root_arg, "--state", &state_arg, "--commit"].map(String::from));
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let (cleaved, text) = run(env!("CARGO_BIN_EXE_ryi"), &args, &root, &target);
        assert!(cleaved, "{label}: {text}");
        run("git", &["add", "-A"], &root, &target);
        let (checked, check) = run("cargo", &["check", "--offline", "--all-targets", "-q", "--message-format", "short"], &root, &target);
        let check = match checked {
            true => "ok".to_string(),
            false => check.lines().find(|line| line.contains("error")).unwrap_or("FAIL").to_string(),
        };
        out.push(format!("## {label}: check {check}"));
        let (_, diff) = run("git", &["diff", "--cached", "-U0", "--no-color"], &root, &target);
        for line in diff.lines() {
            if let Some(file) = line.strip_prefix("+++ b/") {
                out.push(format!("  {file}"));
            } else if (line.starts_with('+') || line.starts_with('-')) && !line.starts_with("+++") && !line.starts_with("---") {
                out.push(format!("    {line}"));
            }
        }
    }
    assert_eq!(
        out.join("\n"),
        r#"## 0 whole-file item to a new file: check ok
  src/_2_dest.rs
    -pub struct Existing;
  src/_3_new.rs
    +pub struct Existing;
  src/lib.rs
    +pub mod _3_new;
## 1 docs, derive, pub use, re-export importer: check ok
  src/_1_src.rs
    -    Base,
    -/// Documented item.
    -/// Second doc line.
    -#[derive(Debug, Clone)]
    -pub struct Documented {
    -    pub base: Base,
    -}
    -
  src/_2_dest.rs
    +use crate::_0_base::Base;
    +
    +/// Documented item.
    +/// Second doc line.
    +#[derive(Debug, Clone)]
    +pub struct Documented {
    +    pub base: Base,
    +}
  src/lib.rs
    -    Documented,
    +pub use crate::_2_dest::Documented;
## many impl, trait method, test crate importer: check ok
  src/_1_src.rs
    +use crate::_2_dest::Plain;
    -pub struct Plain;
    -
    -impl Show for Plain {
    -    fn show(&self) -> u32 {
    -        1
    -    }
    -}
    -
  src/_2_dest.rs
    +use crate::_0_base::Show;
    +
    +pub struct Plain;
    +
    +impl Show for Plain {
    +    fn show(&self) -> u32 {
    +        1
    +    }
    +}
  src/lib.rs
    -    Plain,
    +pub use crate::_2_dest::Plain;
  tests/uses.rs
    -use cleave_ladder::_1_src::Plain;
    +use cleave_ladder::_2_dest::Plain;
## batch all three rows: check ok
  src/_1_src.rs
    -    Base,
    -
    -/// Documented item.
    -/// Second doc line.
    -#[derive(Debug, Clone)]
    -pub struct Documented {
    -    pub base: Base,
    -}
    -
    -pub struct Plain;
    -
    -impl Show for Plain {
    -    fn show(&self) -> u32 {
    -        1
    -    }
    -}
    +use crate::_2_dest::Plain;
  src/_2_dest.rs
    -pub struct Existing;
    +use crate::_0_base::Base;
    +use crate::_0_base::Show;
    +
    +/// Documented item.
    +/// Second doc line.
    +#[derive(Debug, Clone)]
    +pub struct Documented {
    +    pub base: Base,
    +}
    +
    +pub struct Plain;
    +
    +impl Show for Plain {
    +    fn show(&self) -> u32 {
    +        1
    +    }
    +}
  src/_3_new.rs
    +pub struct Existing;
  src/lib.rs
    +pub mod _3_new;
    -pub use _1_src::{
    -    Documented,
    -    Plain,
    -};
    +pub use crate::_2_dest::Documented;
    +pub use crate::_2_dest::Plain;
  tests/uses.rs
    -use cleave_ladder::_1_src::Plain;
    +use cleave_ladder::_2_dest::Plain;
## batch glob re-export, inline super glob, nested test use: check ok
  src/_4_types.rs
    -pub struct Span4 {
    -    pub at: u32,
    -}
    -
    -pub struct Req4 {
    -    pub span: Span4,
    -}
    -
    +use crate::_6_moved::Span4;
  src/_5_user.rs
    -    use crate::_4_types::{Req4, Span4};
    +    use crate::_6_moved::Req4;
    +    use crate::_6_moved::Span4;
  src/_6_moved.rs
    +pub struct Span4 {
    +    pub at: u32,
    +}
    +
    +pub struct Req4 {
    +    pub span: Span4,
    +}
  src/lib.rs
    +pub(crate) mod _6_moved;
    +pub use crate::_6_moved::Req4;
    +pub use crate::_6_moved::Span4;
## batch drops SRC imports made unused: check ok
  src/_1_src.rs
    -use crate::_0_base::{
    -    Base,
    -    Show,
    -};
    -
    -/// Documented item.
    -/// Second doc line.
    -#[derive(Debug, Clone)]
    -pub struct Documented {
    -    pub base: Base,
    -}
    -
    -pub struct Plain;
    -
    -impl Show for Plain {
    -    fn show(&self) -> u32 {
    -        1
    -    }
    -}
    -
  src/_2_dest.rs
    +use crate::_0_base::Base;
    +use crate::_0_base::Show;
    +
    +/// Documented item.
    +/// Second doc line.
    +#[derive(Debug, Clone)]
    +pub struct Documented {
    +    pub base: Base,
    +}
    +
    +pub struct Plain;
    +
    +impl Show for Plain {
    +    fn show(&self) -> u32 {
    +        1
    +    }
    +}
  src/lib.rs
    -pub use _1_src::{
    -    Documented,
    -    Plain,
    -};
    +pub use crate::_2_dest::Documented;
    +pub use crate::_2_dest::Plain;
  tests/uses.rs
    -use cleave_ladder::_1_src::Plain;
    +use cleave_ladder::_2_dest::Plain;
## multiline use retains member layout: check ok
  src/_1_src.rs
    -    Base,
    -/// Documented item.
    -/// Second doc line.
    -#[derive(Debug, Clone)]
    -pub struct Documented {
    -    pub base: Base,
    -}
    -
  src/_2_dest.rs
    +use crate::_0_base::Base;
    +
    +/// Documented item.
    +/// Second doc line.
    +#[derive(Debug, Clone)]
    +pub struct Documented {
    +    pub base: Base,
    +}
  src/lib.rs
    -    Documented,
    +pub use crate::_2_dest::Documented;"#
    );
}
