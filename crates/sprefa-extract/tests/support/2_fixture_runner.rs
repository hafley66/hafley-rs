use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

/// Run a fixture table in a process containing only its own test. Other suite
/// tests replace process-wide PATH and SPREFA_SCIP_INDEX while probing indexers.
/// The build environment supplies the tools; fixture steps supply overrides.
pub fn run(directory: &str, case: impl Fn(&Value) -> Value) {
    const WORKER: &str = "SPREFA_FIXTURE_WORKER";
    let thread = std::thread::current();
    let test = thread.name().expect("fixture runner is called from a test");
    if std::env::var(WORKER).as_deref() == Ok(test) {
        snapshot(directory, evaluate(directory, case));
        return;
    }
    // SCIP stages are persistent within temp_dir and keyed by fixture root.
    // Keep stages for this table separate from parallel tests of the same root.
    let temporary = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", test, "--nocapture", "--test-threads=1"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .env_clear()
        .env(WORKER, test)
        .env("PATH", env!("PATH"))
        .env("HOME", env!("HOME"))
        .env("TMPDIR", temporary.path())
        .env("TMP", temporary.path())
        .env("TEMP", temporary.path())
        .env("LANG", "en_US.UTF-8")
        .env("KACHE_DISABLED", "1")
        .env("DL_TRACE", "0")
        .env("DL_TRAIL", "0")
        .env("RUST_LOG", "off")
        .env("INSTA_UPDATE", "no");
    for (key, value) in [
        ("CARGO_HOME", option_env!("CARGO_HOME")),
        ("RUSTUP_HOME", option_env!("RUSTUP_HOME")),
        ("CARGO_TARGET_DIR", option_env!("CARGO_TARGET_DIR")),
    ] {
        if let Some(value) = value {
            command.env(key, value);
        }
    }
    let output = command.output().expect("isolated fixture test runs");
    assert!(
        output.status.success(),
        "{test}: isolated fixture test failed ({:?})\n{}\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

pub fn evaluate(directory: &str, case: impl Fn(&Value) -> Value) -> BTreeMap<String, Value> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    let mut files: Vec<_> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    files.sort();
    files
        .into_iter()
        .map(|path| {
            let input = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            (
                path.file_stem().unwrap().to_str().unwrap().to_string(),
                case(&input),
            )
        })
        .collect()
}

pub fn snapshot(directory: &str, output: BTreeMap<String, Value>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(directory);
    insta::with_settings!({snapshot_path=>root,prepend_module_to_snapshot=>false,omit_expression=>true}, {
        insta::assert_json_snapshot!("output",output);
    });
}

/// Map fixture placeholders and private scratch roots in JSON values.
pub fn replace_strings(value: &Value, replacements: &[(&str, &str)]) -> Value {
    match value {
        Value::String(text) => Value::String(
            replacements
                .iter()
                .fold(text.clone(), |text, (from, to)| text.replace(from, to)),
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| replace_strings(item, replacements))
                .collect(),
        ),
        Value::Object(entries) => Value::Object(
            entries
                .iter()
                .map(|(key, item)| (key.clone(), replace_strings(item, replacements)))
                .collect(),
        ),
        other => other.clone(),
    }
}

