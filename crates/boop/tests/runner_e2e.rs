use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use anyhow::Result;
use boop::runner::{answer_pending, open_feed, read_answered, Resident, ResidentReply, UdsClient};

#[derive(Default)]
struct EchoResident {
    prompts: Vec<String>,
}

impl Resident for EchoResident {
    fn ask(&mut self, prompt: &str) -> Result<ResidentReply> {
        self.prompts.push(prompt.to_owned());
        Ok(ResidentReply {
            turn: self.prompts.len() as i64,
            text: format!("echo {prompt}"),
        })
    }
}

fn socket_path() -> PathBuf {
    std::env::temp_dir().join(format!(
        "boop-runner-e2e-{}-{}.sock",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

fn request(stream: &mut std::os::unix::net::UnixStream) -> (String, Vec<u8>) {
    let mut raw = Vec::new();
    let mut buffer = [0_u8; 1024];
    loop {
        let read = stream.read(&mut buffer).unwrap();
        raw.extend_from_slice(&buffer[..read]);
        if raw.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    let head = String::from_utf8(raw[..split].to_vec()).unwrap();
    let length = head
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    while raw.len() < split + length {
        let read = stream.read(&mut buffer).unwrap();
        raw.extend_from_slice(&buffer[..read]);
    }
    (
        head.lines().next().unwrap().to_owned(),
        raw[split..split + length].to_vec(),
    )
}

fn reply(stream: &mut std::os::unix::net::UnixStream, body: serde_json::Value) {
    let body = body.to_string();
    write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
    .unwrap();
}

#[test]
fn uds_engine_arrivals_are_serial_and_restart_skips_answered_rows() {
    let socket = socket_path();
    let listener = UnixListener::bind(&socket).unwrap();
    let arrivals = Arc::new(Mutex::new(Vec::<serde_json::Value>::new()));
    let server_arrivals = arrivals.clone();
    let server = thread::spawn(move || {
        for _ in 0..6 {
            let (mut stream, _) = listener.accept().unwrap();
            let (line, body) = request(&mut stream);
            if line.starts_with("POST /arrive ") {
                server_arrivals
                    .lock()
                    .unwrap()
                    .extend(serde_json::from_slice::<Vec<serde_json::Value>>(&body).unwrap());
                reply(&mut stream, serde_json::json!({"tick": 10}));
            } else if line.starts_with("GET /rel/resident ") {
                let rows = server_arrivals
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|arrival| arrival["rel"] == "resident")
                    .map(|arrival| {
                        serde_json::json!({
                            "session": arrival["row"][0],
                            "user_run": arrival["row"][1],
                        })
                    })
                    .collect::<Vec<_>>();
                reply(&mut stream, serde_json::json!({"rows": rows}));
            } else {
                reply(
                    &mut stream,
                    serde_json::json!({
                        "tick": 9,
                        "add": [["source", 7, "seven"], ["source", 2, "two"], ["source", 5, "five"]],
                        "del": []
                    }),
                );
            }
        }
    });

    let client = UdsClient::new(socket.clone());
    let mut feed = open_feed(&client).unwrap();
    let mut resident = EchoResident::default();
    let mut answered = BTreeSet::new();
    assert_eq!(
        answer_pending(&client, feed.as_mut(), &mut resident, &mut answered).unwrap(),
        3
    );
    assert_eq!(resident.prompts, ["two", "five", "seven"]);

    let mut restarted = read_answered(&client).unwrap();
    let mut restart_feed = open_feed(&client).unwrap();
    assert_eq!(
        answer_pending(
            &client,
            restart_feed.as_mut(),
            &mut resident,
            &mut restarted
        )
        .unwrap(),
        0
    );
    assert_eq!(resident.prompts.len(), 3);

    server.join().unwrap();
    let rows = arrivals.lock().unwrap();
    let resident_rows = rows
        .iter()
        .filter(|arrival| arrival["rel"] == "resident")
        .collect::<Vec<_>>();
    assert_eq!(resident_rows.len(), 3);
    assert_eq!(
        resident_rows
            .iter()
            .map(|arrival| arrival["row"][1].as_i64().unwrap())
            .collect::<Vec<_>>(),
        [2, 5, 7]
    );
    std::fs::remove_file(socket).unwrap();
}
