//! Rollback glue for the pure sim. ggrs owns the frame loop; it calls `crate::v1::step`
//! (possibly several times per frame when re-simulating after a late input). This crate has
//! NO engine and NO transport yet — just the ggrs `Config`, the wire input, the save/load/advance
//! handler, and a SyncTest that proves the sim is deterministic enough for rollback.
//!
//! Determinism gate: `cargo test -p smash_net` runs the SyncTest, which rolls back every frame
//! and compares state checksums. Any non-determinism in `step` makes it return MismatchedChecksum
//! and the test fails. Run it after touching the sim.

pub mod assets;
pub mod lobby;

use crate::v1::{InputFrame, SimState, Tune, step};
use bitflags::bitflags;
use game_input::{PlayerInput, dequantize_axis, quantize_axis};
use ggrs::{PlayerType, SessionBuilder, SyncTestSession};
use serde::{Deserialize, Serialize};
pub use smash_rollback::{Game, RollbackSim};

bitflags! {
    /// Every button edge/hold that travels over the wire, one flag each. `Buttons::empty()` = no
    /// input (also ggrs's disconnected-player default). bitflags' serde serializes the raw bits
    /// under bincode, so the packet stays a single `u16`.
    #[derive(Copy, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
    pub struct Buttons: u16 {
        const JUMP           = 1 << 0;
        const JUMP_HELD      = 1 << 1;
        const SHORTHOP       = 1 << 2;
        const SHIELD_HELD    = 1 << 3;
        const SHIELD_PRESSED = 1 << 4;
        const DOWN           = 1 << 5;
        const DOWN_PRESSED   = 1 << 6;
        const ATTACK         = 1 << 7;
        const GRAB           = 1 << 8;
        const ATTACK_HELD    = 1 << 9;
        const SPECIAL        = 1 << 10;
    }
}

/// The only game data sent over the wire. Axis and button meanings are assigned by this game;
/// storage, serialization, replay, and rollback use the shared device-independent packet.
pub type NetInput = PlayerInput;

/// Sample -> wire. The shell calls this on the locally sampled `InputFrame` before handing it to
/// the session; the result is what travels to the peer.
pub fn encode(i: &InputFrame) -> NetInput {
    let mut b = Buttons::empty();
    b.set(Buttons::JUMP, i.jump);
    b.set(Buttons::JUMP_HELD, i.jump_held);
    b.set(Buttons::SHORTHOP, i.shorthop);
    b.set(Buttons::SHIELD_HELD, i.shield_held);
    b.set(Buttons::SHIELD_PRESSED, i.shield_pressed);
    b.set(Buttons::DOWN, i.down);
    b.set(Buttons::DOWN_PRESSED, i.down_pressed);
    b.set(Buttons::ATTACK, i.attack);
    b.set(Buttons::GRAB, i.grab);
    b.set(Buttons::ATTACK_HELD, i.attack_held);
    b.set(Buttons::SPECIAL, i.special);
    NetInput {
        axes: [
            quantize_axis(i.dir),
            quantize_axis(i.aim_y),
            quantize_axis(i.cx),
            quantize_axis(i.cy),
        ],
        buttons: u32::from(b.bits()),
    }
}

/// Wire -> sim input. Both peers decode identically, so the sim sees identical floats.
pub fn decode(n: NetInput) -> InputFrame {
    let b = Buttons::from_bits_retain(n.buttons as u16);
    InputFrame {
        dir: dequantize_axis(n.axes[0]),
        aim_y: dequantize_axis(n.axes[1]),
        cx: dequantize_axis(n.axes[2]),
        cy: dequantize_axis(n.axes[3]),
        jump: b.contains(Buttons::JUMP),
        jump_held: b.contains(Buttons::JUMP_HELD),
        shorthop: b.contains(Buttons::SHORTHOP),
        shield_held: b.contains(Buttons::SHIELD_HELD),
        shield_pressed: b.contains(Buttons::SHIELD_PRESSED),
        down: b.contains(Buttons::DOWN),
        down_pressed: b.contains(Buttons::DOWN_PRESSED),
        attack: b.contains(Buttons::ATTACK),
        attack_held: b.contains(Buttons::ATTACK_HELD),
        grab: b.contains(Buttons::GRAB),
        special: b.contains(Buttons::SPECIAL),
    }
}

// Generic session glue (peer address, ggrs config binding, Netplay/GgrsNetplay, start_p2p) moved
// to the shared shell module so the V4 path never imports through `v1`. v1 keeps using it from there.
pub use crate::netplay::{
    Advance, GgrsConfig, GgrsNetplay, Netplay, NetplayEvent, start_p2p, start_p2p_n,
};

