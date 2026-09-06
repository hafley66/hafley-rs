//! Generic ggrs rollback-session glue, shared by every shell path. Nothing here knows a game:
//! `S: RollbackSim` (from the workspace `smash_rollback` crate) supplies state/input/config.
//! Moved out of `v1::net` so the V4 path never imports through the legacy module; `v1::net`
//! re-uses these items internally.

use ggrs::{DesyncDetection, PlayerType, SessionBuilder};

pub use smash_rollback::{Game, RollbackSim};

/// Peer address type. With the `matchbox` feature this is matchbox's `PeerId` (real P2P); without
/// it (the SyncTest / default build) it is a plain `usize`, since SyncTest only uses local players
/// and never touches the address.
#[cfg(feature = "matchbox")]
pub type PeerAddr = matchbox_socket::PeerId;
#[cfg(not(feature = "matchbox"))]
pub type PeerAddr = usize;

/// Bind the shared rollback chassis to this shell's selected peer-address type.
pub type GgrsConfig<S> = smash_rollback::GgrsConfig<S, PeerAddr>;

/// ggrs types a frontend needs to drive a P2P session, re-exported so shell modules depend only on
/// this module (plus `ggrs` itself for the `NonBlockingSocket` trait they implement).
pub use ggrs::{GgrsError, GgrsEvent, Message, NonBlockingSocket, P2PSession, SessionState};

/// One frame's result from driving a netplayed session forward.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Advance {
    /// Nothing advanced this frame (still synchronizing, or ggrs is too far ahead to predict).
    Stalled,
    /// The authoritative state moved one frame (read it via [`Netplay::state`]).
    Stepped,
    /// A peer dropped — the caller should open its reconnect window.
    PeerGone,
}

/// How often (in frames) a running session compares checksums with each remote peer. Ggrs requires
/// an interval > 0 once detection is `On`; 10 @ 60hz is ~6 reports/sec per peer (the ratio ggrs's own
/// docs use as the example), a fine granularity for production telemetry without spamming the wire.
pub const DESYNC_CHECK_INTERVAL: u32 = 10;

/// One event surfaced from a live session, transport/engine-agnostic (no godot types) so any shell
/// turns it into a toast/analytics line however it likes. `PeerAddr` is the transport address type —
/// a plain `usize` handle tag for the default (non-`matchbox`) build.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NetplayEvent {
    /// ggrs lost this peer (its packets stopped arriving).
    PeerDisconnected(PeerAddr),
    /// Ggrs's own desync detector found a checksum mismatch against this peer at `frame` — the
    /// highest-value telemetry line this module emits: a live, unstaged desync in production.
    Desync {
        frame: ggrs::Frame,
        local_checksum: u128,
        remote_checksum: u128,
        addr: PeerAddr,
    },
}

/// The shell-facing netplay seam: drive a networked match without knowing the model underneath.
/// [`GgrsNetplay`] is the rollback-p2p impl used today; a future server-authoritative client would
/// be a second impl, swapped in without touching the shell. The transport (mesh vs central relay)
/// is a lower seam — ggrs's `NonBlockingSocket` — and does not surface here. See plans/n-player.md.
// reuse-kit-library(netplay-interface): generic Netplay seam + GgrsNetplay impl + start_p2p consumed by both V1 (kneeman) and V4 (v4_net::GodotPair).
pub trait Netplay {
    /// The rolled-back state the caller renders.
    type State;
    /// This peer's local input per frame (the wire form).
    type Input;
    /// Pump the transport + drain session events (must run every frame).
    fn poll(&mut self);
    /// True once the session is synchronized and actually stepping.
    fn running(&self) -> bool;
    /// Feed this peer's local input, advance one frame, and apply the resulting rollback requests.
    fn advance(&mut self, local: Self::Input) -> Advance;
    /// The latest authoritative state to render.
    fn state(&self) -> &Self::State;
    /// Drain session-level events noticed since the last call (desync reports, peer drops already
    /// folded into [`Advance::PeerGone`] too — see [`NetplayEvent`]). Call once per `poll()`.
    fn drain_events(&mut self) -> Vec<NetplayEvent>;
    /// Opt into bounded saved-frame evidence. Only frames confirmed after handled rollback
    /// requests are returned. Frame numbers are GGRS frames, not a game's own clock.
    fn confirmed_checksums(&mut self) -> Vec<(ggrs::Frame, u128)>;
}

/// Rollback-p2p netplay over ggrs, generic over the game. Owns the session + the authoritative
/// [`Game`] for the match's lifetime (created when the data channel opens, dropped on reset/reconnect).
pub struct GgrsNetplay<S: RollbackSim> {
    session: P2PSession<GgrsConfig<S>>,
    game: Game<S>,
    local_handle: usize,
    peer_gone: bool,
    events: Vec<NetplayEvent>,
    checksums: Option<std::collections::BTreeMap<ggrs::Frame, u128>>,
    confirmed: ggrs::Frame,
}

