use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const MAX_LAB_ARGS: usize = 6;

fn main() -> ExitCode {
    match run(std::env::args().skip(1).collect()) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("oh: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<u8, String> {
    match args.as_slice() {
        [group, action, rest @ ..] if group == "lab" && action == "new" => {
            create_lab(rest).map(|()| 0)
        }
        [flag] if flag == "--help" || flag == "-h" => {
            println!("oh lab new --title <slug> --manifest <Cargo.toml> [--root <labs-dir>]");
            Ok(0)
        }
        _ => Err(
            "usage: oh lab new --title <slug> --manifest <Cargo.toml> [--root <labs-dir>]".into(),
        ),
    }
}

fn create_lab(args: &[String]) -> Result<(), String> {
    let mut title = None;
    let mut manifest = None;
    let mut root = None;
    let mut index = 0;
    if args.len() > MAX_LAB_ARGS {
        return Err(format!(
            "lab command accepts at most {MAX_LAB_ARGS} arguments"
        ));
    }
    // budget: MAX_LAB_ARGS user-supplied tokens per lab command
    while index < args.len() {
        let (slot, flag) = match args[index].as_str() {
            "--title" => (&mut title, "--title"),
            "--manifest" => (&mut manifest, "--manifest"),
            "--root" => (&mut root, "--root"),
            other => return Err(format!("unknown argument `{other}`")),
        };
        index += 1;
        let value = args
            .get(index)
            .ok_or_else(|| format!("{flag} needs a value"))?;
        *slot = Some(PathBuf::from(value));
        index += 1;
    }

    let title = title
        .and_then(|path| path.to_str().map(str::to_owned))
        .ok_or("missing --title")?;
    validate_title(&title)?;
    let manifest = manifest.ok_or("missing --manifest")?;
    let manifest = std::fs::canonicalize(&manifest)
        .map_err(|error| format!("resolve manifest {}: {error}", manifest.display()))?;
    if manifest.file_name().is_none_or(|name| name != "Cargo.toml") {
        return Err(format!("{} is not Cargo.toml", manifest.display()));
    }

    let labs_root = match root {
        Some(root) => root,
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("labs"),
    };
    std::fs::create_dir_all(&labs_root)
        .map_err(|error| format!("create labs directory {}: {error}", labs_root.display()))?;
    let date = Command::new("date")
        .args(["-u", "+%Y%m%d"])
        .output()
        .map_err(|error| format!("read UTC date: {error}"))?;
    if !date.status.success() {
        return Err("date -u failed".into());
    }
    let date = String::from_utf8_lossy(&date.stdout).trim().to_owned();
    let stem = format!("lab-{date}-{title}");
    let path = (1_u32..)
        .map(|index| {
            if index == 1 {
                labs_root.join(&stem)
            } else {
                labs_root.join(format!("{stem}-{index:02}"))
            }
        })
        .find(|path| !path.exists())
        .ok_or("could not allocate an indexed lab path")?;

    let metadata = Command::new("cargo")
        .arg("metadata")
        .arg("--offline")
        .arg("--format-version")
        .arg("1")
        .arg("--manifest-path")
        .arg(&manifest)
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))?;
    if !metadata.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&metadata.stderr)
        ));
    }
    let metadata: Value = serde_json::from_slice(&metadata.stdout)
        .map_err(|error| format!("parse cargo metadata: {error}"))?;
    let manifest_text = lab_manifest(&metadata, &manifest, &path)?;
    std::fs::create_dir_all(path.join("src"))
        .map_err(|error| format!("create lab source directory: {error}"))?;
    std::fs::write(path.join("Cargo.toml"), manifest_text)
        .map_err(|error| format!("write lab Cargo.toml: {error}"))?;
    std::fs::write(
        path.join("src/main.rs"),
        "fn main() { println!(\"lab build output\"); }\n",
    )
    .map_err(|error| format!("write lab entrypoint: {error}"))?;
    append_index(&labs_root, &path)?;

    println!("created {}", path.display());
    println!(
        "build: cargo check --offline --manifest-path {}/Cargo.toml",
        path.display()
    );
    let output = Command::new("cargo")
        .arg("check")
        .arg("--offline")
        .arg("--manifest-path")
        .arg(path.join("Cargo.toml"))
        .output()
        .map_err(|error| format!("run generated lab build: {error}"))?;
    print!("{}", String::from_utf8_lossy(&output.stdout));
    eprint!("{}", String::from_utf8_lossy(&output.stderr));
    if !output.status.success() {
        return Err(format!("generated lab build failed with {}", output.status));
    }
    println!("build: passed");
    Ok(())
}