pub fn commands(case: &Value, api: impl Fn(&Value) -> Value) -> Value {
    use std::process::Command;
    let temporary = tempfile::tempdir().unwrap();
    let canonical_work = temporary.path().canonicalize().unwrap();
    let work = canonical_work.to_str().unwrap();
    let expand = |text: &str| {
        text.replace("$work", work)
            .replace(
                "$fixtures",
                concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"),
            )
            .replace("$manifest", env!("CARGO_MANIFEST_DIR"))
            .replace("$ryii", env!("CARGO_BIN_EXE_ryii"))
            .replace("$cargo", env!("CARGO"))
    };
    let mut outputs = BTreeMap::<String, (String, String)>::new();
    let mut observed = BTreeMap::new();
    for step in case["steps"].as_array().unwrap() {
        let name = step["capture"]
            .as_str()
            .or_else(|| step["name"].as_str())
            .unwrap_or("setup");
        let path = || std::path::PathBuf::from(expand(step["path"].as_str().unwrap()));
        match step["action"].as_str().unwrap() {
            "directory" => std::fs::create_dir_all(path()).unwrap(),
            "copy_tree" => {
                let target = std::path::PathBuf::from(expand(step["target"].as_str().unwrap()));
                if step["fresh"] == true && target.exists() {
                    std::fs::remove_dir_all(&target).unwrap();
                }
                copy_tree(
                    std::path::Path::new(&expand(step["source"].as_str().unwrap())),
                    &target,
                );
            }
            "read_tree" => {
                observed.insert(
                    name.to_string(),
                    serde_json::json!(std::fs::read_to_string(path()).unwrap()),
                );
            }
            "replace" => {
                let text = std::fs::read_to_string(path()).unwrap();
                let from = step["from"].as_str().unwrap();
                let to = step["to"].as_str().unwrap();
                let replacement = if let Some(count) = step["count"].as_u64() {
                    assert!(text.contains(from), "{step}");
                    text.replacen(from, to, count as usize)
                } else {
                    text.replace(from, to)
                };
                std::fs::write(path(), replacement).unwrap();
            }
            "tree_matches" => {
                let actual = tree_contents(Path::new(&expand(step["left"].as_str().unwrap())));
                let expected = tree_contents(Path::new(&expand(
                    step["fixture"]
                        .as_str()
                        .or_else(|| step["right"].as_str())
                        .unwrap(),
                )));
                assert_eq!(actual, expected, "{step}");
                observed.insert(name.to_string(), serde_json::json!({"files":actual}));
            }
            "sql_query" => {
                let db =
                    rusqlite::Connection::open(expand(step["database"].as_str().unwrap())).unwrap();
                let count: i64 = db
                    .query_row(step["query"].as_str().unwrap(), [], |row| row.get(0))
                    .unwrap();
                assert_eq!(serde_json::json!(count), step["expect"]);
                observed.insert(name.to_string(), serde_json::json!(count));
            }
            "claim" => {
                let value = claim(step, &observed, &expand);
                observed.insert(name.to_string(), value);
            }
            "copy" => {
                let target = path();
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::copy(expand(step["source"].as_str().unwrap()), target).unwrap();
            }
            "write" => {
                let target = path();
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                let previous = step["preserve_mtime"]
                    .as_bool()
                    .unwrap_or(false)
                    .then(|| std::fs::metadata(&target).unwrap().modified().unwrap());
                let bytes = if let Some(bytes) = step["bytes"].as_array() {
                    bytes
                        .iter()
                        .map(|byte| byte.as_u64().unwrap() as u8)
                        .collect()
                } else if let Some(repeat) = step["repeat"].as_object() {
                    let unit = expand(repeat["text"].as_str().unwrap());
                    let count = repeat["count"].as_u64().unwrap();
                    let mut composed = expand(step["prefix"].as_str().unwrap_or(""));
                    for _ in 0..count {
                        composed.push_str(&unit);
                    }
                    composed.push_str(&expand(step["suffix"].as_str().unwrap_or("")));
                    composed.into_bytes()
                } else {
                    expand(step["text"].as_str().unwrap()).into_bytes()
                };
                std::fs::write(&target, bytes).unwrap();
                if let Some(modified) = previous {
                    std::fs::File::open(&target)
                        .unwrap()
                        .set_times(std::fs::FileTimes::new().set_modified(modified))
                        .unwrap();
                }
                #[cfg(unix)]
                if step["executable"].as_bool().unwrap_or(false) {
                    use std::os::unix::fs::PermissionsExt;
                    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755))
                        .unwrap();
                }
            }
            "remove" => std::fs::remove_file(path()).unwrap(),
            "mtime" => {
                let modified = std::time::UNIX_EPOCH
                    + std::time::Duration::from_millis(step["milliseconds"].as_u64().unwrap());
                std::fs::File::open(path())
                    .unwrap()
                    .set_times(std::fs::FileTimes::new().set_modified(modified))
                    .unwrap();
            }
            "exists" => {
                let exists = path().exists();
                assert_eq!(Value::Bool(exists), step["expected"], "{name}");
                observed.insert(name.to_string(), serde_json::json!({"exists":exists}));
            }
            "file_equals" => {
                let stored = std::fs::read(path()).unwrap();
                let expected: Vec<u8> = if let Some(bytes) = step["bytes"].as_array() {
                    bytes
                        .iter()
                        .map(|byte| byte.as_u64().unwrap() as u8)
                        .collect()
                } else {
                    expand(step["text"].as_str().unwrap()).into_bytes()
                };
                assert_eq!(stored, expected, "{name}");
                let observation = match std::str::from_utf8(&stored) {
                    Ok(text) => serde_json::json!({"stored_text":text}),
                    Err(_) => serde_json::json!({"stored_bytes":stored}),
                };
                observed.insert(name.to_string(), observation);
            }
            "directory_empty" => {
                let leftovers: Vec<String> = std::fs::read_dir(path())
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect();
                assert!(leftovers.is_empty(), "{name}: {leftovers:?}");
                observed.insert(name.to_string(), serde_json::json!({"empty":true}));
            }
            "absent_prefix" => {
                let survivors: Vec<String> = std::fs::read_dir(path())
                    .unwrap()
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .filter(|file| file.starts_with(step["prefix"].as_str().unwrap()))
                    .collect();
                assert!(survivors.is_empty(), "{name}: {survivors:?}");
                observed.insert(name.to_string(), serde_json::json!({"absent":true}));
            }
            "sqlite_rows" => {
                let connection = rusqlite::Connection::open(path()).unwrap();
                let mut statement = connection
                    .prepare(&expand(step["sql"].as_str().unwrap()))
                    .unwrap();
                let columns = statement.column_count();
                let mut rows = Vec::new();
                let mut found = statement.query([]).unwrap();
                while let Some(row) = found.next().unwrap() {
                    let mut values = Vec::with_capacity(columns);
                    for column in 0..columns {
                        values.push(match row.get_ref(column).unwrap() {
                            rusqlite::types::ValueRef::Null => Value::Null,
                            rusqlite::types::ValueRef::Integer(value) => {
                                serde_json::json!({"integer":value})
                            }
                            rusqlite::types::ValueRef::Real(value) => {
                                serde_json::json!({"real":value})
                            }
                            rusqlite::types::ValueRef::Text(value) => serde_json::json!(
                                {"text":String::from_utf8(value.to_vec()).unwrap().replace(work, "$work")}
                            ),
                            rusqlite::types::ValueRef::Blob(value) => {
                                serde_json::json!({"blob":value.len()})
                            }
                        });
                    }
                    rows.push(Value::Array(values));
                }
                observed.insert(name.to_string(), serde_json::json!({"rows":rows}));
            }
            "run" | "command" | "git" => {
                let program = if step["action"] == "git" {
                    "git".to_string()
                } else {
                    expand(
                        step["program"]
                            .as_str()
                            .unwrap_or(env!("CARGO_BIN_EXE_ryii")),
                    )
                };
                let mut command = Command::new(program);
                command.args(
                    step[if step["action"] == "git" {
                        "args"
                    } else {
                        "arguments"
                    }]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|argument| expand(argument.as_str().unwrap())),
                );
                if let Some(environment) = step["environment"].as_object() {
                    for (key, value) in environment {
                        command.env(key, expand(value.as_str().unwrap()));
                    }
                }
                if let Some(cwd) = step["cwd"].as_str() {
                    command.current_dir(expand(cwd));
                }
                if let Some(keys) = step["env_remove"].as_array() {
                    for key in keys {
                        command.env_remove(key.as_str().unwrap());
                    }
                }
                let started = std::time::Instant::now();
                let output = command.output().unwrap();
                let elapsed = started.elapsed();
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                assert_eq!(
                    output.status.success(),
                    step["success"]
                        .as_bool()
                        .unwrap_or_else(|| step["expect_exit_code"].as_i64().unwrap_or(0) == 0),
                    "{name}: {stderr}"
                );
                if let Some(code) = step["expect_exit_code"].as_i64() {
                    assert_eq!(
                        output.status.code(),
                        Some(code as i32),
                        "{name}: {stdout}\n{stderr}"
                    );
                }
                if step["action"] == "git" {
                    continue;
                }
                outputs.insert(name.to_string(), (stdout.clone(), stderr.clone()));
                if let Some(target) = step["stdout_to"].as_str() {
                    std::fs::write(expand(target), &stdout).unwrap();
                }
                let mut result = serde_json::json!({"exit_code":output.status.code(),"stdout":stdout,"stderr":stderr});
                if let Some(limit) = step["maximum_seconds"].as_u64() {
                    let within_budget = elapsed.as_secs() < limit;
                    assert!(within_budget, "{name}: exceeded {limit}s: {elapsed:?}");
                    result["within_budget"] = Value::Bool(within_budget);
                }
                observed.insert(name.to_string(), result);
            }
            "compare" => {
                let left = &outputs[step["left"].as_str().unwrap()].0;
                let mut projected = match step["projection"].as_str() {
                    Some("prefix") | Some("plan_lines") => left
                        .lines()
                        .filter(|line| {
                            if step["projection"] == "prefix" {
                                line.starts_with(step["prefix"].as_str().unwrap())
                            } else {
                                line.starts_with("plan ") || line.starts_with("  ")
                            }
                        })
                        .map(|line| format!("{line}\n"))
                        .collect(),
                    Some("ported") => crate::v6_only::ported(left),
                    Some("written") => left
                        .lines()
                        .filter(|line| crate::v6_only::is_written_syntax_row(line))
                        .map(|line| format!("{line}\n"))
                        .collect(),
                    Some("resolved") => left
                        .lines()
                        .filter(|line| {
                            !crate::v6_only::is_written_syntax_row(line)
                                && !["symbol", "occurrence", "local", "free_name"]
                                    .iter()
                                    .any(|kind| line.contains(&format!("\"record\":\"{kind}\"")))
                        })
                        .map(|line| format!("{line}\n"))
                        .collect(),
                    Some("index_evidence") => left
                        .lines()
                        .map(|line| match line.find(",\"index_mtime_unix_ms\":") {
                            Some(start) if line.starts_with("{\"record\":\"scip_index\",") => {
                                format!("{}}}\n", &line[..start])
                            }
                            _ => format!("{line}\n"),
                        })
                        .collect(),
                    _ => left.clone(),
                };
                if let Some(replacements) = step["replace"].as_object() {
                    for (from, to) in replacements {
                        projected = projected.replace(from, to.as_str().unwrap());
                    }
                }
                let expected = if let Some(file) = step["fixture"].as_str() {
                    std::fs::read_to_string(file).unwrap()
                } else if let Some(text) = step["text"].as_str() {
                    text.to_string()
                } else {
                    let stream = &outputs[step["right"].as_str().unwrap()].0;
                    if step["projection"] == "plan_lines" {
                        stream
                            .lines()
                            .filter(|line| line.starts_with("plan ") || line.starts_with("  "))
                            .map(|line| format!("{line}\n"))
                            .collect()
                    } else if step["right_projection"] == "ported" {
                        crate::v6_only::ported(stream)
                    } else {
                        stream.clone()
                    }
                };
                assert_eq!(projected, expected, "{name}");
                observed.insert(name.to_string(), serde_json::json!({"equal":true}));
            }
            "process_dead" => {
                let pid = std::fs::read_to_string(path()).unwrap().trim().to_string();
                let alive = Command::new("kill")
                    .args(["-0", &pid])
                    .output()
                    .unwrap()
                    .status
                    .success();
                if alive {
                    let _ = Command::new("kill").args(["-9", &pid]).status();
                }
                assert!(!alive, "{name}: grandchild {pid} survived");
                observed.insert(name.to_string(), serde_json::json!({"alive":alive}));
            }
            "api" => {
                observed.insert(
                    name.to_string(),
                    replace_strings(
                        &api(&replace_strings(step, &[("$work", work)])),
                        &[(work, "$work")],
                    ),
                );
            }
            _ => panic!("unknown fixture action: {step}"),
        }
    }
    for step in case["steps"].as_array().unwrap() {
        if let Some(redactions) = step["stdout_redactions"].as_array() {
            let name = step["capture"]
                .as_str()
                .or_else(|| step["name"].as_str())
                .unwrap();
            let result = observed.get_mut(name).unwrap();
            let mut text = result["stdout"].as_str().unwrap().to_string();
            for redaction in redactions {
                text = regex::Regex::new(redaction["pattern"].as_str().unwrap())
                    .unwrap()
                    .replace_all(&text, redaction["replacement"].as_str().unwrap())
                    .into_owned();
            }
            result["stdout"] = Value::String(text);
        }
    }
    let raw: std::collections::BTreeSet<String> = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["raw_stdout"].as_bool().unwrap_or(false))
        .map(|step| {
            step["capture"]
                .as_str()
                .or_else(|| step["name"].as_str())
                .unwrap_or("setup")
                .to_string()
        })
        .collect();
    let unasserted_stderr: std::collections::BTreeSet<_> = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["unasserted_stderr"] == true)
        .map(|step| {
            step["capture"]
                .as_str()
                .or_else(|| step["name"].as_str())
                .unwrap()
        })
        .collect();
    // Keep record order and every stable field. Per-case exclusions remove only
    // timestamp/version evidence that the old test explicitly left unpinned.
    for (name, result) in observed.iter_mut() {
        if let Some(stream) = result["stdout"].as_str() {
            if raw.contains(name) {
                result["stdout"] = Value::String(stream.replace(work, "$work"));
            } else {
                let mut records = Vec::new();
                for line in stream.lines() {
                    let mut row: Value =
                        serde_json::from_str(&line.replace(work, "$work")).unwrap();
                    if row["record"] == "scip_index" {
                        if let Some(fields) = case["unasserted_index_fields"].as_array() {
                            for field in fields {
                                row.as_object_mut().unwrap().remove(field.as_str().unwrap());
                            }
                        }
                    }
                    records.push(row);
                }
                result["stdout"] = Value::Array(records);
            }
            if unasserted_stderr.contains(name.as_str()) {
                result.as_object_mut().unwrap().remove("stderr");
            } else {
                result["stderr"] =
                    Value::String(result["stderr"].as_str().unwrap().replace(work, "$work"));
            }
        }
    }
    replace_strings(&serde_json::json!(observed), &[(work, "$work")])
}

