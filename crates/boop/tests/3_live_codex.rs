//! Opt-in authenticated coverage against a task-owned, wrapped Codex TUI.
//! The manifest comes from an actual launch receipt, never a transcript fixture.

use std::path::Path;
use boop::{bus, mail, registry::Registry, Store};

#[test]
#[ignore = "requires a live wrapped Codex TUI and BOOP_LIVE_CODEX_MANIFEST"]
fn authenticated_wrapped_codex_retries_preserve_one_transcript_receipt() {
    let manifest = std::env::var("BOOP_LIVE_CODEX_MANIFEST").expect("path to the test-owned launch manifest");
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(&manifest).unwrap()).unwrap();
    assert_eq!(fixture["test_owned"], true);
    let store = Store::open(fixture["database"].as_str().unwrap().into()).unwrap();
    let routes = bus::routes_in(&store).unwrap();
    let name = fixture["route"].as_str().unwrap();
    let route = &routes[name];
    assert_eq!(route.kind, "coordinator");
    assert_eq!(route.mode.as_deref(), Some("native-owned"));
    assert_eq!(route.session_id.as_deref(), fixture["thread"].as_str());
    assert!(Path::new(route.app_server_socket.as_deref().unwrap()).exists());
    let live = store.live_row(route.session_id.as_deref().unwrap()).unwrap().unwrap();
    assert!(live.pid.is_some_and(|pid| boop::live::pid_alive(pid as u32)));
    let message = bus::messages_in(&store).unwrap().into_iter()
        .find(|message| message.id == fixture["message_id"].as_str().unwrap()).unwrap();
    assert_eq!(message.to, name);
    assert!(store.delivery_accepted(&message.id, name).unwrap());
    let registry = Registry::discover();
    let adapter = registry.get(route.harness.unwrap());
    let session = adapter.session_by_id(route.session_id.as_deref().unwrap(), route.cwd.as_deref()).expect("observed native session");
    let read = || {
        let rows = adapter.messages(&session, None);
        let users = rows.iter().filter(|row| row.role == "user" && row.text == message.body).count();
        let answers = rows.iter().filter(|row| row.role == "assistant" && row.text == fixture["answer"].as_str().unwrap()).count();
        (users, answers)
    };
    let before = read();
    assert_eq!(before, (1, 1));
    for _ in 0..2 {
        let landing = mail::deliver_hail(&registry, &store, &routes, &message).unwrap();
        assert_eq!(landing.rung, mail::Rung::AlreadyAccepted);
    }
    std::thread::sleep(std::time::Duration::from_secs(5));
    let after = read();
    assert_eq!(after, before);
    let receipt = serde_json::json!({"case":"authenticated wrapped Codex retry", "status":"PASS",
        "manifest":manifest, "route":name,"thread":route.session_id,"message_id":message.id,
        "before":before,"after":after,"attempts_after_acceptance":2});
    std::fs::write(fixture["receipt"].as_str().unwrap(), serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
}