/// This game (Smash) as a [`RollbackSim`]. Keeps the wire format (`NetInput`/`encode`/`decode`), the
/// pure `step`, and the field-fold `checksum` game-specific; everything else in this crate is generic.
pub struct Smash;

impl RollbackSim for Smash {
    type State = SimState;
    type Input = NetInput;
    type Config = Tune;
    fn initial(_cfg: &Tune) -> SimState {
        SimState::spawn()
    }
    fn advance(state: &SimState, inputs: &[NetInput], cfg: &Tune) -> SimState {
        // wire -> sim input (both peers decode identically), then the pure step over N players.
        let decoded: Vec<InputFrame> = inputs.iter().map(|n| decode(*n)).collect();
        let refs: Vec<&InputFrame> = decoded.iter().collect();
        step(state, &refs, cfg)
    }
    fn checksum(state: &SimState) -> u128 {
        checksum(state)
    }
}

/// Concrete instantiations for this game, so the shell names short aliases instead of `<Smash>`.
pub type SmashConfig = GgrsConfig<Smash>;
pub type SmashGame = Game<Smash>;
pub type SmashNetplay = GgrsNetplay<Smash>;

/// Deterministic checksum over the whole state. ggrs compares these across rollbacks to catch
/// non-determinism. Folds every field's raw bits (floats via `to_bits`) through FNV-1a.
pub fn checksum(s: &SimState) -> u128 {
    let mut h: u64 = 0xcbf29ce4_84222325;
    let mut fold = |x: u64| {
        h ^= x;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    };
    for f in &s.fighters {
        fold(f.frame as u64);
        fold(f.pos.x.to_bits() as u64);
        fold(f.pos.y.to_bits() as u64);
        fold(f.vel.x.to_bits() as u64);
        fold(f.vel.y.to_bits() as u64);
        fold(f.state as u64);
        fold(f.facing.to_bits() as u64);
        fold(f.air_jumps as u64);
        fold(f.air_dodges as u64);
        fold(f.fast_falling as u64);
        fold(f.full_hop as u64);
        for s in &f.buf {
            fold(s.action as u64);
            fold(s.timer as u64);
            fold(s.aim.x.to_bits() as u64);
            fold(s.aim.y.to_bits() as u64);
        }
        fold(f.autohop_aerial as u64);
        fold(f.intangible as u64);
        fold(f.regrab_lock as u64);
        fold(f.ground_plat as u64);
        fold(f.ground_ink as u64);
        for row in &f.hit_cd {
            for c in row {
                fold(*c as u64);
            }
        }
        fold(f.hitlag as u64);
        fold(f.damage.to_bits() as u64);
        fold(f.hitstun as u64);
        fold(f.holding as u64);
        fold(f.coyote as u64);
        fold(f.invuln as u64);
        fold(f.grab_link as u64);
        fold(f.grab_timer as u64);
        fold(f.tech_buf as u64);
        fold(f.tumble as u64);
        fold(f.wall_hit as u64);
        fold(f.drop_buf as u64);
        fold(f.flick_x_age as u64);
        fold(f.flick_y_age as u64);
        fold(f.stick_was_hard_x as u64);
        fold(f.stick_was_hard_y as u64);
        fold(f.cstick_held as u64);
        fold(f.b_reversed as u64);
        fold(f.pickup_hold as u64);
        fold(f.shield_hp.to_bits() as u64);
        fold(f.shield_stun as u64);
        fold(f.charge as u64);
        fold(f.wall_touch as u64);
        fold(f.wall_nx.to_bits() as u64);
        fold(f.badges as u64);
        fold(f.char_id as u64);
        fold(f.arm as u64);
        fold(f.arm_cd as u64);
        fold(f.qb_cd as u64);
        fold(f.station as u64); // occupied ship station (station mount v1)
    }
    fold(s.tick);
    fold(s.rng);
    fold(s.helm.aim.x.to_bits() as u64);
    fold(s.helm.aim.y.to_bits() as u64);
    fold(s.helm.thrust.to_bits() as u64);
    for it in &s.items {
        fold(it.kind as u64);
        fold(it.pos.x.to_bits() as u64);
        fold(it.pos.y.to_bits() as u64);
        fold(it.vel.x.to_bits() as u64);
        fold(it.vel.y.to_bits() as u64);
        fold(it.owner as u64);
        fold(it.gas.to_bits() as u64);
        fold(it.gas_max.to_bits() as u64);
        fold(it.timer as u64);
        fold(it.facing.to_bits() as u64);
        fold(it.stroke as u64);
        fold(it.thrown as u64);
        fold(it.mount as u64); // ship-station mount (-1 = free item)
    }
    // ink paths ARE sim state (fighters stand on them, walls block off them) — fold every live
    // field. The node arrays fold only their live prefix (0..len): slots past `len` are dead
    // scratch no sim read touches, so they can't carry a divergence that matters.
    for p in &s.paths {
        fold(p.len as u64);
        fold(p.owner as u64);
        fold(p.drawing as u64);
        fold(p.kind as u64);
        fold(p.budget.to_bits() as u64);
        fold(p.pos.x.to_bits() as u64);
        fold(p.pos.y.to_bits() as u64);
        fold(p.vel.x.to_bits() as u64);
        fold(p.vel.y.to_bits() as u64);
        fold(p.percent.to_bits() as u64);
        fold(p.mass.to_bits() as u64);
        fold(p.shake as u64);
        fold(p.rot.to_bits() as u64);
        fold(p.omega.to_bits() as u64);
        fold(p.anchor as u64);
        fold(p.anchor_dir.x.to_bits() as u64);
        fold(p.anchor_dir.y.to_bits() as u64);
        for i in 0..p.len as usize {
            let node = s.nodes[p.start as usize + i];
            fold(node.pt.x.to_bits() as u64);
            fold(node.pt.y.to_bits() as u64);
            fold(p.node_born(i, &s.nodes));
            fold(node.class as u64);
        }
        fold(p.props.stroke_life as u64);
        fold(p.props.floor_tol.to_bits() as u64);
        fold(p.props.wall_tol.to_bits() as u64);
        fold(p.props.ledge_curve.to_bits() as u64);
        fold(p.props.min_seg.to_bits() as u64);
        fold(p.props.bounce.to_bits() as u64);
        fold(p.props.density.to_bits() as u64);
        fold(p.props.solid as u64);
        fold(p.props.force_wall as u64);
        fold(p.props.zone as u64);
    }
    h as u128
}

