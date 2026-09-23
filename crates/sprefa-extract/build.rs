use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn ident(name: &str) -> String {
    let mut out = String::new();
    for ch in name.chars() {
        out.push(if ch.is_ascii_alphanumeric() { ch } else { '_' });
    }
    out
}

fn type_name(name: &str) -> String {
    name.split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            format!("{}{}", chars.next().unwrap().to_ascii_uppercase(), chars.as_str())
        })
        .collect()
}

fn generate_kotlin_scm() {
    let path = "queries/kotlin/scip.scm";
    println!("cargo:rerun-if-changed={path}");
    let scm = std::fs::read_to_string(path).expect("Kotlin SCM exists");
    let language = tree_sitter::Language::new(tree_sitter_kotlin_sg::LANGUAGE);
    let query = hafley_scm::build(&language, &scm).expect("Kotlin SCM compiles");
    let mut code = String::from("// Generated from queries/kotlin/scip.scm at build time.\n");
    for (relation_id, relation) in query.relations.iter().enumerate() {
        let name = type_name(relation);
        assert!(!name.is_empty(), "emitted relation needs an identifier");
        let fields: Vec<u16> = query.emits.iter()
            .filter(|emit| emit.relation as usize == relation_id)
            .flat_map(|emit| emit.fields.iter().map(|field| field.key))
            .collect();
        code.push_str(&format!(
            "pub(crate) struct {name}<'a> {{ fact: &'a hafley_scm::EmittedFact, arena: &'a hafley_scm::MatchArena }}\n"
        ));
        code.push_str(&format!(
            "impl<'a> {name}<'a> {{\n  pub(crate) fn rows(arena: &'a hafley_scm::MatchArena) -> impl Iterator<Item = Self> + 'a {{ arena.emitted.iter().filter(|fact| fact.relation == {relation_id}).map(move |fact| Self {{ fact, arena }}) }}\n"
        ));
        for field_id in fields.iter().copied().collect::<std::collections::BTreeSet<_>>() {
            let field_name = ident(&query.fields[field_id as usize]);
            assert!(!field_name.is_empty(), "emitted field needs an identifier");
            let required = query.emits.iter()
                .filter(|emit| emit.relation as usize == relation_id)
                .all(|emit| emit.fields.iter().any(|field| field.key == field_id));
            if required {
                code.push_str(&format!(
                    "  pub(crate) fn {field_name}(&self) -> &'a hafley_scm::EmittedValue {{ self.fact.get(self.arena, {field_id}).expect(\"SCM emitted required field\") }}\n"
                ));
            } else {
                code.push_str(&format!(
                    "  pub(crate) fn {field_name}(&self) -> Option<&'a hafley_scm::EmittedValue> {{ self.fact.get(self.arena, {field_id}) }}\n"
                ));
            }
        }
        code.push_str("}\n");
    }
    let out = std::path::PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR"));
    std::fs::write(out.join("kotlin_scm.rs"), code).expect("write generated SCM accessors");
}

fn command_output(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|value| !value.is_empty())
}

fn main() {
    generate_kotlin_scm();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=src");

    let mut git_paths = vec![
        "HEAD".to_string(),
        "index".to_string(),
        "packed-refs".to_string(),
    ];
    if let Some(reference) = command_output("git", &["symbolic-ref", "-q", "HEAD"]) {
        git_paths.push(reference);
    }
    for git_path in git_paths {
        if let Some(path) = command_output("git", &["rev-parse", "--git-path", &git_path]) {
            println!("cargo:rerun-if-changed={path}");
        }
    }

    let git_hash = command_output("git", &["rev-parse", "--short=12", "HEAD"])
        .unwrap_or_else(|| "unknown".to_string());
    let build_datetime =
        command_output("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]).unwrap_or_else(|| {
            let seconds = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_secs());
            format!("unix:{seconds}")
        });

    println!("cargo:rustc-env=SPREFA_BUILD_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=SPREFA_BUILD_DATETIME={build_datetime}");
}