impl<S: RollbackSim> GgrsNetplay<S> {
    pub fn new(session: P2PSession<GgrsConfig<S>>, game: Game<S>, local_handle: usize) -> Self {
        Self {
            session,
            game,
            local_handle,
            peer_gone: false,
            events: Vec::new(),
            checksums: None,
            confirmed: -1,
        }
    }
}

impl<S: RollbackSim> Netplay for GgrsNetplay<S> {
    type State = S::State;
    type Input = S::Input;

    fn poll(&mut self) {
        self.session.poll_remote_clients();
        for ev in self.session.events() {
            match ev {
                GgrsEvent::Disconnected { addr } => {
                    self.peer_gone = true;
                    self.events.push(NetplayEvent::PeerDisconnected(addr));
                }
                GgrsEvent::DesyncDetected {
                    frame,
                    local_checksum,
                    remote_checksum,
                    addr,
                } => self.events.push(NetplayEvent::Desync {
                    frame,
                    local_checksum,
                    remote_checksum,
                    addr,
                }),
                _ => {}
            }
        }
    }

    fn drain_events(&mut self) -> Vec<NetplayEvent> {
        std::mem::take(&mut self.events)
    }

    fn running(&self) -> bool {
        self.session.current_state() == SessionState::Running
    }

    fn advance(&mut self, local: S::Input) -> Advance {
        if self.peer_gone {
            return Advance::PeerGone;
        }
        if !self.running() {
            return Advance::Stalled; // still synchronizing; hold the last rendered frame
        }
        if self
            .session
            .add_local_input(self.local_handle, local)
            .is_err()
        {
            return Advance::Stalled;
        }
        match self.session.advance_frame() {
            Ok(reqs) => {
                if let Some(history) = self.checksums.as_mut() {
                    self.game.handle_observed(reqs, |frame, checksum| {
                        history.insert(frame, checksum);
                        while history.len() > 600 { history.pop_first(); }
                    });
                } else {
                    self.game.handle(reqs);
                }
                self.confirmed = self.session.confirmed_frame();
                Advance::Stepped
            }
            Err(GgrsError::PredictionThreshold) => Advance::Stalled, // too far ahead; skip a frame
            Err(_) => Advance::Stalled,
        }
    }

    fn state(&self) -> &S::State {
        &self.game.state
    }

    fn confirmed_checksums(&mut self) -> Vec<(ggrs::Frame, u128)> {
        self.checksums.get_or_insert_with(Default::default).range(..=self.confirmed)
            .map(|(&frame, &hash)| (frame, hash)).collect()
    }
}

/// Build a 2-player rollback session over a caller-supplied socket (the transport). Handle order is
/// FIXED so both peers agree: handle 0 = host, handle 1 = guest. `local_handle` says which one this
/// peer is; `remote_addr` is the address the socket tags inbound packets with (host sees the guest
/// as `remote_addr`, guest sees the host). `input_delay` frames trade latency for fewer rollbacks.
pub fn start_p2p<Sim, Sock>(
    local_handle: usize,
    remote_addr: PeerAddr,
    socket: Sock,
    input_delay: usize,
) -> Result<P2PSession<GgrsConfig<Sim>>, GgrsError>
where
    Sim: RollbackSim,
    Sock: NonBlockingSocket<PeerAddr> + 'static,
{
    start_p2p_n::<Sim, Sock>(local_handle, &[remote_addr; 2], socket, input_delay)
}

/// N-player rollback session. `addrs[h]` is the transport address ggrs tags handle `h`'s packets
/// with; the entry at `local_handle` is ignored (that handle is us, played locally). `num_players`
/// = `addrs.len()`. The socket is the transport — a p2p mesh or a central relay, both just impls of
/// `NonBlockingSocket`. Both peers MUST build the same `addrs` order so handles agree.
pub fn start_p2p_n<Sim, Sock>(
    local_handle: usize,
    addrs: &[PeerAddr],
    socket: Sock,
    input_delay: usize,
) -> Result<P2PSession<GgrsConfig<Sim>>, GgrsError>
where
    Sim: RollbackSim,
    Sock: NonBlockingSocket<PeerAddr> + 'static,
{
    let mut b = SessionBuilder::<GgrsConfig<Sim>>::new()
        .with_num_players(addrs.len())?
        .with_input_delay(input_delay)
        // Production desync telemetry: compare checksums with every remote peer on a cadence and
        // surface any mismatch as `NetplayEvent::Desync` (see `Netplay::drain_events`).
        .with_desync_detection_mode(DesyncDetection::On {
            interval: DESYNC_CHECK_INTERVAL,
        });
    for (handle, &addr) in addrs.iter().enumerate() {
        let player = if handle == local_handle {
            PlayerType::Local
        } else {
            PlayerType::Remote(addr)
        };
        b = b.add_player(player, handle)?;
    }
    b.start_p2p_session(socket)
}
