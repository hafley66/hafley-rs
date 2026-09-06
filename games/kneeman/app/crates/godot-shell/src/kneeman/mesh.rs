//! Mesh netplay (WIRE PROTOCOL v2): k-player (k <= `crate::sim::MAX_PLAYERS`) signaling + session
//! orchestration. Lives here — not in `session.rs`/`mod.rs` — purely because those two files sit at
//! their `.dl/lint-file-budget.dl` R5 ratchet caps; new logic goes in new modules instead of
//! regrowing them (see that file's own comment). `session.rs`/`mod.rs` keep thin delegating
//! wrappers for the handful of `#[func]`/lifecycle methods that used to hold this code inline.
//!
//! k<=2 reproduces the ORIGINAL single-peer host/guest handshake byte-for-byte (same wire shape, no
//! "to"/"from"/"handle"/"count" fields sent, same gating) — see `legacy_connect`/`begin_session`'s
//! early branch. k>2 is new: every unordered pair `(i, j)`, `i < j`, gets its OWN
//! `WebRtcPeerConnection` + negotiated data channel id 1 (per-connection, so id 1 can't collide
//! across pairs); `rtc::is_initiator` says which side of the pair creates the offer.
//!
//! `PeerAddr` (this shell always builds `smash_net` without the `matchbox` feature) is a plain
//! `usize`, and we choose to make it exactly equal to the ggrs handle — so `MeshSocket` below needs
//! no separate address<->handle table, just an array indexed by handle.

use godot::classes::web_rtc_data_channel::ChannelState;
use godot::classes::web_rtc_peer_connection::ConnectionState;
use godot::classes::{WebRtcDataChannel, WebRtcPeerConnection};
use godot::prelude::*;

use crate::sim::net::{
    GgrsNetplay, Message, NetplayEvent, NonBlockingSocket, Smash, SmashGame, start_p2p, start_p2p_n,
};

use crate::identity::{Identity, slot_color};
use crate::rtc::{self, Role, RtcSocket};
use crate::sim::{self, SimState};

use super::{KneeMan, Phase, Room};

/// How long (ms) `begin_mesh_session` waits for every OTHER peer's roster pick before giving up on
/// the missing ones and stamping the documented default (roster slot 0) instead. Both peers apply
/// the exact same rule off the exact same missing data, so frame 0 can't diverge over it.
const MESH_PICK_TIMEOUT_MS: u64 = 5_000;

/// One pair's live connection, keyed by the REMOTE handle (see `MeshSession::links`).
struct PeerLink {
    pc: Gd<WebRtcPeerConnection>,
    channel: Gd<WebRtcDataChannel>,
    channel_was_open: bool, // edge-detect -> the one-shot "peer_channel_open" analytics line
}

/// k>2 mesh state, boxed behind `KneeMan::mesh` (only allocated once a "matched" frame reports a
/// party > 2, so the k<=2 path — still `KneeMan::pc`/`channel` directly — pays nothing extra).
#[derive(Default)]
pub(super) struct MeshSession {
    links: [Option<PeerLink>; sim::MAX_PLAYERS], // this peer's own handle slot stays None
    picks: [Option<usize>; sim::MAX_PLAYERS],    // roster pick each OTHER handle sent us
    deadline_ms: u64,                            // see MESH_PICK_TIMEOUT_MS
}

/// ggrs `NonBlockingSocket<usize>` over a fixed handle -> channel map. `addr` IS the remote's ggrs
/// handle (see module doc), so send/receive just index by it directly.
pub(super) struct MeshSocket {
    local: usize,
    channels: [Option<Gd<WebRtcDataChannel>>; sim::MAX_PLAYERS],
}

impl NonBlockingSocket<usize> for MeshSocket {
    fn send_to(&mut self, msg: &Message, addr: &usize) {
        let Some(ch) = self.channels[*addr].as_mut() else {
            return;
        };
        let bytes = bincode::serialize(msg).expect("serialize ggrs message");
        ch.put_packet(&PackedByteArray::from(bytes.as_slice()));
    }