/// ggrs types a frontend needs to drive a P2P session, re-exported from the shared shell module.
pub use crate::netplay::{
    GgrsError, GgrsEvent, Message, NonBlockingSocket, P2PSession, SessionState,
};

// ---------------------------------------------------------------------------------------------
// matchbox transport (M3). Only the WebRTC glue. matchbox's own `ggrs` feature pins ggrs 0.11, so
// we take its RAW channel and implement ggrs 0.13's `NonBlockingSocket` ourselves (the pattern the
// ggrs 0.13 docs spell out). The browser app (web crate) drives the message loop + frame loop.
// ---------------------------------------------------------------------------------------------
#[cfg(feature = "matchbox")]
pub mod transport {
    use super::SmashConfig;
    use ggrs::{Message, NonBlockingSocket, SessionBuilder};
    // Re-export the ggrs + matchbox types a frontend names, so it only needs to depend on smash_net.
    pub use ggrs::{GgrsError, P2PSession, PlayerType, SessionState};
    use matchbox_socket::ChannelConfig;
    pub use matchbox_socket::{MessageLoopFuture, PeerId, PeerState, WebRtcChannel, WebRtcSocket};

    /// Wraps a matchbox channel so ggrs can send/receive its `Message`s over WebRTC. bincode for
    /// the wire; an unreliable+unordered channel (ggrs has its own reliability layer).
    pub struct Socket(pub WebRtcChannel);

    impl NonBlockingSocket<PeerId> for Socket {
        fn send_to(&mut self, msg: &Message, addr: &PeerId) {
            let bytes = bincode::serialize(msg).expect("serialize ggrs message");
            self.0.send(bytes.into_boxed_slice(), *addr);
        }

        fn receive_all_messages(&mut self) -> Vec<(PeerId, Message)> {
            self.0
                .receive()
                .into_iter()
                .filter_map(|(peer, packet)| bincode::deserialize(&packet).ok().map(|m| (peer, m)))
                .collect()
        }
    }

    /// Open the matchbox socket for a room. Returns the socket (poll `update_peers` each frame) and
    /// the message-loop future the caller MUST drive (`spawn_local` on wasm).
    pub fn connect(room_url: &str) -> (WebRtcSocket, MessageLoopFuture) {
        WebRtcSocket::builder(room_url)
            .add_channel(ChannelConfig::unreliable())
            .build()
    }

