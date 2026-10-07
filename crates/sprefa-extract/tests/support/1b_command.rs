use serde_json::Value;
use std::process::{Command, Output};
use std::time::Duration;

pub fn execute(step: &Value, expand: &impl Fn(&str) -> String) -> (Output, Duration) {
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
    let controlled = step.get("stdin").is_some() || step.get("stdin_file").is_some()
        || step.get("close_stdout_after").is_some() || step.get("hard_deadline_ms").is_some();
    let output = if controlled {
        use std::io::{Read, Write};
        use std::process::Stdio;
        command.stdin(Stdio::piped()).stdout(if step["stdout_null"] == true { Stdio::null() } else { Stdio::piped() }).stderr(Stdio::piped());
        let mut child = command.spawn().unwrap();
        if step.get("stdin").is_some() || step.get("stdin_file").is_some() {
            let input = if let Some(file) = step["stdin_file"].as_str() { std::fs::read(expand(file)).unwrap() }
                else { expand(step["stdin"].as_str().unwrap()).into_bytes() };
            child.stdin.take().unwrap().write_all(&input).unwrap();
        } else { drop(child.stdin.take()); }
        if let Some(count) = step["close_stdout_after"].as_u64() {
            let mut first = vec![0; count as usize];
            child.stdout.take().unwrap().read_exact(&mut first).unwrap();
        }
        if let Some(limit) = step["hard_deadline_ms"].as_u64().filter(|_| crate::wall_bench::enabled()) {
            while child.try_wait().unwrap().is_none() {
                if crate::wall_bench::expired(started, Duration::from_millis(limit)) {
                    let _ = child.kill(); let _ = child.wait();
                    panic!("command still running after {limit}ms: {step}");
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        child.wait_with_output().unwrap()
    } else { command.output().unwrap() };
    let elapsed = started.elapsed();
    (output, elapsed)
}