    fn receive_all_messages(&mut self) -> Vec<(usize, Message)> {
        let mut out = Vec::new();
        for (h, ch) in self.channels.iter_mut().enumerate() {
            if h == self.local {
                continue;
            }
            let Some(ch) = ch.as_mut() else { continue };
            for _ in 0..ch.get_available_packet_count() {
                let packet = ch.get_packet();
                if let Ok(msg) = bincode::deserialize::<Message>(packet.as_slice()) {
                    out.push((h, msg));
                }
            }
        }
        out
    }
}

/// `"[0,1,2,3]"` — the session_begin analytics "handles" field: every handle in this party.
fn handles_json(count: usize) -> String {
    let body: Vec<String> = (0..count).map(|h| h.to_string()).collect();
    format!("[{}]", body.join(","))
}

/// Dispatch one signaling frame from the relay (already JSON-parsed by key). Ported verbatim from
/// the pre-mesh single-peer handler for every k<=2 branch; extended for "matched"'s handle/count and
/// offer/answer/ice's to/from once a party is > 2 (WIRE PROTOCOL v2 #1-#2).
pub(super) fn handle_signal(k: &mut KneeMan, text: &GString) {
    let d = rtc::parse_json(text);
    match rtc::dget_str(&d, "kind").as_str() {
        "matched" => {
            if let Some(role) = Role::from_str(&rtc::dget_str(&d, "role")) {
                let handle = opt_int(&d, "handle");
                let count = opt_int(&d, "count");
                let (handle, count) = rtc::resolve_matched(handle, count, role);
                // Reconnect re-pair: keep the pre-drop handle/role, whatever re-dial order the
                // relay saw -- see `resolve_rematched`'s doc for the mid-match player-swap this
                // prevents.
                let (handle, role) = rtc::resolve_rematched(
                    k.phase == Phase::Reconnecting,
                    count,
                    k.local_handle,
                    (handle, role),
                );
                k.local_handle = handle;
                k.party_count = count;
                crate::analytics::log(
                    "party_matched",
                    &format!(
                        r#","room":"{}","handle":{handle},"count":{count}"#,
                        k.room.as_ref().map(|r| r.code.as_str()).unwrap_or(""),
                    ),
                );
                setup_peer(k, role);
            }
        }
        "offer" => {
            let from = resolve_from(k, &d);
            k.sig.offer_in += 1;
            k.note_peer_build(rtc::dget_str(&d, "hash"));
            note_peer_pick(k, from, &d);
            let sdp = rtc::dget_str(&d, "sdp");
            if k.party_count > 2 {
                ensure_mesh(k);
                if mesh_link_missing(k, from) {
                    create_pc_for(k, from, &rtc::ice_config(), false); // reactive: we answer, not offer
                }
                if let Some(mut pc) = mesh_pc(k, from) {
                    pc.set_remote_description("offer", &sdp);
                }
            } else if let Some(mut pc) = k.pc.clone() {
                pc.set_remote_description("offer", &sdp);
            }
        }
        "answer" => {
            let from = resolve_from(k, &d);
            k.sig.answer_in += 1;
            k.note_peer_build(rtc::dget_str(&d, "hash"));
            note_peer_pick(k, from, &d);
            let sdp = rtc::dget_str(&d, "sdp");
            if k.party_count > 2 {
                if let Some(mut pc) = mesh_pc(k, from) {
                    pc.set_remote_description("answer", &sdp);
                }
            } else if let Some(mut pc) = k.pc.clone() {
                pc.set_remote_description("answer", &sdp);
            }
        }
        "ice" => {
            let from = resolve_from(k, &d);
            k.sig.ice_in += 1;
            let media = rtc::dget_str(&d, "media");
            let index = rtc::dget_int(&d, "index") as i32;
            let name = rtc::dget_str(&d, "name");
            let pc = if k.party_count > 2 {
                mesh_pc(k, from)
            } else {
                k.pc.clone()
            };
            if let Some(mut pc) = pc {
                pc.add_ice_candidate(&media, index, &name);
            }
        }
        // Host mints the private reconnect room and relays the code (or, k>2, broadcasts it to the
        // whole party — the relay side, not this client, decides fan-out); guest(s) store it so
        // everyone re-dials the SAME room if the transport drops later. Ignore once we have one.
        "room" => {
            let code = rtc::dget_str(&d, "code");
            if !code.is_empty() && k.room.is_none() {
                k.room = Some(Room {
                    code,
                    deadline_ms: None,
                });
            }
        }
        // Reconnect resume (k==2 only — see `abort_party` for the k>2 punt): the host ships the sim
        // state to start the rebuilt session from.
        "resume" => {
            let b64 = rtc::dget_str(&d, "state");
            if let Some(snap) = crate::net::decode_state(&b64) {
                k.resume_snapshot = Some(snap);
                k.got_resume = true;
            }
        }
        // Host's authoritative ruleset, broadcast to the whole party: adopt it so every reducer runs
        // identical physics.
        "tune" => {
            let b64 = rtc::dget_str(&d, "tune");
            if let Some(t) = crate::net::decode_tune(&b64) {
                k.tune.set(t);
                k.got_tune = true;
            }
        }
        "bye" => k.reset_offline(),
        _ => {}
    }
}