fn copy_tree(source: &Path, target: &Path) {
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let to = target.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &to);
        } else {
            std::fs::copy(entry.path(), to).unwrap();
        }
    }
}

fn claim(
    step: &Value,
    observed: &BTreeMap<String, Value>,
    expand: &impl Fn(&str) -> String,
) -> Value {
    let cell = |key: &str| {
        let key = step[key].as_str().unwrap();
        let (name, field) = key
            .rsplit_once('.')
            .filter(|(_, field)| ["stdout", "stderr"].contains(field))
            .unwrap_or((key, "stdout"));
        let value = &observed[name];
        if value.is_object() {
            value[field].clone()
        } else {
            value.clone()
        }
    };
    let kind = step["kind"].as_str().unwrap();
    if kind == "cells_equal" {
        let actual = cell("left") == cell("right");
        assert!(actual, "{step}");
        return serde_json::json!(actual);
    }
    let value = cell("cell");
    let text = value.as_str().unwrap_or("");
    let needle = || expand(step["text"].as_str().unwrap());
    let (actual, expected) = match kind {
        "contains" => (
            serde_json::json!(text.contains(&needle())),
            Value::Bool(true),
        ),
        "absent" => (
            serde_json::json!(!text.contains(&needle())),
            Value::Bool(true),
        ),
        "any_contains" => (
            serde_json::json!(step["texts"]
                .as_array()
                .unwrap()
                .iter()
                .any(|needle| text.contains(&expand(needle.as_str().unwrap())))),
            Value::Bool(true),
        ),
        "text_equals" => (value.clone(), Value::String(needle())),
        "count_substring" => (
            serde_json::json!(text.matches(&needle()).count()),
            step["expect"].clone(),
        ),
        "line_count" => (
            serde_json::json!(text
                .lines()
                .filter(|line| line.starts_with(step["line_prefix"].as_str().unwrap()))
                .count()),
            step["expect"].clone(),
        ),
        "line_equals" => {
            let mut line = text
                .lines()
                .nth(step["line"].as_u64().unwrap() as usize)
                .unwrap()
                .to_string();
            if let Some(replacements) = step["replace"].as_object() {
                for (from, to) in replacements {
                    line = line.replace(&expand(from), to.as_str().unwrap());
                }
            }
            (Value::String(line), Value::String(needle()))
        }
        "list_contains" | "list_absent" => {
            let present = value.as_array().unwrap().contains(&step["value"]);
            (Value::Bool(present), Value::Bool(kind == "list_contains"))
        }
        _ if kind.starts_with("json_") => {
            let row: Value = serde_json::from_str(
                text.lines()
                    .find(|line| line.starts_with(step["line_prefix"].as_str().unwrap_or("{")))
                    .unwrap(),
            )
            .unwrap();
            let selected = row
                .pointer(&format!(
                    "/{}",
                    step["pointer"].as_str().unwrap().trim_start_matches('/')
                ))
                .unwrap();
            let actual = match kind {
                "json_column" => Value::Array(
                    selected
                        .as_array()
                        .unwrap()
                        .iter()
                        .map(|row| row[step["column"].as_str().unwrap()].clone())
                        .collect(),
                ),
                "json_len" => serde_json::json!(selected.as_array().unwrap().len()),
                "json_column_absent" => Value::Bool(
                    !selected
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|row| row[step["column"].as_str().unwrap()] == step["value"]),
                ),
                "json_scalar" | "json_equals" => selected.clone(),
                _ => panic!("unknown claim: {step}"),
            };
            let expected = if kind == "json_column_absent" {
                Value::Bool(true)
            } else {
                step["expect"].clone()
            };
            (actual, expected)
        }
        _ => panic!("unknown claim: {step}"),
    };
    assert_eq!(actual, expected, "{step}");
    actual
}

