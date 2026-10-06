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
    let output = if step.get("stdin").is_some() || step.get("stdin_file").is_some() {
        use std::io::Write;
        let input = if let Some(file) = step["stdin_file"].as_str() {
            std::fs::read(expand(file)).unwrap()
        } else { expand(step["stdin"].as_str().unwrap()).into_bytes() };
        command.stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
        let mut child = command.spawn().unwrap();
        child.stdin.take().unwrap().write_all(&input).unwrap();
        child.wait_with_output().unwrap()
    } else { command.output().unwrap() };
    let elapsed = started.elapsed();
    (output, elapsed)
}