/// Read an optional JSON int field, treating both "absent" and the -1 old-client sentinel as `None`
/// (matches `rtc::dget_int_or`'s established convention elsewhere in the handshake).
fn opt_int(d: &VarDictionary, key: &str) -> Option<i64> {
    let v = rtc::dget_int_or(d, key, -1);
    (v >= 0).then_some(v)
}

/// The peer handle an inbound offer/answer/ice frame concerns: the relay's "from" once stamped
/// (always true once the party is > 2, or an upgraded relay); otherwise "the other one" — valid
/// only for the pre-mesh k==2 case this falls back to.
fn resolve_from(k: &KneeMan, d: &VarDictionary) -> usize {
    let from = opt_int(d, "from");
    let fallback = if k.local_handle == 0 { 1 } else { 0 };
    rtc::resolve_peer_field(from, fallback)
}

/// Record the peer's roster pick + display identity from an offer/answer's pname/pcolor/pchar,
/// keyed by the handle it came `from` (WIRE PROTOCOL v2 #5: "track peer picks PER HANDLE"). k<=2
/// also mirrors it into the legacy single-peer fields so `render.rs`'s cosmetic name/color display
/// keeps working exactly as before. -1 pchar = an old client that didn't send one (ignored).
fn note_peer_pick(k: &mut KneeMan, from: usize, d: &VarDictionary) {
    let pchar = rtc::dget_int_or(d, "pchar", -1);
    if pchar >= 0 {
        if k.party_count > 2 {
            ensure_mesh(k);
            if let Some(mesh) = k.mesh.as_mut() {
                mesh.picks[from] = Some(pchar as usize);
            }
        } else {
            k.peer_char = Some(pchar as usize);
        }
    }
    let name = rtc::dget_str(d, "pname");
    if name.is_empty() {
        return; // old client didn't send identity; leave peer_identity/None alone
    }
    if k.party_count <= 2 {
        let color_html = rtc::dget_str(d, "pcolor");
        let color = Color::from_html(color_html.as_str())
            .unwrap_or_else(|| slot_color(1 - k.local_handle.min(1)));
        k.peer_identity = Some(Identity {
            name,
            color,
            font_px: 32,
        });
    }
    // k>2 per-peer cosmetic name/color display is a documented gap (render.rs's `player_name`/
    // `slot_tint` only know the single legacy `peer_identity` slot) -- not required by WIRE
    // PROTOCOL v2, and inert here: the pick above still drives char_id (the checksummed part).
}

fn ensure_mesh(k: &mut KneeMan) {
    if k.mesh.is_none() {
        k.mesh = Some(Box::new(MeshSession {
            deadline_ms: crate::net::now_ms() + MESH_PICK_TIMEOUT_MS,
            ..Default::default()
        }));
    }
}

