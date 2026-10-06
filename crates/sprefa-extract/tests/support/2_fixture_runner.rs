use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;

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
                observed.insert(name.to_string(), api(step));
            }
            _ => panic!("unknown fixture action: {step}"),
        }
    }
    // Keep record order and every stable field. Per-case exclusions remove only
    // timestamp/version evidence that the old test explicitly left unpinned.
    for result in observed.values_mut() {
        if let Some(stream) = result["stdout"].as_str() {
            let mut records = Vec::new();
            for line in stream.lines() {
                let mut row: Value = serde_json::from_str(&line.replace(work, "$work")).unwrap();
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
            result["stderr"] =
                Value::String(result["stderr"].as_str().unwrap().replace(work, "$work"));
        }
    }
    serde_json::json!(observed)
}
