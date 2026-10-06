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
    let work = temporary.path().to_str().unwrap();
    let expand = |text: &str| text.replace("$work", work);
    let mut outputs = BTreeMap::<String, (String, String)>::new();
    let mut observed = BTreeMap::new();
    for step in case["steps"].as_array().unwrap() {
        let name = step["name"].as_str().unwrap_or("setup");
        let path = || std::path::PathBuf::from(expand(step["path"].as_str().unwrap()));
        match step["action"].as_str().unwrap() {
            "directory" => std::fs::create_dir_all(path()).unwrap(),
            "copy" => {
                let target = path();
                std::fs::create_dir_all(target.parent().unwrap()).unwrap();
                std::fs::copy(step["source"].as_str().unwrap(), target).unwrap();
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
            "run" => {
                let mut command = Command::new(env!("CARGO_BIN_EXE_ryii"));
                command.args(
                    step["arguments"]
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
                let started = std::time::Instant::now();
                let output = command.output().unwrap();
                let elapsed = started.elapsed();
                let stdout = String::from_utf8(output.stdout).unwrap();
                let stderr = String::from_utf8(output.stderr).unwrap();
                assert_eq!(
                    output.status.success(),
                    step["success"].as_bool().unwrap_or(true),
                    "{name}: {stderr}"
                );
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
                } else {
                    let stream = &outputs[step["right"].as_str().unwrap()].0;
                    if step["right_projection"] == "ported" {
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
    let raw: std::collections::BTreeSet<String> = case["steps"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|step| step["raw_stdout"].as_bool().unwrap_or(false))
        .map(|step| step["name"].as_str().unwrap_or("setup").to_string())
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
            result["stderr"] =
                Value::String(result["stderr"].as_str().unwrap().replace(work, "$work"));
        }
    }
    serde_json::json!(observed)
}