fn mesh_link_missing(k: &KneeMan, peer: usize) -> bool {
    k.mesh
        .as_ref()
        .map(|m| m.links[peer].is_none())
        .unwrap_or(true)
}

fn mesh_pc(k: &KneeMan, peer: usize) -> Option<Gd<WebRtcPeerConnection>> {
    k.mesh
        .as_ref()
        .and_then(|m| m.links[peer].as_ref())
        .map(|l| l.pc.clone())
}

/// Build the peer connection(s) + negotiated data channel(s), wire the SDP/ICE signals back to the
/// `#[func]` handlers in `mod.rs` (bound with the peer handle, so one callback serves every pair).
/// The host additionally broadcasts room/resume/tune (unchanged from the pre-mesh flow, just moved
/// here); k<=2 then opens ONE connection exactly as before, k>2 opens one per pair we initiate.
pub(super) fn setup_peer(k: &mut KneeMan, role: Role) {
    k.role = Some(role);
    k.ice_typ_seen = 0;
    crate::analytics::log(
        "peer_setup",
        &format!(
            r#","role":"{role:?}","turn":{},"handle":{},"count":{}"#,
            rtc::turn_url_count(),
            k.local_handle,
            k.party_count,
        ),
    );
    let cfg = rtc::ice_config();
    crate::analytics::log(
        "ice_cfg",
        &format!(
            r#","json":{}"#,
            crate::analytics::jstr(&rtc::to_json(&cfg).to_string())
        ),
    );
    if k.local_handle == 0 && !host_broadcast(k) {
        k.reset_offline();
        return;
    }
    if k.party_count > 2 {
        ensure_mesh(k);
        let local = k.local_handle;
        let count = k.party_count;
        for h in 0..count {
            if h == local || !rtc::is_initiator(local, h) || !mesh_link_missing(k, h) {
                continue;
            }
            create_pc_for(k, h, &cfg, true);
        }
    } else {
        legacy_connect(k, role, &cfg);
    }
    godot_print!(
        "netplay: matched as {role:?} (handle {}, party {})",
        k.local_handle,
        k.party_count
    );
}

/// Host-only broadcast: mint the private reconnect room (once) or ship the resume snapshot on a
/// reconnect, then ship the authoritative Tune. Byte-for-byte the pre-mesh behavior — the relay (not
/// this client) decides whether these fan out to one guest or a whole party.
fn host_broadcast(k: &mut KneeMan) -> bool {
    if k.room.is_none() {
        let code = crate::net::mint_room_code(&k.identity.get_cloned().name);
        let mut d = VarDictionary::new();
        d.set("kind", "room");
        d.set("code", code.clone());
        if let Some(mut ws) = k.ws.clone() {
            let error = ws.send_text(&rtc::to_json(&d));
            if error != godot::global::Error::OK {
                godot_error!("netplay: room send failed: {error:?}");
                return false;
            }
        }
        k.room = Some(Room {
            code,
            deadline_ms: None,
        });
    } else if let Some(snap) = k.resume_snapshot {
        let mut d = VarDictionary::new();
        d.set("kind", "resume");
        d.set("state", crate::net::encode_state(&snap));
        if let Some(mut ws) = k.ws.clone() {
            let error = ws.send_text(&rtc::to_json(&d));
            if error != godot::global::Error::OK {
                godot_error!("netplay: resume send failed: {error:?}");
                return false;
            }
        }
    }
    let mut d = VarDictionary::new();
    d.set("kind", "tune");
    d.set("tune", crate::net::encode_tune(&k.tune.get_cloned()));
    if let Some(mut ws) = k.ws.clone() {
        let error = ws.send_text(&rtc::to_json(&d));
        if error != godot::global::Error::OK {
            godot_error!("netplay: tune send failed: {error:?}");
            return false;
        }
    }
    true
}

