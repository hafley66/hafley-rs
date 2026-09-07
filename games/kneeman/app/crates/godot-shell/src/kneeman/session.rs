//! Netplay session lifecycle: matchmaking, WebRTC signaling/handshake, ggrs stepping,
//! reconnect, phase transitions, analytics heartbeat, and the status chip text.

use godot::classes::WebSocketPeer;
use godot::classes::web_rtc_data_channel::ChannelState;
use godot::classes::web_rtc_peer_connection::ConnectionState;
use godot::classes::web_socket_peer::State as WsState;
use godot::prelude::*;

use crate::net::{NetDebug, chan_name, conn_name, gather_name, now_ms, signal_name, ws_name};
use crate::rtc::{self, Role};
use crate::sim::net::{Advance, encode};
use crate::sim::{self, InputFrame, SimState};

use super::touch::{TOUCH_CSTICK, TOUCH_STICK};
use super::{KneeMan, Phase, RECONNECT_WINDOW_MS, Room, SigCounts, gv, mesh};

// Resume + Tune are sent together before the next poll. Their base64 envelopes
// exceed WebSocketPeer's default 65,535-byte queue. Bound both directions equally.
const SIGNAL_BUFFER_BYTES: i32 = 256 * 1024;

/// The LOCAL single-player starting state (mod.rs:194 `init`). Default is the normal
/// `SimState::spawn()`; set `SMASH_SCENARIO=droptest` to launch the debug drop-test playground
/// (`SimState::spawn_drop_test`) instead -- the ship plus one of every collision-surface kind, for
/// hand-driving enter/land/tunnel jank. Reading `std::env` HERE (session construction, not `step()`)
/// keeps the deterministic tick untouched; the net/lobby spawn paths (`mesh.rs`) are unaffected, so
/// only offline local play sees the branch. Default OFF: a plain `just run-debug` is unchanged.
pub(super) fn local_spawn() -> SimState {
    match std::env::var("SMASH_SCENARIO").as_deref() {
        Ok("droptest") => SimState::spawn_drop_test(),
        _ => SimState::spawn(),
    }
}