fn validate_title(title: &str) -> Result<(), String> {
    let valid = !title.is_empty()
        && title.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        });
    if valid {
        Ok(())
    } else {
        Err("title must be lowercase kebab-case ASCII letters and digits".into())
    }
}

fn lab_manifest(metadata: &Value, manifest: &Path, lab_path: &Path) -> Result<String, String> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("cargo metadata has no package list")?;
    let source = packages
        .iter()
        .find(|package| {
            package["manifest_path"].as_str() == Some(manifest.to_string_lossy().as_ref())
        })
        .ok_or("cargo metadata did not return the entry manifest package")?;
    let node = metadata["resolve"]["nodes"]
        .as_array()
        .and_then(|nodes| nodes.iter().find(|node| node["id"] == source["id"]))
        .ok_or("cargo metadata has no resolved dependency node for entry package")?;
    let mut sections = BTreeMap::<&str, BTreeMap<String, String>>::new();
    for dependency in node["deps"]
        .as_array()
        .ok_or("entry package has no resolved dependencies")?
    {
        let alias = dependency["name"]
            .as_str()
            .ok_or("dependency has no alias")?;
        let package_id = dependency["pkg"]
            .as_str()
            .ok_or("dependency has no package id")?;
        let package = packages
            .iter()
            .find(|package| package["id"].as_str() == Some(package_id))
            .ok_or_else(|| format!("resolved package {package_id} is absent"))?;
        if package["source"].is_null() {
            continue;
        }
        let declared = source["dependencies"]
            .as_array()
            .and_then(|dependencies| {
                dependencies.iter().find(|declared| {
                    let rename = declared["rename"].as_str();
                    let declared_alias =
                        rename.unwrap_or_else(|| declared["name"].as_str().unwrap_or_default());
                    normalized_name(declared_alias) == normalized_name(alias)
                })
            })
            .ok_or_else(|| format!("dependency `{alias}` is missing its declaration"))?;
        let declared_alias = declared["rename"]
            .as_str()
            .or_else(|| declared["name"].as_str())
            .ok_or_else(|| format!("dependency `{alias}` has no declared name"))?;
        let version = package["version"]
            .as_str()
            .ok_or("resolved package has no version")?;
        let features = declared["features"].as_array().cloned().unwrap_or_default();
        let feature_list = features
            .iter()
            .filter_map(Value::as_str)
            .map(toml_string)
            .collect::<Vec<_>>()
            .join(", ");
        let package_name = package["name"]
            .as_str()
            .ok_or("resolved package has no name")?;
        let mut fields = vec![format!("version = {}", toml_string(&format!("={version}")))];
        if package_name != declared_alias {
            fields.push(format!("package = {}", toml_string(package_name)));
        }
        if !feature_list.is_empty() {
            fields.push(format!("features = [{feature_list}]"));
        }
        if declared["uses_default_features"].as_bool() == Some(false) {
            fields.push("default-features = false".into());
        }
        if declared["optional"].as_bool() == Some(true) {
            fields.push("optional = true".into());
        }
        let section = match declared["kind"].as_str() {
            Some("dev") => "dev-dependencies",
            Some("build") => "build-dependencies",
            _ => "dependencies",
        };
        sections.entry(section).or_default().insert(
            declared_alias.to_owned(),
            format!(
                "{} = {{ {} }}",
                toml_string(declared_alias),
                fields.join(", ")
            ),
        );
    }

    let crate_name = lab_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("lab directory name is not valid UTF-8")?;
    let mut output = format!(
        "[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n\n[workspace]\n",
        toml_string(crate_name)
    );
    for (section, dependencies) in sections {
        output.push_str(&format!("\n[{section}]\n"));
        for dependency in dependencies.values() {
            output.push_str(dependency);
            output.push('\n');
        }
    }
    Ok(output)
}

fn append_index(root: &Path, lab_path: &Path) -> Result<(), String> {
    let index = root.join("INDEX.md");
    let mut text = if index.exists() {
        std::fs::read_to_string(&index).map_err(|error| format!("read lab index: {error}"))?
    } else {
        "# Labs\n\n".to_owned()
    };
    let name = lab_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("lab directory name is not valid UTF-8")?;
    text.push_str(&format!("- [{name}]({name}/)\n"));
    std::fs::write(index, text).map_err(|error| format!("write lab index: {error}"))
}

fn toml_string(value: &str) -> String {
    serde_json::to_string(value).expect("string serialization cannot fail")
}

fn normalized_name(value: &str) -> String {
    value.replace('-', "_")
}