/// k<=2: the ORIGINAL single connection, unchanged (bound callbacks now carry the remote handle so
/// they share `on_sdp_created`/`on_ice_created` with the k>2 path, but the wire shape they send is
/// identical to before — no "to" field).
fn legacy_connect(k: &mut KneeMan, role: Role, cfg: &VarDictionary) {
    let remote = role.handles().1;
    let mut pc = WebRtcPeerConnection::new_gd();
    pc.initialize_ex().configuration(cfg).done();
    let gd = k.to_gd();
    pc.connect(
        "session_description_created",
        &Callable::from_object_method(&gd, "on_sdp_created").bind(&[(remote as i64).to_variant()]),
    );
    pc.connect(
        "ice_candidate_created",
        &Callable::from_object_method(&gd, "on_ice_created").bind(&[(remote as i64).to_variant()]),
    );
    let mut channel = pc
        .create_data_channel_ex("ggrs")
        .options(&rtc::data_channel_options())
        .done()
        .expect("create negotiated data channel");
    rtc::set_binary(&mut channel);
    k.channel = Some(channel);
    k.pc = Some(pc.clone());
    if role == Role::Host {
        pc.create_offer();
    }
}

/// k>2: one pair's connection. `initiator` says whether WE create the offer (the lower-handle side
/// per `rtc::is_initiator`) or wait to answer an inbound one.
fn create_pc_for(k: &mut KneeMan, peer: usize, cfg: &VarDictionary, initiator: bool) {
    let mut pc = WebRtcPeerConnection::new_gd();
    pc.initialize_ex().configuration(cfg).done();
    let gd = k.to_gd();
    pc.connect(
        "session_description_created",
        &Callable::from_object_method(&gd, "on_sdp_created").bind(&[(peer as i64).to_variant()]),
    );
    pc.connect(
        "ice_candidate_created",
        &Callable::from_object_method(&gd, "on_ice_created").bind(&[(peer as i64).to_variant()]),
    );
    let mut channel = pc
        .create_data_channel_ex("ggrs")
        .options(&rtc::data_channel_options())
        .done()
        .expect("create negotiated data channel");
    rtc::set_binary(&mut channel);
    if let Some(mesh) = k.mesh.as_mut() {
        mesh.links[peer] = Some(PeerLink {
            pc: pc.clone(),
            channel,
            channel_was_open: false,
        });
    }
    if initiator {
        pc.create_offer();
    }
}

/// WebRTC fired a local description (offer/answer) for the connection bound to `peer`. Set it
/// locally and relay it back through the signaling socket -- k<=2 sends the exact pre-mesh shape
/// (no "to"); k>2 stamps "to":peer so the relay routes it (WIRE PROTOCOL v2 #2).
pub(super) fn on_sdp_created(k: &mut KneeMan, sdp_type: GString, sdp: GString, peer: i64) {
    let peer = peer as usize;
    let mut pc = if k.party_count > 2 {
        let Some(pc) = mesh_pc(k, peer) else { return };
        pc
    } else {
        let Some(pc) = k.pc.clone() else { return };
        pc
    };
    pc.set_local_description(&sdp_type, &sdp);
    if sdp_type == GString::from("offer") {
        k.sig.offer_out += 1;
    } else {
        k.sig.answer_out += 1;
    }
    let mut d = VarDictionary::new();
    d.set("kind", sdp_type);
    d.set("sdp", sdp);
    d.set("hash", rtc::BUILD_HASH); // peer flags a version mismatch from this
    let id = k.identity.get_cloned();
    d.set("pname", GString::from(&id.name));
    d.set("pcolor", id.color.to_html()); // "#rrggbbaa" hex; parsed in `note_peer_pick`

    // Local player's roster pick, RESOLVED (unset -1 -> 0, roster-capped) before it rides the wire:
    // `begin_session` stamps our slot with exactly this resolution, so every peer must stamp the
    // same value (see the frame-0 desync note on the k<=2 path this was lifted from).
    let cap = crate::roster::roster().len() as i64 - 1;
    d.set("pchar", k.charsel.get_cloned()[0].clamp(0, cap));
    if k.party_count > 2 {
        d.set("to", peer as i64);
    }
    if let Some(mut ws) = k.ws.clone() {
        ws.send_text(&rtc::to_json(&d));
    }
}