/// Resolve fixture paths through the library, retaining the span-independent
/// edge tuple consumed by mutation and relocation contracts.
pub fn resolved_rows(step: &Value) -> Value {
    use sprefa_extract::{resolve_project, ResolveArms, ResolveRequest};
    let root = Path::new(step["root"].as_str().unwrap());
    let paths: Vec<_> = step["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|file| root.join(file.as_str().unwrap()))
        .collect();
    let facts = resolve_project(&ResolveRequest {
        paths: &paths,
        arms: ResolveArms {
            call: true,
            types: true,
            flow: false,
        },
        scip: Default::default(),
        project_root: None,
        scip_records: Default::default(),
        occurrence_text: false,
        rust_checker: None,
        ts_checker: None,
        go_checker: None,
        witness: false,
    })
    .unwrap();
    let mut calls = Vec::new();
    let mut types = Vec::new();
    let call_fields = [
        "caller_path",
        "caller_name",
        "callee_path",
        "callee_name",
        "kind",
        "origin",
    ];
    let type_fields = [
        "owner_path",
        "owner_name",
        "target_path",
        "target_name",
        "kind",
        "origin",
    ];
    for fact in facts {
        let value = serde_json::to_value(fact).unwrap();
        let (fields, output) = match value["record"].as_str() {
            Some("resolved_edge") => (&call_fields, &mut calls),
            Some("resolved_type_edge") => (&type_fields, &mut types),
            _ => continue,
        };
        let mut row = serde_json::Map::new();
        for field in fields {
            let source = if *field == "origin" {
                "resolution_origin"
            } else {
                field
            };
            let mut cell = value[source].clone();
            if field.ends_with("_path") {
                let path = cell.as_str().unwrap();
                cell = Path::new(path)
                    .strip_prefix(root)
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| path.to_string())
                    .into();
            }
            row.insert(field.to_string(), cell);
        }
        output.push(Value::Object(row));
    }
    for (rows, fields) in [(&mut calls, call_fields), (&mut types, type_fields)] {
        rows.sort_by_key(|row| {
            fields
                .iter()
                .map(|field| row[*field].as_str().map(str::to_string))
                .collect::<Vec<_>>()
        });
        rows.dedup();
    }
    serde_json::json!({"calls":calls,"types":types})
}