/// Percent-encode the few characters that would break a query string. Names are short + tame, so
/// anything outside the unreserved set becomes %XX (the relay's `url_decode` reverses it).
fn url_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Trim a shared join-link room code; `None` for empty/whitespace-only (falls through to open
/// matchmaking instead of silently dialing the relay's "default" room). `url_escape` handles the
/// wire encoding at the dial call site, same as name/color -- this only decides usability.
pub(super) fn sanitize_join_code(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// `?join=<code>` boot param (web only, see `net::query_param`), sanitized. Shareable lobby link:
/// both people open `/game/?join=chris-fight` and land in the same private room. A product
/// feature -- independent of `crate::webtest`'s test_mode, never arms it.
pub(super) fn boot_join_code() -> Option<String> {
    crate::net::query_param("join").and_then(|raw| sanitize_join_code(&raw))
}

impl KneeMan {
    /// Ping the relay's /status to learn the live build, and (if mid-match) re-trade the peer hash.
    /// Called on focus-in so a tab woken after a deploy notices it's running stale code.
    pub(super) fn ping_version(&mut self) {
        if let Some(http) = self.http.as_mut() {
            let _ = http.request(&rtc::status_url());
        }
    }

    /// Record the opponent's build hash from the handshake. Surfaced by `status_text` so a mismatched
    /// pair sees it before the differing sims desync. Empty/"unknown" hashes are ignored.
    pub(super) fn note_peer_build(&mut self, hash: String) {
        if !hash.is_empty() && hash != "unknown" {
            self.peer_build = Some(hash);
        }
    }

    /// Once per frame: heartbeat the live phase/channel while connected (so a stall shows as repeated
    /// lines with unchanging state), and flush the buffered events to the relay on a cadence. ~60Hz
    /// physics tick: heartbeat every 60 frames (~1s), flush every 30 (~0.5s).
    pub(super) fn pump_analytics(&mut self) {
        self.ev_tick = self.ev_tick.wrapping_add(1);
        if self.phase != Phase::Offline && self.ev_tick % 60 == 0 {
            crate::analytics::log(
                "hb",
                &format!(
                    r#","phase":"{}","chan":"{}","conn":"{}","oi":{},"ai":{},"ii":{}"#,
                    Self::phase_name(self.phase),
                    self.last_net.channel,
                    self.last_net.conn,
                    self.sig.offer_in,
                    self.sig.answer_in,
                    self.sig.ice_in,
                ),
            );
        }
        // slow world-size sample (~30s): durable footprint + live ink/item counts, so a growing
        // persistent home (permanent strokes, gif blobs, autosaves) is visible in the firehose
        // across sessions. Fires in every phase — the offline home room is exactly the world
        // whose growth we want to watch.
        if self.ev_tick % 1800 == 0 {
            let s = self.state.get();
            let live = || s.paths.iter().filter(|p| p.active());
            let paths = live().count();
            let pts: u32 = live().map(|p| p.len as u32).sum();
            let mass: f32 = live().map(|p| p.mass).sum();
            let perm = live().filter(|p| p.props.stroke_life < 0).count();
            let items = s.items.iter().filter(|i| i.active()).count();
            let (cb, slots) = self
                .world
                .as_ref()
                .map(|w| (w.cache_bytes(), w.slots().len()))
                .unwrap_or((0, 0));
            crate::analytics::log(
                "world",
                &format!(
                    r#","cb":{cb},"slots":{slots},"paths":{paths},"pts":{pts},"mass":{mass:.0},"perm":{perm},"items":{items}"#
                ),
            );
        }
        if self.ev_tick % 30 == 0 {
            let url = crate::rtc::event_url();
            if let Some(h) = self.analytics_http.as_mut() {
                crate::analytics::flush(h, &url);
            }
        }
    }

    /// Refresh `window.__smash` for the E2E/replay test harness (no-op unless `?autofind`/`?script`
    /// armed it at boot -- see `crate::webtest`). Runs once per physics tick so a Playwright poll
    /// always reads the tick/checksum/phase the sim just settled on.
    pub(super) fn publish_webtest(&mut self) {
        if crate::webtest::test_mode() {
            let receipts = self.net.as_mut().map(|net| net.confirmed_checksums()).unwrap_or_default();
            let s = self.state.get();
            crate::webtest::refresh(
                s.tick,
                Self::phase_name(self.phase),
                self.phase == Phase::Running,
                &s,
            );
            crate::webtest::debug_json(&format!(
                r#"{{"peer_char":{},"charsel0":{},"handle":{},"char0":{},"char1":{},"x":[{},{}],"y":[{},{}],"states":["{:?}","{:?}"],"ship":[{},{}],"terrain":{}}}"#,
                self.peer_char.map(|c| c as i64).unwrap_or(-1),
                self.charsel.get_cloned()[0],
                self.local_handle,
                s.fighters[0].char_id,
                s.fighters[1].char_id,
                s.fighters[0].pos.x,
                s.fighters[1].pos.x,
                s.fighters[0].pos.y,
                s.fighters[1].pos.y,
                s.fighters[0].state,
                s.fighters[1].state,
                s.paths[sim::SHIP_SLOT].pos.x,
                s.paths[sim::SHIP_SLOT].pos.y,
                serde_json::json!({
                    "holding": s.fighters[0].holding,
                    "cells": s.paths.iter().filter_map(|p| p.cell.map(|c| (c.id.get(), p.percent))).collect::<Vec<_>>(),
                    "items": s.items.iter().enumerate().filter_map(|(slot, i)| i.cell.map(|c|
                        (c.id.get(), slot, i.owner, i.thrown))).collect::<Vec<_>>(),
                }),
            ));
            crate::webtest::confirmed_checksums(&receipts);
        }
    }

    /// Stable string name for a phase (shared by the NetDebug DTO and the event firehose).
    pub(super) fn phase_name(p: Phase) -> &'static str {
        match p {
            Phase::Offline => "offline",
            Phase::Signaling => "signaling",
            Phase::Running => "running",
            Phase::Reconnecting => "reconnecting",
        }
    }

    /// The ONLY writer of `self.phase`: logs the transition to the firehose, then sets it. Routing
    /// every phase change through here means the event log can never miss one.
    pub(super) fn set_phase(&mut self, p: Phase) {
        if self.phase != p {
            let room = self.room.as_ref().map(|r| r.code.as_str()).unwrap_or("");
            crate::analytics::log(
                "phase",
                &format!(
                    r#","from":"{}","to":"{}","room":"{}","handle":{}"#,
                    Self::phase_name(self.phase),
                    Self::phase_name(p),
                    room,
                    self.local_handle,
                ),
            );
            self.toast_phase(self.phase, p);
        }
        self.phase = p;
    }

    /// Surface a phase transition to the player as a snackbar. Same (from,to) chokepoint the firehose
    /// logs, so the connect/drop/reconnect story is told once and both consumers stay in lockstep.
    /// Only real lifecycle edges toast; an intentional Leave (Signaling/Offline -> Offline) stays quiet.
    pub(super) fn toast_phase(&self, from: Phase, to: Phase) {
        use crate::toast::{ToastKind, push};
        let (kind, text) = match (from, to) {
            (Phase::Signaling, Phase::Running) => {
                (ToastKind::Success, "Connected to your opponent")
            }
            (Phase::Reconnecting, Phase::Running) => (ToastKind::Success, "Reconnected"),
            (Phase::Running, Phase::Reconnecting) => {
                (ToastKind::Warn, "Connection lost, reconnecting…")
            }
            (Phase::Reconnecting, Phase::Offline) => {
                (ToastKind::Error, "Reconnect timed out. You are offline.")
            }
            (Phase::Running, Phase::Offline) => (ToastKind::Error, "Disconnected"),
            _ => return,
        };
        push(&self.toasts, kind, text);
    }

    /// Read the live transport states off the ws/pc/channel handles and publish them for the panel.
    pub(super) fn publish_netdbg(&mut self) {
        let ws = self
            .ws
            .as_ref()
            .map(|w| ws_name(w.get_ready_state()))
            .unwrap_or("—");
        let (conn, gather, signal) = match self.pc.as_ref() {
            Some(pc) => (
                conn_name(pc.get_connection_state()),
                gather_name(pc.get_gathering_state()),
                signal_name(pc.get_signaling_state()),
            ),
            None => ("—", "—", "—"),
        };
        let channel = self
            .channel
            .as_ref()
            .map(|c| chan_name(c.get_ready_state()))
            .unwrap_or("—");
        let nd = NetDebug {
            phase: Self::phase_name(self.phase),
            role: match self.role {
                Some(Role::Host) => "host",
                Some(Role::Guest) => "guest",
                None => "—",
            },
            handle: self.local_handle,
            ws,
            conn,
            gather,
            signal,
            channel,
            offer: (self.sig.offer_out, self.sig.offer_in),
            answer: (self.sig.answer_out, self.sig.answer_in),
            ice: (self.sig.ice_out, self.sig.ice_in),
            build_hash: rtc::BUILD_HASH,
            stale_build: self.stale_build,
            peer_build_mismatch: self
                .peer_build
                .as_deref()
                .map(|p| !p.is_empty() && p != "unknown" && p != rtc::BUILD_HASH)
                .unwrap_or(false),
        };
        // Firehose the transport state machine, edge-triggered: log only when ws/conn/gather/signal/
        // channel actually flips, so it's deltas (channel connecting->open is the mode-A/B tell), not
        // one line every frame.
        let o = self.last_net;
        if (nd.ws, nd.conn, nd.gather, nd.signal, nd.channel)
            != (o.ws, o.conn, o.gather, o.signal, o.channel)
        {
            crate::analytics::log(
                "net",
                &format!(
                    r#","ws":"{}","conn":"{}","gather":"{}","signal":"{}","chan":"{}","handle":{}"#,
                    nd.ws, nd.conn, nd.gather, nd.signal, nd.channel, nd.handle,
                ),
            );
        }
        self.last_net = nd;
        self.netdbg.set(nd);
    }

    /// Cap-pressure warn (plans/body-bus.md step 10): the sim's fixed arrays (paths, items)
    /// are deliberately NOT hard limits on play — when one runs sustained-hot, warn the
    /// player/dev instead of silently dropping strokes. Toasts once per hot episode
    /// (re-arms after occupancy falls back under the line); raising a cap is a routine
    /// schema bump, and this is the tell that it's due.
    pub(super) fn check_pressure(&mut self) {
        const HOT: f32 = 0.8; // occupancy fraction that counts as pressure
        const SUSTAIN: u32 = 180; // 3s at 60fps before the warn fires
        let s = self.state.get();
        let paths = s.paths.iter().filter(|p| p.active()).count() as f32 / sim::MAX_DRAWN as f32;
        let items = s.items.iter().filter(|i| i.active()).count() as f32 / sim::MAX_ITEMS as f32;
        if paths > HOT || items > HOT {
            self.pressure_frames = self.pressure_frames.saturating_add(1);
            if self.pressure_frames == SUSTAIN && !self.pressure_warned {
                self.pressure_warned = true;
                crate::toast::push(
                    &self.toasts,
                    crate::toast::ToastKind::Warn,
                    format!(
                        "Object pressure: ink {}/{} · items {}/{} — new strokes may drop",
                        (paths * sim::MAX_DRAWN as f32).round() as usize,
                        sim::MAX_DRAWN,
                        (items * sim::MAX_ITEMS as f32).round() as usize,
                        sim::MAX_ITEMS,
                    ),
                );
                crate::analytics::log(
                    "pressure",
                    &format!(r#","paths":{paths:.2},"items":{items:.2}"#),
                );
            }
        } else {
            self.pressure_frames = 0;
            self.pressure_warned = false;
        }
    }

    // --- frame loop (local + netplay) -----------------------------------------------------------

    /// Sample the local player's controls into the engine-agnostic `InputFrame`. A `ScriptedDevice`
    /// entry covering `tick` (see `crate::webtest`, the E2E/replay test hook) wins over the live
    /// device poll -- scripted input rides this exact call site, so it goes through ggrs exactly
    /// like a human press. `tick` is `SimState.tick`, the same counter `window.__smash.tick` reports.
    /// Falls back to the `controls` boundary (the only raw-device site); the touch stick is merged
    /// in from our widget.
    pub(super) fn sample_input(tick: u64) -> InputFrame {
        crate::webtest::scripted_frame(tick)
            .unwrap_or_else(|| crate::controls::poll(TOUCH_STICK.get(), TOUCH_CSTICK.get()))
    }

    /// Local play: step the pure sim with both players' frames and render. P1 is this machine's main
    /// controls; P2 is couch co-op (second gamepad / WASD), neutral until someone grabs it.
    pub(super) fn step_local(&mut self) {
        let frame = Self::sample_input(self.state.get().tick);
        self.last_aim = sim::Vector2::new(frame.cx, frame.cy);
        let p2 = crate::controls::poll_p2();
        let before = self.state.get();
        let tune = self.tune.get_cloned();
        let mut next = sim::step(&before, &[&frame, &p2], &tune); // pure scan
        self.debugger.record(&before, &next, [frame, p2], &tune);
        self.poll_spawn_keys(&mut next); // number keys 1..0 force-spawn test items (local play only)
        self.state.set(next);
        self.base_mut().set_position(gv(next.fighters[0].pos));
        self.render_fighters(&next);
        self.update_camera();
        self.base_mut().queue_redraw();
    }

    /// Testing helper: number keys 1..9,0 force-spawn the first ten items of the roster onto the
    /// stage (rising-edge, so one press = one spawn). Local play only — it mutates state outside the
    /// rollback loop, so it is deliberately not wired into the netplay path.
    pub(super) fn poll_spawn_keys(&mut self, s: &mut SimState) {
        let tune = self.tune.get_cloned();
        for i in crate::controls::number_key_edges(&mut self.spawn_prev) {
            if let Some(card) = sim::MENU_ITEMS.get(i) {
                sim::spawn_kind(s, card.kind, card.tool, card.stroke, &tune);
            }
        }
    }

    /// Netplay: ggrs owns the loop. Poll the transport, feed local input, advance (rolling back as
    /// needed via `Game::handle`), then mirror the rollback state into `self.state` for rendering.
    pub(super) fn step_net(&mut self) {
        if let Some(mut pc) = self.pc.clone() {
            pc.poll();
        }
        mesh::poll_peers(self); // k>2: pump every OTHER pair's connection (the one above is k<=2's)
        // Transport-level drop: ICE failed or the data channel closed. ggrs also reports the peer
        // gone (its packets stopped) via a Disconnected event. Either one opens the reconnect window.
        let mut peer_gone = self.transport_dropped();
        let sampled = Self::sample_input(self.state.get().tick);
        self.last_aim = sim::Vector2::new(sampled.cx, sampled.cy);
        let events = {
            let Some(net) = self.net.as_mut() else { return };
            net.poll(); // pump transport + drain session events (may flag a peer drop)
            let events = net.drain_events(); // desync reports; peer-gone already folds into Advance
            if !peer_gone {
                let local = encode(&sampled);
                if net.advance(local) == Advance::PeerGone {
                    peer_gone = true;
                }
            }
            events
        };
        mesh::log_events(&events);
        if peer_gone {
            // k>2 has no per-peer resume yet (WIRE PROTOCOL v2 #4's documented punt): any drop ends
            // the whole party instead of opening the k==2 reconnect window.
            if self.party_count > 2 {
                mesh::abort_party(self, "peer_dropped", mesh::dropped_peer_handle(self));
            } else {
                self.begin_reconnect();
            }
            return;
        }
        if let Some(net) = self.net.as_ref() {
            self.state.set(*net.state()); // mirror the authoritative frame for rendering
        }
        let s = self.state.get();
        self.base_mut().set_position(gv(s.fighters[0].pos));
        self.render_fighters(&s);
        self.update_camera();
        self.base_mut().queue_redraw();
    }

    // --- netplay setup / signaling --------------------------------------------------------------

    /// Start matchmaking on a named room (a lobby). Two clients that call this with the same key
    /// pair up over the existing 1v1 transport. No-op unless Offline (like the status chip's tap).
    pub fn matchmake_room(&mut self, key: &str) {
        if self.phase != Phase::Offline {
            return;
        }
        self.room = Some(Room {
            code: key.to_string(),
            deadline_ms: None,
        });
        self.resume_snapshot = None;
        self.got_resume = false;
        self.got_tune = false;
        if !self.dial(Some(key)) {
            self.room = None;
            return;
        }
        self.set_phase(Phase::Signaling);
        godot_print!("netplay: dialing lobby {} ...", key);
    }

    /// Dial the signaling relay. The relay replies `matched` with a role, kicking off the handshake.
    /// A fresh match has no room yet (the host mints one once paired); reconnects re-dial with it.
    pub(super) fn start_matchmaking(&mut self) {
        self.room = None;
        self.resume_snapshot = None; // fresh match starts from spawn, not a stale snapshot
        self.got_resume = false;
        self.got_tune = false;
        if !self.dial(None) {
            return;
        }
        self.set_phase(Phase::Signaling);
        godot_print!("netplay: dialing {} ...", rtc::signaling_url());
    }

    /// Open a signaling socket, tagged with our identity (so the relay's /status lists us) and an
    /// optional `room` code. `None` = open matchmaking (relay's "default" room); `Some(code)` =
    /// re-pair with a specific opponent on reconnect. Returns false if the dial failed.
    pub(super) fn dial(&mut self, room: Option<&str>) -> bool {
        let mut ws = WebSocketPeer::new_gd();
        ws.set_inbound_buffer_size(SIGNAL_BUFFER_BYTES);
        ws.set_outbound_buffer_size(SIGNAL_BUFFER_BYTES);
        let id = self.identity.get_cloned();
        let c = id.color;
        let hex = format!(
            "%23{:02x}{:02x}{:02x}",
            (c.r * 255.0) as u8,
            (c.g * 255.0) as u8,
            (c.b * 255.0) as u8
        );
        let mut url = format!(
            "{}?name={}&color={}&hash={}",
            rtc::signaling_url(),
            url_escape(&id.name),
            hex,
            rtc::BUILD_HASH,
        );
        if let Some(code) = room {
            url.push_str(&format!("&room={}", url_escape(code)));
        }
        if ws.connect_to_url(&url) != godot::global::Error::OK {
            godot_error!("netplay: signaling dial failed");
            return false;
        }
        self.ws = Some(ws);
        self.sig = SigCounts::default(); // fresh tallies per match attempt
        true
    }

    /// Per-frame while Signaling: drain inbound relay frames, drive the peer connection, and start
    /// the rollback session the moment the data channel opens.
    pub(super) fn pump_signaling(&mut self) {
        let texts = {
            let Some(ws) = self.ws.as_mut() else { return };
            ws.poll();
            match ws.get_ready_state() {
                WsState::OPEN => {}
                WsState::CONNECTING | WsState::CLOSING => return, // not ready / shutting down
                _ => {
                    // CLOSED (or unknown): bail to offline below.
                    self.reset_offline();
                    return;
                }
            }
            let mut v = Vec::new();
            for _ in 0..ws.get_available_packet_count() {
                let pkt = ws.get_packet();
                v.push(GString::from(
                    String::from_utf8_lossy(pkt.as_slice()).as_ref(),
                ));
            }
            v
        };
        for t in texts {
            self.handle_signal(&t);
        }
        if let Some(mut pc) = self.pc.clone() {
            pc.poll();
        }
        if self.party_count > 2 {
            // No single `self.channel` to gate on (see `mesh::MeshSession::links`); the mesh gate
            // (every pair open + tune + picks, or the pick timeout) lives in `begin_session` itself.
            mesh::maybe_begin_session(self);
        } else if let Some(ch) = self.channel.clone() {
            if ch.get_ready_state() == ChannelState::OPEN && self.net.is_none() {
                // Both roles wait for atomic SDP startup negotiation. A fresh transport host
                // may need the surviving guest's snapshot, Tune and complementary fighter slot.
                let waiting_for_resume = !self.got_resume;
                let waiting_for_tune = !self.got_tune;
                if !waiting_for_resume && !waiting_for_tune {
                    self.begin_session();
                }
            }
        }
    }

    /// Dispatch one signaling frame from the relay (already JSON-parsed by key). The full match
    /// lives in `mesh::handle_signal` (k<=2 byte-for-byte the original single-peer flow, k>2 the new
    /// mesh routing) — see that module's doc for why this is a thin wrapper, not a signature change.
    pub(super) fn handle_signal(&mut self, text: &GString) {
        mesh::handle_signal(self, text);
    }

    /// Channel(s) open: hand off to a fresh ggrs session and flip to Running. See
    /// `mesh::begin_session` (k<=2 branch is byte-for-byte the original single-peer flow).
    pub(super) fn begin_session(&mut self) {
        mesh::begin_session(self);
    }

    /// Tear down all networking and return to single-player. Frees the room ("turn off").
    pub(super) fn reset_offline(&mut self) {
        self.net = None;
        self.channel = None;
        self.pc = None;
        if let Some(mut ws) = self.ws.take() {
            ws.close();
        }
        self.role = None;
        self.room = None;
        self.resume_snapshot = None;
        self.got_resume = false;
        self.got_tune = false;
        self.party_count = 2;
        self.mesh = None;
        self.set_phase(Phase::Offline);
        godot_print!("netplay: offline");
    }

    /// Has the live transport died? ICE went to `failed`, or the ggrs data channel closed. (A merely
    /// `disconnected` connection can still recover ICE on its own, so it does NOT count here — only a
    /// definitive failure triggers the heavier room reconnect.) k>2 has no single `self.pc`/`channel`
    /// to check — see `mesh::any_peer_dropped`.
    pub(super) fn transport_dropped(&self) -> bool {
        if self.party_count > 2 {
            return mesh::any_peer_dropped(self);
        }
        let chan_closed = self
            .channel
            .as_ref()
            .map(|c| c.get_ready_state() == ChannelState::CLOSED)
            .unwrap_or(false);
        let conn_failed = self
            .pc
            .as_ref()
            .map(|p| p.get_connection_state() == ConnectionState::FAILED)
            .unwrap_or(false);
        chan_closed || conn_failed
    }

    /// Peer dropped mid-match: drop the dead transport but KEEP the room identity, re-dial the
    /// private room, and open the reconnect window. Both peers do this and re-pair with each other
    /// (only they know the code). Without a shared code (drop before it was exchanged) we can't
    /// rejoin, so fall straight to offline.
    pub(super) fn begin_reconnect(&mut self) {
        let Some(code) = self.room.as_ref().map(|r| r.code.clone()) else {
            self.reset_offline();
            return;
        };
        godot_print!("netplay: peer dropped — reconnecting to room {code}");
        // Capture the latest sim state so the rebuilt session resumes here instead of from spawn.
        // Both peers capture; the new host's snapshot wins (it ships it over the relay).
        self.resume_snapshot = self
            .net
            .as_ref()
            .map(|n| *n.state())
            .or_else(|| Some(self.state.get()));
        self.got_resume = false;
        self.got_tune = false;
        self.net = None;
        self.channel = None;
        self.pc = None;
        if let Some(mut ws) = self.ws.take() {
            ws.close();
        }
        self.role = None;
        if !self.dial(Some(&code)) {
            self.reset_offline();
            return;
        }
        if let Some(r) = self.room.as_mut() {
            r.deadline_ms = Some(now_ms() + RECONNECT_WINDOW_MS);
        }
        self.set_phase(Phase::Reconnecting);
    }

    /// One line describing where we are in the netplay lifecycle, shown top-left every frame.
    pub(super) fn status_text(&self) -> String {
        // Version warnings ride in front of the lifecycle text: a stale local build (deploy happened
        // while this tab slept) or an opponent on a different build (their sim will diverge from ours).
        if self.stale_build {
            return "⚠ NEW BUILD LIVE  ·  reload the page to update".to_string();
        }
        if let Some(peer) = self.peer_build.as_ref() {
            if peer != rtc::BUILD_HASH {
                return format!(
                    "⚠ VERSION MISMATCH  ·  you {} vs opponent {} — both reload",
                    rtc::BUILD_HASH,
                    peer,
                );
            }
        }
        match self.phase {
            Phase::Offline => "OFFLINE  ·  tap to find a match".to_string(),
            Phase::Signaling => "SIGNALING…  ·  waiting for an opponent".to_string(),
            Phase::Running => {
                let who = match self.role {
                    Some(Role::Host) => "host",
                    Some(Role::Guest) => "guest",
                    None => "?",
                };
                format!("NETPLAY  ·  {who} (handle {})", self.local_handle)
            }
            Phase::Reconnecting => {
                let secs = self
                    .room
                    .as_ref()
                    .and_then(|r| r.deadline_ms)
                    .map(|d| d.saturating_sub(now_ms()).div_ceil(1000))
                    .unwrap_or(0);
                format!("RECONNECTING…  ·  waiting {secs}s for your opponent")
            }
        }
    }

    /// Push the current phase into the on-screen chip, and size/fade it for the context: the 20px
    /// chip is a speck on a phone, so bump it on a touchscreen; during a live match fade it to 75%
    /// so it doesn't fight the action.
    pub(super) fn update_status(&mut self) {
        let txt = self.status_text();
        let mobile = godot::classes::DisplayServer::singleton().is_touchscreen_available();
        let in_match = self.phase == Phase::Running;
        if let Some(mut l) = self.status.clone() {
            l.set_text(&txt);
            l.add_theme_font_size_override("font_size", if mobile { 34 } else { 20 });
            let alpha = if in_match { 0.75 } else { 1.0 };
            l.set_modulate(Color::from_rgba(1.0, 1.0, 1.0, alpha));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signaling_buffer_fits_resume_and_tune_burst() {
        let state = bincode::serialize(&SimState::spawn()).unwrap();
        let tune = bincode::serialize(&sim::Tune::default()).unwrap();
        let burst = 4 * (state.len().div_ceil(3) + tune.len().div_ceil(3));
        assert!(burst > 65_535, "fixture must exceed the old queue");
        assert!(burst + 16 * 1024 < SIGNAL_BUFFER_BYTES as usize,
            "reserve room for JSON envelopes, SDP, ICE and terrain metadata");
    }

    #[test]
    fn join_code_trims_surrounding_whitespace() {
        let want = Some("chris-fight".to_string());
        assert_eq!(sanitize_join_code("  chris-fight  "), want);
        assert_eq!(sanitize_join_code("chris-fight"), want); // already-clean passes through
    }

    #[test]
    fn join_code_empty_or_whitespace_only_is_none() {
        assert_eq!(sanitize_join_code(""), None);
        assert_eq!(sanitize_join_code("   "), None);
    }
}