/// WebRTC found a local ICE candidate for the connection bound to `peer`. Relay it, stamping "to"
/// once the party is > 2.
pub(super) fn on_ice_created(
    k: &mut KneeMan,
    media: GString,
    index: i32,
    name: GString,
    peer: i64,
) {
    k.sig.ice_out += 1;
    let cand = name.to_string();
    let typ = cand
        .split(" typ ")
        .nth(1)
        .and_then(|s| s.split(' ').next())
        .unwrap_or("?");
    let bit = match typ {
        "host" => 1u8,
        "srflx" => 2,
        "relay" => 4,
        "prflx" => 8,
        _ => 0,
    };
    if bit != 0 && k.ice_typ_seen & bit == 0 {
        k.ice_typ_seen |= bit;
        crate::analytics::log("cand", &format!(r#","typ":"{typ}","h":{}"#, k.local_handle));
    }
    let mut d = VarDictionary::new();
    d.set("kind", "ice");
    d.set("media", media);
    d.set("index", index);
    d.set("name", name);
    if k.party_count > 2 {
        d.set("to", peer);
    }
    if let Some(mut ws) = k.ws.clone() {
        ws.send_text(&rtc::to_json(&d));
    }
}

/// Channel(s) open: hand off to a fresh ggrs session and flip to Running. k<=2 is byte-for-byte the
/// original single-peer flow. k>2 delegates to `begin_mesh_session`'s own gating.
pub(super) fn begin_session(k: &mut KneeMan) {
    if k.party_count > 2 {
        begin_mesh_session(k);
        return;
    }
    let Some(role) = k.role else { return };
    let (local_handle, remote) = role.handles();
    let Some(channel) = k.channel.clone() else {
        return;
    };
    let socket = RtcSocket { channel, remote };
    match start_p2p::<Smash, _>(local_handle, remote, socket, rtc::INPUT_DELAY) {
        Ok(session) => {
            let mut state = k.resume_snapshot.unwrap_or_else(SimState::spawn);
            if k.resume_snapshot.is_none() {
                let cap = crate::roster::roster().len() - 1;
                let pick = (k.charsel.get_cloned()[0].max(0) as usize).min(cap);
                state.fighters[local_handle].char_id = pick as u8;
                if let Some(pc) = k.peer_char {
                    state.fighters[remote].char_id = pc.min(cap) as u8;
                }
            }
            let game = SmashGame::from_state(state, k.tune.get_cloned());
            k.net = Some(Box::new(GgrsNetplay::new(session, game, local_handle)));
            k.state.set(state);
            k.local_handle = local_handle;
            let room = k.room.as_ref().map(|r| r.code.clone()).unwrap_or_default();
            let peer_build = k.peer_build.clone().unwrap_or_default();
            crate::analytics::log(
                "session_begin",
                &format!(
                    r#","handle":{local_handle},"remote":{remote},"resumed":{},"room":"{room}","build":"{}","peer_build":"{}","count":2,"handles":{}"#,
                    k.resume_snapshot.is_some(),
                    rtc::BUILD_HASH,
                    peer_build,
                    handles_json(2),
                ),
            );
            k.set_phase(Phase::Running);
            if let Some(r) = k.room.as_mut() {
                r.deadline_ms = None; // back in a match; close the reconnect window
            }
            godot_print!("netplay: channel open, rollback running (handle {local_handle})");
        }
        Err(e) => {
            crate::analytics::log(
                "session_fail",
                &format!(r#","err":{}"#, crate::analytics::jstr(&format!("{e:?}"))),
            );
            godot_error!("netplay: session start failed: {e:?}");
            k.reset_offline();
        }
    }
}

/// k>2 session start. Gates on: every pair's channel OPEN, the guest's Tune landed (same guest-only
/// gate as k<=2), and every OTHER handle's pick noted -- OR `MESH_PICK_TIMEOUT_MS` elapsed, in which
/// case a still-missing pick resolves to roster slot 0. Every peer stamps a given handle with either
/// the EXACT value that handle sent, or the exact same literal default (0) -- never a third, made-up
/// value -- so a slow/lost pchar can't corrupt a stamp into something peer-specific. It CAN still
/// race (peer A's local clock times out and stamps 0 for a handle whose pick peer B receives just
/// after A's deadline, so B stamps the real value): that's a genuine frame-0 divergence, not a bug in
/// this rule, and it's exactly what `log_events`' new desync telemetry exists to catch in production.
/// A fully race-proof version would need a host-arbitrated "picks final" broadcast; out of scope here.
fn begin_mesh_session(k: &mut KneeMan) {
    let count = k.party_count;
    let local = k.local_handle;
    if k.role == Some(Role::Guest) && !k.got_tune {
        return;
    }
    let all_open = (0..count).all(|h| h == local || link_open(k, h));
    if !all_open {
        return;
    }
    let deadline_passed = k
        .mesh
        .as_ref()
        .map(|m| crate::net::now_ms() >= m.deadline_ms)
        .unwrap_or(false);
    let all_picked = (0..count).all(|h| h == local || pick_noted(k, h));
    if !all_picked && !deadline_passed {
        return;
    }

    let mut channels: [Option<Gd<WebRtcDataChannel>>; sim::MAX_PLAYERS] = Default::default();
    if let Some(mesh) = k.mesh.as_ref() {
        for (h, slot) in channels.iter_mut().enumerate() {
            if h != local {
                *slot = mesh.links[h].as_ref().map(|l| l.channel.clone());
            }
        }
    }
    let socket = MeshSocket { local, channels };
    let addrs: Vec<usize> = (0..count).collect(); // addr IS the handle -- see MeshSocket's doc
    match start_p2p_n::<Smash, _>(local, &addrs, socket, rtc::INPUT_DELAY) {
        Ok(session) => {
            let cap = crate::roster::roster().len() - 1;
            let mut state = SimState::spawn_n(count);
            let my_pick = (k.charsel.get_cloned()[0].max(0) as usize).min(cap);
            state.fighters[local].char_id = my_pick as u8;
            for h in 0..count {
                if h == local {
                    continue;
                }
                let pick = k
                    .mesh
                    .as_ref()
                    .and_then(|m| m.picks[h])
                    .unwrap_or(0)
                    .min(cap);
                state.fighters[h].char_id = pick as u8;
            }
            let game = SmashGame::from_state(state, k.tune.get_cloned());
            k.net = Some(Box::new(GgrsNetplay::new(session, game, local)));
            k.state.set(state);
            crate::analytics::log(
                "session_begin",
                &format!(
                    r#","handle":{local},"resumed":false,"room":"{}","build":"{}","peer_build":"{}","count":{count},"handles":{}"#,
                    k.room.as_ref().map(|r| r.code.clone()).unwrap_or_default(),
                    rtc::BUILD_HASH,
                    k.peer_build.clone().unwrap_or_default(),
                    handles_json(count),
                ),
            );
            k.set_phase(Phase::Running);
            if let Some(r) = k.room.as_mut() {
                r.deadline_ms = None;
            }
            godot_print!(
                "netplay: mesh channels open, rollback running (handle {local}, party {count})"
            );
        }
        Err(e) => {
            crate::analytics::log(
                "session_fail",
                &format!(r#","err":{}"#, crate::analytics::jstr(&format!("{e:?}"))),
            );
            godot_error!("netplay: mesh session start failed: {e:?}");
            k.reset_offline();
        }
    }
}

fn link_open(k: &KneeMan, peer: usize) -> bool {
    k.mesh
        .as_ref()
        .and_then(|m| m.links[peer].as_ref())
        .map(|l| l.channel.get_ready_state() == ChannelState::OPEN)
        .unwrap_or(false)
}

fn pick_noted(k: &KneeMan, peer: usize) -> bool {
    k.mesh
        .as_ref()
        .map(|m| m.picks[peer].is_some())
        .unwrap_or(false)
}

/// pump_signaling's k>2 gate: there's no single `self.channel` to test (that's the k<=2 field), so
/// just try `begin_session` — its own gating (above) is idempotent and a no-op until ready.
pub(super) fn maybe_begin_session(k: &mut KneeMan) {
    if k.net.is_none() {
        k.begin_session();
    }
}

/// Pump every mesh peer connection's ICE/signaling state machine each frame (the k<=2 path polls its
/// single `self.pc` inline in `step_net`; this covers the rest for k>2). Also edge-triggers the
/// one-shot "peer_channel_open" analytics line the first frame each pair's channel opens.
pub(super) fn poll_peers(k: &mut KneeMan) {
    let Some(mesh) = k.mesh.as_mut() else { return };
    let mut opened = Vec::new();
    for (h, link) in mesh.links.iter_mut().enumerate() {
        let Some(link) = link.as_mut() else { continue };
        link.pc.poll();
        let open = link.channel.get_ready_state() == ChannelState::OPEN;
        if open && !link.channel_was_open {
            link.channel_was_open = true;
            opened.push(h);
        }
    }
    for h in opened {
        crate::analytics::log("peer_channel_open", &format!(r#","peer":{h}"#));
    }
}

/// k>2 transport-drop check: ANY pair's channel closed or connection failed ends the whole party
/// (see `abort_party` — the documented reconnect punt).
pub(super) fn any_peer_dropped(k: &KneeMan) -> bool {
    let Some(mesh) = k.mesh.as_ref() else {
        return false;
    };
    mesh.links.iter().flatten().any(|l| {
        l.channel.get_ready_state() == ChannelState::CLOSED
            || l.pc.get_connection_state() == ConnectionState::FAILED
    })
}

/// Best-effort: which handle's link tripped `any_peer_dropped`, for the "party_abort" analytics
/// payload. `None` when the drop was reported by ggrs itself rather than the transport (e.g. no ICE
/// failure yet, just ggrs's own disconnect timeout) -- the abort still proceeds either way.
pub(super) fn dropped_peer_handle(k: &KneeMan) -> Option<usize> {
    let mesh = k.mesh.as_ref()?;
    mesh.links.iter().enumerate().find_map(|(h, l)| {
        let l = l.as_ref()?;
        (l.channel.get_ready_state() == ChannelState::CLOSED
            || l.pc.get_connection_state() == ConnectionState::FAILED)
            .then_some(h)
    })
}

/// k>2 peer-drop punt (WIRE PROTOCOL v2 #4): end straight to Offline with a toast + analytics event,
/// instead of the k==2 reconnect dance. Structured so a later per-peer resume only has to replace
/// this one function's body, not any of its call sites.
pub(super) fn abort_party(k: &mut KneeMan, reason: &str, peer: Option<usize>) {
    crate::analytics::log(
        "party_abort",
        &format!(
            r#","reason":"{reason}","peer":{},"count":{}"#,
            peer.map(|p| p as i64).unwrap_or(-1),
            k.party_count,
        ),
    );
    crate::toast::push(
        &k.toasts,
        crate::toast::ToastKind::Error,
        "Party disconnected — back to local play",
    );
    k.reset_offline();
}

/// Turn drained ggrs session events into the "/ev" firehose. Only `Desync` needs a line here --
/// `PeerDisconnected` is already covered by `step_net`'s `peer_gone` handling (phase toast, and for
/// k>2 `abort_party`), so logging it again would just double that line. Checksums ride as hex
/// strings, not bare JSON numbers -- a u128 doesn't fit a JS `number` without losing precision, and
/// this is the one thing in the payload that must stay byte-exact to be useful.
pub(super) fn log_events(events: &[NetplayEvent]) {
    for ev in events {
        if let NetplayEvent::Desync {
            frame,
            local_checksum,
            remote_checksum,
            addr,
        } = ev
        {
            crate::analytics::log(
                "desync",
                &format!(
                    r#","frame":{frame},"local_checksum":"{local_checksum:x}","remote_checksum":"{remote_checksum:x}","peer":{addr}"#
                ),
            );
        }
    }
}
