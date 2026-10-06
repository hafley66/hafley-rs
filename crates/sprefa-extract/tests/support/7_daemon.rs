use crate::daemon_guard::DaemonGuard;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Default)]
struct State {
    daemons: BTreeMap<String, DaemonGuard>,
    pids: BTreeMap<String, i32>,
    inodes: BTreeMap<String, u64>,
}

pub fn evaluate(case: &Value) -> Value {
    let state = RefCell::new(State::default());
    crate::fixture_runner::commands(case, |step| api(step, &mut state.borrow_mut()))
}

fn api(step: &Value, state: &mut State) -> Value {
    let result = match step["api"].as_str().unwrap() {
        "guard" => {
            let cache = step["cache"].as_str().unwrap();
            let guard = state.daemons.entry(cache.to_owned()).or_insert_with(|| DaemonGuard::new(Path::new(cache)));
            match step["operation"].as_str().unwrap() {
                "register" => Value::Null,
                "record_inode" => {
                    state.inodes.insert(cache.to_owned(), std::fs::metadata(guard.socket()).unwrap().ino());
                    json!({"inode_recorded":true})
                }
                "stop" => json!({"stopped":guard.stop()}),
                operation => {
                    if operation.starts_with("wait_") {
                        let until = Instant::now() + Duration::from_secs(step["seconds"].as_u64().unwrap());
                        while (if operation == "wait_pid" { guard.pid().is_none() } else { guard.socket().exists() }) && Instant::now() < until {
                            std::thread::sleep(Duration::from_millis(step["poll_ms"].as_u64().unwrap()));
                        }
                    }
                    if let Some(pid) = guard.pid() { state.pids.insert(cache.to_owned(), pid); }
                    let mut value = json!({
                        "socket_exists":guard.socket().exists(),
                        "has_pid":guard.pid().is_some(),
                        "alive":guard.pid().is_some_and(DaemonGuard::alive),
                        "recorded_pid_alive":state.pids.get(cache).is_some_and(|pid| DaemonGuard::alive(*pid)),
                        "socket_within_limit":guard.socket().as_os_str().len() <= 100,
                        "inode_changed":state.inodes.get(cache).is_some_and(|old| std::fs::metadata(guard.socket()).ok().is_some_and(|metadata| metadata.ino() != *old)),
                    });
                    value.as_object_mut().unwrap().retain(|field, _| step["fields"].as_array().unwrap().iter().any(|name| name == field));
                    value
                }
            }
        }
        "advance_mtime" => {
            let file = std::fs::OpenOptions::new().write(true).open(step["path"].as_str().unwrap()).unwrap();
            let delta = Duration::from_secs(step["seconds"].as_u64().unwrap());
            file.set_times(std::fs::FileTimes::new().set_modified(file.metadata().unwrap().modified().unwrap() + delta)).unwrap();
            json!({"advanced_seconds":delta.as_secs()})
        }
        "trace_requests" => {
            let timeline: Value = serde_json::from_slice(&std::fs::read(step["path"].as_str().unwrap()).unwrap()).unwrap();
            let root = std::env::current_dir().unwrap().to_string_lossy().into_owned();
            let requests: HashSet<_> = timeline.as_array().unwrap().iter()
                .filter(|e| e["name"] == "daemon_request" && e["ph"] == "E" && e["args"]["verb"] == step["verb"] && e["args"]["request_root"] == root)
                .filter_map(|e| e["args"]["request_id"].as_u64().or_else(|| e["args"]["request_id"].as_str()?.parse().ok())).collect();
            json!({"request_spans":requests.len()})
        }
        "parallel_clients" => {
            let commands = step["commands"].as_array().unwrap();
            let expected = std::fs::read(step["stdout_file"].as_str().unwrap()).unwrap();
            let barrier = std::sync::Barrier::new(commands.len() + 1);
            let outputs = std::thread::scope(|scope| {
                let workers: Vec<_> = commands.iter().map(|command| {
                    let barrier = &barrier;
                    scope.spawn(move || { barrier.wait(); crate::command_support::execute(command, &|text| crate::fixture_runner::expand_text(text, "")).0 })
                }).collect();
                barrier.wait();
                workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>()
            });
            json!({"clients":outputs.iter().map(|o| json!({"success":o.status.success(),"matches_direct":o.stdout == expected})).collect::<Vec<_>>()})
        }
        other => panic!("unknown daemon api: {other}"),
    };
    if let Some(expected) = step.get("expect") { assert_eq!(&result, expected, "{step}"); }
    result
}