    /// Replicates matchbox's `players()` (which lives behind its ggrs-0.11 feature) for ggrs 0.13:
    /// our id plus every connected peer, sorted for a stable handle order across both peers.
    pub fn players(socket: &mut WebRtcSocket) -> Vec<PlayerType<PeerId>> {
        let Some(me) = socket.id() else {
            return vec![PlayerType::Local];
        };
        let mut ids: Vec<PeerId> = socket
            .connected_peers()
            .chain(std::iter::once(me))
            .collect();
        ids.sort();
        ids.into_iter()
            .map(|id| {
                if id == me {
                    PlayerType::Local
                } else {
                    PlayerType::Remote(id)
                }
            })
            .collect()
    }

    /// Build the rollback session once `players()` reports everyone. Handle = index in the sorted
    /// player list (identical on both peers). `input_delay` frames trade latency for fewer rollbacks.
    pub fn start_session(
        players: Vec<PlayerType<PeerId>>,
        channel: WebRtcChannel,
        input_delay: usize,
    ) -> Result<P2PSession<SmashConfig>, ggrs::GgrsError> {
        let mut builder = SessionBuilder::<SmashConfig>::new()
            .with_num_players(players.len())?
            .with_input_delay(input_delay);
        for (handle, player) in players.into_iter().enumerate() {
            builder = builder.add_player(player, handle)?;
        }
        builder.start_p2p_session(Socket(channel))
    }
}

/// Build a `num_players`-local SyncTest session (every player local, rolls back `check_distance`
/// frames each step and checksums). This is the determinism harness, not real networking; `1..=
/// crate::v1::MAX_PLAYERS` all work since `step`/`checksum` are already player-count agnostic.
pub fn synctest_session_n(
    num_players: usize,
    check_distance: usize,
) -> SyncTestSession<SmashConfig> {
    synctest_session_for::<Smash>(num_players, check_distance)
}

/// Build the same all-local rollback determinism harness for any pure simulation using this
/// crate's shared [`RollbackSim`] boundary.
pub fn synctest_session_for<S: RollbackSim>(
    num_players: usize,
    check_distance: usize,
) -> SyncTestSession<GgrsConfig<S>> {
    let mut b = SessionBuilder::<GgrsConfig<S>>::new()
        .with_num_players(num_players)
        .expect("valid player count")
        .with_check_distance(check_distance);
    for handle in 0..num_players {
        b = b
            .add_player(PlayerType::Local, handle)
            .expect("add local player");
    }
    b.start_synctest_session().expect("synctest session")
}

/// 2-player convenience wrapper — today's only caller, kept so existing call sites don't churn.
pub fn synctest_session(check_distance: usize) -> SyncTestSession<SmashConfig> {
    synctest_session_n(2, check_distance)
}

/// Input capture + deterministic replay. A live session dumps an `InputLog` (both peers' wire
/// inputs, one entry per frame); replaying it through the pure sim reproduces the match exactly.
/// Used for regression fixtures and as a second determinism check alongside the SyncTest.
pub mod replay {
    use super::{NetInput, SimState, Tune, checksum, decode, step};
    use serde::{Deserialize, Serialize};

    /// A recorded match: both players' wire inputs per frame. Serializable (bincode) so a captured
    /// session can be saved to bytes and replayed later without the engine or transport.
    #[derive(Clone, Default, PartialEq, Serialize, Deserialize)]
    pub struct InputLog {
        pub frames: Vec<(NetInput, NetInput)>,
    }

    impl InputLog {
        /// Append one frame of both peers' inputs (the shell calls this each tick to capture).
        pub fn push(&mut self, p0: NetInput, p1: NetInput) {
            self.frames.push((p0, p1));
        }
        pub fn len(&self) -> usize {
            self.frames.len()
        }
        pub fn is_empty(&self) -> bool {
            self.frames.is_empty()
        }
        /// Serialize to a compact byte blob (a fixture file or a network/debug dump).
        pub fn to_bytes(&self) -> Vec<u8> {
            bincode::serialize(self).expect("serialize input log")
        }
        /// Reload a captured log; `None` if the bytes aren't a valid log.
        pub fn from_bytes(b: &[u8]) -> Option<Self> {
            bincode::deserialize(b).ok()
        }
    }

    /// Replay a log through the pure sim from spawn, returning the per-frame state checksums. Pure
    /// and deterministic: the same log + Tune yields the same checksum stream on every run/machine.
    pub fn replay(log: &InputLog, tune: &Tune) -> Vec<u128> {
        let mut s = SimState::spawn();
        let mut out = Vec::with_capacity(log.frames.len());
        for &(p0, p1) in &log.frames {
            let i0 = decode(p0);
            let i1 = decode(p1);
            s = step(&s, &[&i0, &i1], tune);
            out.push(checksum(&s));
        }
        out
    }
}

#[cfg(test)]
mod tests;
