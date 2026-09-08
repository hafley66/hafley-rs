# Engine-independent Rust networking and Matchbox

Research date: 2026-09-08. Source inspection, no candidate installed or compiled.
Scope: small co-op, persistent/chunked worlds, and rollback combat occurring in
the same world. Rust owns simulation; Godot is a presentation/input adapter.

## Index and evidence boundaries

- Landscape comparison: Luna survey, with primary-source review of the main candidates.
- Matchbox transport, signaling, channels, version compatibility and memory behavior.
- Mixed-world rollback implications and bounded next experiments.
- Existing local state: native GGRS 0.13 loopback/scripted-input lab;
  Godot Web/Emscripten single-player build; no connected Web multiplayer session.

## Landscape

Versions below identify inspected releases/docs, not compatibility-tested dependencies.
Browser support does not establish Godot/Emscripten side-module compatibility.
No candidate was installed. MIT/Apache means dual MIT OR Apache-2.0.

| Library | Inspected version; license | Engine-independent responsibility | Platform and integration boundary |
| --- | --- | --- | --- |
| [Naia](https://github.com/naia-lib/naia) | naia-server 0.25.0; MIT/Apache | Authoritative entity/component replication, rooms, per-user scope, typed channels, tick-buffered input | Native UDP and browser WebRTC; ECS-agnostic with optional adapters. Application supplies simulation, prediction policy and persistence |
| [Renet2](https://github.com/UkoeHB/renet2) | 0.16.0; MIT/Apache | Reliable/unreliable channels, fragmentation, configurable channel memory limits; renetcode2 authentication/encryption | Native UDP, in-memory, browser WebTransport/WebSocket; application supplies entity replication and rollback |
| [Renet](https://github.com/lucaspoffo/renet) | 2.0.0; MIT/Apache | Client/server message channels with separate transports | Native netcode/UDP and Steam integrations; no browser transport verified in this pass |
| [Matchbox](https://github.com/johanhelsing/matchbox) | 0.14.0; MIT/Apache | WebRTC peer byte channels, signaling and optional GGRS adapter | Native and browser; detailed compatibility caveats below |
| [GGRS](https://github.com/gschup/ggrs) | 0.13.0; MIT/Apache | Input prediction, rollback coordination, spectators, determinism testing | Application supplies deterministic save/load/advance and compatible socket; already used by native lab |
| [Fortress Rollback](https://github.com/wallstop/fortress-rollback) | quickstart 0.14; MIT/Apache | GGRS fork with additional session/testing APIs | Explicitly ALPHA. Documents separate browser and Godot/Emscripten integration; no local verification |
| [iroh](https://github.com/n0-computer/iroh) | 1.1.0; MIT/Apache | Public-key-addressed QUIC, NAT traversal and encrypted relay fallback | Native connectivity documented; browser documentation has a contradiction recorded below |
| [xwt](https://github.com/MOZGIII/xwt) | core 0.9.0, web 0.20.0, wtransport 0.19.0; MIT | Common WebTransport API over browser and native backends | Browser wasm32-unknown-unknown and native wtransport; repository describes work in progress |
| [wtransport](https://github.com/BiagioFesta/wtransport) | 0.7.1; MIT/Apache | HTTP/3 WebTransport streams/datagrams | Native implementation; browser peers use browser WebTransport API |
| [Quinn](https://github.com/quinn-rs/quinn) | 0.11.11; MIT/Apache | QUIC streams/datagrams; quinn-proto sans-I/O protocol | Native endpoints; browser cannot directly open its UDP socket |
| [s2n-quic](https://github.com/aws/s2n-quic) | 1.88.0; Apache-2.0 | QUIC transport | Native platforms; browser endpoint not verified |
| [netcode-official](https://github.com/mas-bandwidth/netcode.rs) | 1.0.0; BSD-3-Clause | Secure UDP connection tokens and protocol protections | Native UDP; gameplay replication and reliability remain separate |
| [turbulence](https://github.com/kyren/turbulence) | 0.4.0; MIT/Apache/CC0 | Transport-agnostic typed channels, compression, multiplexing and flow control | Older release, unstable API warning; current browser integration unverified |
| [laminar](https://github.com/TimonPost/laminar) | 0.5.0; MIT/Apache | UDP reliability, ordering, fragmentation and heartbeat | Native; older release lineage, no browser/authentication layer established |
| [Backroll](https://github.com/HouraiTeahouse/backroll-rs) | 0.6.0; ISC | Rollback with UDP/Steam integrations | Early-beta warning; current browser support unverified |
| [Crystalorb](https://github.com/ErnWong/crystalorb) | Version/license not verified | Client prediction, server reconciliation, mock networking, Rapier example | Follow-up lead; nightly/incomplete documentation, current target coverage unverified |

[Lightyear](https://github.com/cBournhonesque/lightyear), inspected docs 0.29.0,
provides replication/prediction/rollback/interest management but is Bevy-coupled.
It is recorded as comparative prior art outside the requested engine-independent set.

### Candidate distinctions for this game

These are architectural mappings, not benchmark results:

- Naia exposes entity replication and per-client visibility suitable for evaluating
  a shared world with entities outside each player's immediate view.
- Renet2 exposes message channels and native/browser transports while leaving the
  world schema, replication and reconciliation policy in the application.
- Matchbox plus a compatible rollback adapter covers small-group peer input
  exchange. World/chunk synchronization and late-join snapshots need additional policy.
- iroh addresses peer reachability and identity; Quinn/wtransport/xwt address
  transport. None of these alone defines game authority or rollback state.
- These are alternative layers/candidates, not dependencies to install together.

### Source caveats and follow-up leads

The [iroh FAQ](https://docs.iroh.computer/about/faq) currently says both that iroh
works in browsers and that it does not. Its browser-start link redirects to the
[language overview](https://docs.iroh.computer/languages). Treat exact browser
transport/relay behavior and Emscripten compatibility as unverified here; native
hole-punching claims cannot be transferred to browser APIs.

The [Fortress guide](https://wallstop.github.io/fortress-rollback/user-guide/)
explicitly distinguishes wasm32-unknown-unknown from wasm32-unknown-emscripten,
documents an application-provided NonBlockingSocket for Godot, and describes a
browser Matchbox adapter with bounded per-poll decoding. It warns that Matchbox's
upstream GGRS trait implementation does not implement Fortress's trait. These are
maintainer documentation and CI claims, not tests reproduced in this lab; bounded
polling also does not bound Matchbox's underlying queues. The repository labels
the project ALPHA.

The survey records published releases and primary documentation rather than star
counts. Older projects and unverified targets remain explicitly separated from
integration evidence. Release freshness alone does not establish correctness.

## Matchbox metadata and documentation inventory

[Repository](https://github.com/johanhelsing/matchbox),
[socket docs](https://docs.rs/matchbox_socket/0.14.0/matchbox_socket/),
[release v0.14.0](https://github.com/johanhelsing/matchbox/releases/tag/v0.14.0).
GitHub latest release verified as v0.14.0, published 2026-02-13. MIT OR Apache-2.0.
Version-pinned source entry points inspected:

- `matchbox_socket/Cargo.toml`: platform dependencies and GGRS version.
- `matchbox_socket/src/webrtc_socket/socket.rs`: builder, channels, queues, ICE.
- `matchbox_socket/src/ggrs_socket.rs`: GGRS socket implementation and player ordering.
- `examples/simple/src/main.rs` and `.cargo/config.toml`: non-engine lifecycle and browser target.
- `matchbox_server/README.md`: room grouping and deployment command.
- `matchbox_signaling/examples/client_server.rs`: host/client topology and hooks.
- `examples/custom_signaller/README.md`: iroh-based signaling without matchbox_server.
- `examples/error_handling/README.md`: failure logging and timed disconnect example.
- Repository README and release history: package boundaries and shipped examples.
- Issues #492, #469 and #334: queue pressure, setup race, relay observability.

These are the inspected documentation/source entry points, not an exhaustive API
or issue inventory. GitHub Discussions were not separately audited.

## Matchbox capability matrix

| Component | Responsibility | Application still supplies |
| --- | --- | --- |
| `matchbox_socket` | WebRTC peer connections and byte channels, native/browser backends | Payload schema, application limits, simulation |
| `matchbox_server` | Ready-made full-mesh signaling and next-N room grouping | Hosting, room access policy, credentials, operational limits |
| `matchbox_signaling` | Embedded/custom signaling, including host/client example | Session lifecycle and game authority |
| `matchbox_socket/ggrs` | GGRS socket adapter | Compatible GGRS version, deterministic state and save/load/advance |
| `bevy_matchbox` | Optional Bevy integration | Unneeded for this engine-independent lab |

Signaling exchanges connection setup messages. Gameplay packets then travel over
WebRTC data channels. A TURN service can relay gameplay when direct connectivity
fails; it is separate from the signaling server. Room URLs can use `?next=2` to
group arriving peers in pairs. See the [server README](https://github.com/johanhelsing/matchbox/blob/v0.14.0/matchbox_server/README.md).

The signaling library also ships a [host/client example](https://github.com/johanhelsing/matchbox/blob/v0.14.0/matchbox_signaling/examples/client_server.rs).
Its connection hook explicitly allows all requests in the example. A host/client
connection topology does not itself implement authoritative game simulation.

The [custom-signaller example](https://github.com/johanhelsing/matchbox/blob/v0.14.0/examples/custom_signaller/README.md)
uses iroh for signaling, replacing matchbox_server while retaining Matchbox WebRTC
data transport. It documents native and wasm32-unknown-unknown execution; neither
path was run here. This is an existing composition example, not a required stack.

## API and lifecycle

Illustrative v0.14 API fragment, inspected against source but not compiled here:

```rust
use matchbox_socket::{ChannelConfig, WebRtcSocketBuilder};

let (mut socket, message_loop) = WebRtcSocketBuilder::new(
    "wss://signal.example/world-version/session?next=4",
)
.add_channel(ChannelConfig::unreliable()) // tick/input traffic
.add_channel(ChannelConfig::reliable())   // ordered control messages
.build();

// Keep message_loop polled on the appropriate executor for this target.
// Drain socket.update_peers() and channel_mut(index).receive().
// Send owned packet bytes using channel_mut(index).try_send(packet, peer).
```

The official [non-engine example](https://github.com/johanhelsing/matchbox/blob/v0.14.0/examples/simple/src/main.rs)
contains a complete lifecycle: native Tokio or browser spawn_local, socket updates,
send/receive and termination when the message-loop future completes. Dropping that
future makes sends fail; `send` panics on that error, while `try_send` returns it.
Do not leave a polled future idle indefinitely or place it inside simulation state.

The [GGRS adapter](https://github.com/johanhelsing/matchbox/blob/v0.14.0/matchbox_socket/src/ggrs_socket.rs)
sorts peer IDs for consistent player order and warns when used on a reliable/ordered
channel. GGRS supplies its own input redundancy; ordered retransmission can delay
newer input behind older packets. Room/player membership changes still need an
application policy.

## Compatibility and allocation limits for this lab

The [v0.14 manifest](https://github.com/johanhelsing/matchbox/blob/v0.14.0/matchbox_socket/Cargo.toml)
depends on optional GGRS **0.11**, while this lab uses **0.13**. Its built-in adapter
therefore does not directly implement the lab's GGRS trait version. Resolve the
version combination or evaluate a compatible adapter before integration.

The wasm32 dependency branch includes wasm-bindgen, web-sys, js-sys and
wasm-bindgen-futures. The [simple example's target](https://github.com/johanhelsing/matchbox/blob/v0.14.0/examples/simple/.cargo/config.toml)
is `wasm32-unknown-unknown`. This lab is an Emscripten side-module inside Godot.
Browser support alone does not verify that loader/ABI combination. Previous lab
work removed unrelated wasm-bindgen imports to make the Godot module load.
No claim is made that integration is impossible; a separate transport module or
Godot WebRTC bridge are candidate boundaries requiring tests.

[Channel source](https://github.com/johanhelsing/matchbox/blob/v0.14.0/matchbox_socket/src/webrtc_socket/socket.rs)
uses unbounded inbound/outbound MPSC queues. `receive()` assembles a new Vec;
the GGRS adapter serializes messages into owned packets. This fails to establish
the lab's desired bounded/zero-allocation transport contract. Application-side
send caps do not by themselves bound every internal queue.

`RtcIceServerConfig` accepts URLs, username and credential. The defaults contain
Google STUN URLs. Production TURN needs a supplied service and credential policy.
The author's [0.4 announcement](https://johanhelsing.studio/posts/matchbox-0-4)
documents TURN support and testing in NES Bundler. Secondary summaries claiming
Matchbox has no TURN configuration contradict this source and current code.

## Maintainer and issue signals

- [#492, open](https://github.com/johanhelsing/matchbox/issues/492): outbound
  backpressure API request; current source inspection independently confirms
  unbounded queues. Relevant to bulk world/chunk transfer.
- [#469, open](https://github.com/johanhelsing/matchbox/issues/469): user reports
  missed IdAssigned with frame-polled browser futures on 0.10.0. Treat as a
  regression-test lead, not proof the current release reproduces it.
- [#334, open](https://github.com/johanhelsing/matchbox/issues/334): request to
  expose whether a peer uses a TURN relay. Relay observability needs checking.
- [Author tutorial](https://johanhelsing.studio/posts/extreme-bevy): explains
  Matchbox plus GGRS separation. Its Bevy glue is optional; use release-pinned
  manifests rather than copying tutorial dependency versions.
- Latest releases verified through GitHub API: 0.12.0 on 2025-05-22; 0.13.0 on
  2025-10-25; 0.14.0 on 2026-02-13. 0.13 added path-based peer isolation; 0.14
  updates native WebRTC dependencies and optional Bevy compatibility. See
  [release history](https://github.com/johanhelsing/matchbox/releases).

## Co-op, persistent terrain and combat in the same world

Design implications, not claims that a surveyed library implements this game:

- One small co-op group can share the same session for movement, combat and
  interactions. A moveset change need not establish a new transport connection.
- Persistent terrain needs chunk transfer, revision/ownership rules, reconnect
  snapshots, late-join catch-up and durable confirmed updates in addition to input
  delivery. SQLite can retain confirmed state at the owned storage boundary.
- If rollback combat interacts with terrain, moving platforms, projectiles or
  nearby actors, those dependencies must be restored/replayed consistently or
  supplied as tick-addressed authoritative inputs. Rolling back fighters alone
  while reading unversioned world changes can change the outcome.
- Effects written outside the rollback snapshot need confirmation/deduplication
  or a rollback-aware log. Network topology alone does not define this policy.
- Reliable bulk traffic and latency-sensitive inputs need separate budgets and
  priority rules. Separate channel IDs still share underlying network capacity.
- A full mesh has N(N-1)/2 peer links: 4 players means 6 links, 8 means 28.
  This is a connection-count calculation, not a measured performance limit.

## Next verification gates

1. Native two-peer packet exchange using a pinned candidate, then browser to
   browser and native to browser, before adding gameplay.
2. GGRS version/type compatibility and Godot/Emscripten loading proof.
3. Delayed/dropped/duplicate inputs, reconnect, forced relay, background tab and
   cancellation tests with queue sizes and allocation measurements.
4. Two clients replay the same terrain edit, projectile hit and moving platform
   during rollback; assert state hashes and confirmed persistence exactly once.
5. Preserve independent expected traces and an MP4 with actual connection,
   confirmation, prediction, rollback and transport status labels.

No deployment, dependency adoption, performance benchmark, mobile test or network
compatibility execution was performed by this research pass.