fn tree_contents(root: &Path) -> BTreeMap<String, Value> {
    fn read(root: &Path, path: &Path, files: &mut BTreeMap<String, Value>) {
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.insert(
                    format!("{}/", path.strip_prefix(root).unwrap().to_string_lossy()),
                    serde_json::json!({"directory":true}),
                );
                read(root, &path, files);
            } else {
                let bytes = std::fs::read(&path).unwrap();
                let value = match String::from_utf8(bytes) {
                    Ok(text) => Value::String(text),
                    Err(error) => serde_json::json!({"bytes":error.into_bytes()}),
                };
                files.insert(
                    path.strip_prefix(root)
                        .unwrap()
                        .to_string_lossy()
                        .into_owned(),
                    value,
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    read(root, root, &mut files);
    files
}

pub fn editing_api(step: &Value) -> Value {
    use sprefa_extract::ScipSource;
    match step["api"].as_str().unwrap() {
        "source_absent" => {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(step["path"].as_str().unwrap());
            let text = std::fs::read_to_string(path).unwrap();
            let needle = step["needle"].as_str().unwrap();
            let hits: Vec<_> = text
                .lines()
                .enumerate()
                .filter(|(_, line)| line.contains(needle))
                .collect();
            assert!(hits.is_empty(), "{step}: {hits:?}");
            serde_json::json!({"path":step["path"],"needle":needle,"hits":hits})
        }
        "scip_index" => {
            let root = Path::new(step["root"].as_str().unwrap());
            let index = sprefa_extract::ScipTypescript.build(root).unwrap();
            let target = Path::new(step["into"].as_str().unwrap());
            std::fs::copy(index, target).unwrap();
            serde_json::json!({"index":target.to_str().unwrap(),"is_file":target.is_file()})
        }
        _ => panic!("unknown editing API: {step}"),
    }
}
