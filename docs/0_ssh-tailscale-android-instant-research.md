# SSH, Tailscale, tmux, Boop, and Instant on Android

Research date: 2026-09-06<br>
Boop checkout: `2ea68efff2a5afb15c3f229d4c05e837a080ef76`<br>
Instant checkout inspected read-only: `6f2786cb8c95311a943ba3f39bd45645f4c6134e`

## 1. Scope and evidence labels

This report contains no tailnet names, addresses, keys, tokens, SSH configuration contents, or
machine names. No connection, authentication, ACL, firewall, `sshd`, Serve, Funnel, or deployment
state was changed.

Evidence labels used below:

- **Repository**: observed in the named source at the commits above.
- **Executed**: a bounded local command completed during this research.
- **Upstream**: stated by a linked primary project or platform source.
- **Proposed**: an integration shape inferred from repository and upstream behavior. It is not
  implemented or verified on a network.

Local versions were OpenSSH 9.7p1, tmux 3.7b, Tailscale 1.98.8, Node 24.15.0, pnpm 11.10.0, and
the Instant-local Playwright 1.61.1. Current upstream releases checked on 2026-09-06 were
[OpenSSH 10.5p1](https://www.openssh.com/releasenotes.html),
[tmux 3.7c](https://github.com/tmux/tmux/releases/tag/3.7c),
[Tailscale 1.102.3](https://github.com/tailscale/tailscale/releases/tag/v1.102.3), and
[Playwright 1.63.0](https://github.com/microsoft/playwright/releases/tag/v1.63.0). The semantic
code search MCP named in `AGENTS.md`, `adb`, and standalone Chromium/Chrome executables were
unavailable. Repository search used `rg`; Playwright's managed browser availability was not
changed or tested.

## 2. Verified implementation inventory

### 2.1 Boop harness and transport seams

| Interface | Exact signature or call site | Current boundary |
|---|---|---|
| Lane configuration | `ChannelSpec { model, effort, cwd, resume, lane, executable }` in `crates/boop-acp/src/channel.rs:116` | Contains no host, SSH, or network endpoint. |
| Live conversation | `trait LaneChannel: Send` with `start_turn`, `steer`, `next_event`, `interrupt`, `last_activity_ms`, and `close` at `crates/boop-acp/src/channel.rs:136` | Abstracts the harness protocol process. `Delivery::{MidTurn, NextTurn}` expresses steering acceptance. |
| Harness construction | `Harness::open_channel(&ChannelSpec) -> Result<Box<dyn LaneChannel>>` at `crates/boop-harness/src/harness.rs:422` | The supervisor receives a provider-neutral conversation. |
| Supervisor | `run(lane: LaneRun, channel: &mut dyn LaneChannel) -> Result<i32>` at `crates/boop-proc/src/supervise.rs:518` | Owns mailbox draining, turn polling, stall handling, parent liveness, result recording, and channel close. |
| Construction call | `adapter.open_channel(&spec)` then `supervise::run(run, channel.as_mut())` at `crates/boop/src/cli/job.rs:377-407` | SSH cannot be inserted by adding fields only to `LaneChannel`; tmux and parent probes remain local. |
| tmux abstraction | `trait Multiplexer` at `crates/boop-mux/src/lib.rs:19` | Every method receives `socket: Option<&str>`; the sole production implementation is local `Tmux`. |

`Multiplexer` currently exposes `current_pane`, `session_of_pane`, `pane_id`, `pane_pid`,
`live_sessions`, `has_session`, `kill_session`, `target_alive`, `capture_pane`,
`new_detached_session`, `new_bare_session`, `new_window`, `swap_windows`, and `kill_window`
(`crates/boop-mux/src/lib.rs:22-76`). Production callers obtain the process-global local adapter
through `boop_store::tmux::mux()`, including lane creation and cleanup in
`crates/boop/src/cli/mail.rs:715-760`, parent liveness in
`crates/boop-proc/src/supervise.rs:849,1095`, and capture in
`crates/boop/src/cli/job.rs:2262`. The socket parameter is tmux server identity on one host; it
does not identify a host.

Boop 0.0.10, `boop-mux` 0.0.9, and the 0.0.2 `boop-acp`, `boop-harness`, `boop-proc`, and
`boop-store` crates are present. No SSH crate, SSH target type, remote command runner, remote tmux
adapter, HTTP server, or browser client is present.

### 2.2 Instant server, browser, session, and tmux paths

Instant 0.1.21 is a Tauri 2 application. The frontend operations inspected here use Tauri
`invoke` and application events. `src/reactive/httpTransport.ts:13` is ordinary `fetch` with a
2-second abort signal and JSON-only response decoding, but its generated endpoints point to the
loopback activity and ghcache services. `NATIVE_HTTP.md:14-20` declares seven worktree/config
routes. Its implementation plan still says loopback-only, fresh process bearer token, SSE for
one-way events, and WebSockets for future PTY/CDP sessions
(`docs/PLAN-native-http.md:47-57,114-115,160-166`). Those declarations are plans, not a running
Instant application API.

The only HTTP listener implemented by Instant itself is the activity server at
`127.0.0.1:8787` in `src-tauri/src/activity.rs:320-335`. It has permissive
`Access-Control-Allow-Origin: *` for the browser extension and routes for config, heartbeat,
matches, diagnostics, and ingest.
It is not the application host and is not an authentication boundary. Generated ghcache and
activity URLs are fixed to `127.0.0.1:7748` and `127.0.0.1:8787` in `src/generated/api.ts`.
The worktree `EventSource` closes on a hard error and waits for the next scan to subscribe again
(`src/worktrees.ts:1381-1395`).

The terminal boundary is:

```rust
pub async fn open_session(
    app: AppHandle,
    store: State<'_, PtyStore>,
    events: State<'_, PtyEvents>,
    id: String,
    name: String,
    tmux_target: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    cols: u16,
    rows: u16,
    graphics: Option<bool>,
    cell_w: Option<u16>,
    cell_h: Option<u16>,
    attach_only: Option<bool>,
) -> Result<(), String>;

pub fn write_pty(store: State<PtyStore>, id: String, data: String) -> Result<(), String>;
pub fn resize_pty(
    store: State<PtyStore>, id: String, cols: u16, rows: u16,
    cell_w: Option<u16>, cell_h: Option<u16>,
) -> Result<(), String>;
pub fn close_pty(store: State<PtyStore>, id: String);
pub async fn kill_session(store: State<'_, PtyStore>, name: String) -> Result<(), String>;
```

These are at `src-tauri/src/pty.rs:576,823,834,861,910`. `PtyStore` is a
`Mutex<HashMap<String, PtyHandle>>` owned by the Tauri process (`pty.rs:51-52`). A handle owns a
writer and PTY master. Direct children are owned and killed on close; tmux clients are not retained
as children, so closing a tab drops the local PTY while the tmux server and session continue
(`pty.rs:707-730,857-866`). Output enters a 256-item synchronous queue, batches up to 256 chunks
or 16 ms, and emits `pty-data-batch` to the webview
(`src-tauri/src/0_pty_events.rs:9-11,38-112`). There is no sequence number, replay cursor, browser
WebSocket, reconnect state, or per-browser ownership record.

Release builds default to tmux socket `instant-prod`; debug uses the default socket, and
`INSTANT_TMUX_SOCKET` overrides both (`pty.rs:104-130`). Existing attach paths use
`attach-session -d` and new paths use `new-session -A -D`, explicitly detaching another client to
enforce one current size (`pty.rs:157-172,661-682`). This would detach a desktop Instant terminal
when an Android terminal attaches to the same tmux session.

Instant already calls Boop and `boop-mux` locally:

- `boop_mux_capture(target: String, socket: Option<String>) -> Result<String, String>` at
  `src-tauri/src/0_tmux.rs:15`.
- `boop_mux_send_keys(body: String, target: Option<String>, socket: Option<String>, mode:
  Option<String>) -> Result<String, String>` at `src-tauri/src/0_tmux.rs:73`. It leaves copy mode,
  loads a tmux paste buffer, waits 400 ms, and submits. `mode="escape"` sends Escape.
- `boop_mux_session(target: String, socket: Option<String>) -> Result<Option<String>, String>` at
  `src-tauri/src/0_harness_store.rs:13`.
- Read APIs for turns, recent turns, lanes, lane events, annotations, comments, presets,
  favorites, turn location, agent touches, and session graph are Tauri commands in
  `src-tauri/src/0_boop.rs:225-1437`.

### 2.3 Responsive and Android surface

The active Instant source has 0 CSS `@media` rules and 0 references to `safe-area`,
`viewport-fit`, `interactive-widget`, `visualViewport`, or `virtualKeyboard`. It has 0 active web
app manifest links or service-worker registrations; a vendored miniPaint manifest link is
commented out. `index.html:6` supplies only `width=device-width, initial-scale=1.0`.

The app window is fixed with a 6 px inset and the workbench is a horizontal flex row
(`src/styles.css:137-144,315-330`). The activity rail is 148 px in big mode or 44 px in compact
mode; top-level items have a 28 px minimum height (`styles.css:346-376`). Some nested rows use 20
px minimum heights (`styles.css:462`). WCAG 2.2 AA target-size guidance requires a 24 by 24 CSS
pixel target or its spacing/equivalent exceptions; 44 by 44 is the enhanced AAA criterion
([W3C 2.5.8](https://www.w3.org/WAI/WCAG22/Understanding/target-size-minimum.html),
[W3C 2.5.5](https://www.w3.org/WAI/WCAG22/Understanding/target-size-enhanced.html)). Actual target
boxes and spacing still require rendered measurement.

Navigation is one activity rail plus Dockview. Built-in rail panels are `sessions`, `worktrees`,
`activity`, `boop`, `favorites`, `config`, and `status` (`src/panels.ts:67-130`,
`src/favorites.ts:437-441`). A fresh Dockview layout contains the `sessions` panel; saved group,
split, and tab layout is restored from `settings.dockJSON` (`src/reactdock.tsx:356-417`). Terminal
turns are identified over the xterm surface, and a selected/favorited turn can open as a Dockview
preview split (`src/favorites.ts:286-324`). There is no mobile navigation mode or turn-pane
breakpoint. Recurring reminders, the Instant turn widget, and favorite-reason changes belong to a
separate implementation lane and are outside this report.

`SessionSidebar` accepts `{ sid, getCwd, width, placement: "right" | "bottom", onWidth,
onResizeEnd }` and clamps pointer resizing to 160 through 560 px
(`src/sessionSidebar.tsx:4-37`). Its state defaults to closed, 420 px, with right placement and is
persisted per session (`src/reactdock.tsx:220-288`). CSS supports bottom placement
(`styles.css:687-723`), but no viewport rule selects it. Dockview layout is persisted through
`settings.dockJSON` (`reactdock.tsx:346,394`).

xterm input uses a hidden textarea. Host `mousedown` focuses it, textarea focus/blur updates
terminal ownership, `term.onData` invokes `write_pty`, and `term.onResize` invokes `resize_pty`
(`src/terminal.ts:1019-1050`). The custom key handler is macOS-oriented and handles Cmd, Alt,
Shift+Enter, clipboard, and kitty sequences (`terminal.ts:1052-1110`). Focus after tab changes is
retried by animation frame and after 60 ms (`terminal.ts:1163-1170`). Composition events, Android
IME action keys, voice input, the VirtualKeyboard API, and visual viewport changes have no
explicit handling.

OS uploads are Tauri-native: the separate dropcatcher receives Finder paths, stashes them, and
emits `os-file-drop` (`src/dropcatcher.ts:1-47`, `src/dnd.ts:1-146`). There is no application-level
browser `<input type="file">`, multipart upload route, or streamed upload protocol. Pointer events
exist for resize and terminal interactions, and some canvas/resize surfaces set
`touch-action: none`; this is not a complete touch layout.

## 3. Upstream network and persistence capabilities

### 3.1 Android browser to a private Instant host

**Upstream.** Tailscale Serve exposes a local service only inside the tailnet and requires HTTPS
certificates ([Serve](https://tailscale.com/docs/features/tailscale-serve)). A Serve reverse proxy
can map an HTTPS tailnet FQDN to a loopback HTTP listener. On macOS App Store and standalone
variants, port proxying works while direct file/directory serving is restricted
([Serve examples](https://tailscale.com/docs/reference/examples/serve)). Browsers do not treat
WireGuard encryption as HTTPS. Tailscale certificates cover the full `machine.tailnet.ts.net`
name, expose that FQDN in certificate-transparency logs, and expire after 90 days when manually
provisioned ([HTTPS certificates](https://tailscale.com/docs/how-to/set-up-https-certificates)).

Serve removes caller-supplied Tailscale identity headers, inserts authenticated identity headers
for ordinary tailnet users, omits them for tagged sources, and may include external users with an
accepted node share. Tailscale says a backend trusting these headers should listen only on
localhost ([Serve identity headers](https://tailscale.com/docs/features/tailscale-serve#identity-headers)).
Tailnet device keys stop endpoint traffic when they expire; the default for new domains is 180
days ([key expiry](https://tailscale.com/docs/features/access-control/key-expiry)). This expiry is
separate from any Instant browser session.

**Proposed topology.** Keep the Instant HTTP/WebSocket listener on `127.0.0.1:<fixed-port>` and put
Tailscale Serve on the tailnet HTTPS origin. This preserves the planned loopback trust boundary
and allows one same-origin `https://` plus `wss://` browser surface. A direct LAN/tailnet listener
is a second configuration: bind to an explicit interface address, terminate TLS in Instant or a
local reverse proxy, and retain application authentication. Binding Vite or the application to
`0.0.0.0` expands the listener to every interface and should remain a development-only recipe.
Vite defaults to localhost; `server.host` changes that and proxied HMR needs WebSocket forwarding
([Vite server options](https://vite.dev/config/server-options)).

The Serve documentation confirms HTTP reverse proxying but does not explicitly document
WebSocket Upgrade behavior on the pages reviewed. Treat WSS-through-Serve as an integration
check, not an upstream guarantee in this report.

Remote pages need HTTPS for secure-context APIs; loopback HTTP is considered potentially
trustworthy only when it is local to the browser
([secure contexts](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Secure_Contexts)).
A PWA needs an active manifest for cross-browser installability, and service workers require a
secure context
([web app manifest](https://web.dev/learn/pwa/web-app-manifest/),
[Service Worker API](https://developer.mozilla.org/en-US/docs/Web/API/Service_Worker_API)).

### 3.2 SSH paths and tmux lifetime

| Path | Upstream behavior | Integration consequence |
|---|---|---|
| Direct OpenSSH | `ssh host command...` logs in and executes a remote command. Host config resolves command line, user, then system configuration. | Reuse the system client and the user's host aliases. Do not copy private key bytes into Boop state. |
| ProxyJump | `-J` or `ProxyJump` connects to the jump host and forwards TCP to the final host. Destination command-line options generally do not configure the jump host. | Keep jump-host settings in SSH config or model jump and destination independently. See [ssh(1)](https://man.openbsd.org/ssh) and [ssh_config(5)](https://man.openbsd.org/ssh_config). |
| OpenSSH over Tailscale | Ordinary SSH works over a Tailscale IP or MagicDNS name and still uses the destination's OpenSSH keys and host verification. | The network path changes; SSH ownership and known-host handling do not. |
| Tailscale SSH | Tailscale claims tailnet IP port 22, authenticates with tailnet identity, distributes host keys, and applies both network and SSH policy. The server is available on Linux and the open-source `tailscale` plus `tailscaled` macOS variant; clients may be any Tailscale platform. | Keep it a selectable host policy. The standard macOS Tailscale app can still carry ordinary OpenSSH traffic. See [Tailscale SSH](https://tailscale.com/docs/features/tailscale-ssh). |

OpenSSH `StrictHostKeyChecking=yes` refuses unknown and changed host keys; `accept-new` adds new
keys while still refusing changed keys. A mismatch must be surfaced with the destination and
fingerprint and must not trigger an automatic deletion or retry with checking disabled
([ssh_config host verification](https://man.openbsd.org/ssh_config#StrictHostKeyChecking)).
`IdentitiesOnly=yes` restricts attempts to configured identities even when an agent offers more.
`IdentityAgent` selects or disables the agent socket
([ssh_config identities](https://man.openbsd.org/ssh_config#IdentitiesOnly)). Agent forwarding is
off by default; a remote user able to access the forwarded socket can request signatures, so the
Boop transport has no reason to turn it on automatically.

The installed OpenSSH 9.7 predates OpenSSH 10.3's additional validation of command-line `-J`
user/host strings. Any future structured target input must accept host aliases and validated
host/user/port fields rather than interpolating untrusted strings into a shell command
([OpenSSH 10.3 notes](https://www.openssh.com/releasenotes.html#OpenSSH_10.3)).

tmux sessions outlive detached clients and can be reattached. `-L socket-name` selects a separate
server socket. Multiple clients may attach; `attach-session -d` and `new-session -A -D` detach
other clients. See the tmux project's
[Getting Started guide](https://github.com/tmux/tmux/wiki/Getting-Started). For a remote session,
the SSH child and PTY are connection lifetime; the remote tmux server owns shell and harness
process lifetime. Network loss should close or replace the SSH client without killing tmux.
Reconnect should run an exact-session attach again.

Tailscale SSH policy changes can terminate existing SSH connections, enabling it can hang current
tailnet-port-22 connections, and restarting `tailscaled` terminates Tailscale SSH sessions. Remote
tmux continues only if the tmux server and its host process remain alive. Ordinary OpenSSH over a
tailnet has OpenSSH's process behavior and Tailscale's network-key expiry behavior.

## 4. Proposed interfaces, lifetime, and storage

### 4.1 Browser host boundary

Type signatures precede the implementation sequence:

```rust
pub struct BrowserSessionId([u8; 32]);
pub struct TerminalLeaseId([u8; 16]);

pub struct BrowserSession {
    pub subject: String,
    pub issued_at_ms: u64,
    pub last_seen_ms: u64,
    pub expires_at_ms: u64,
}

pub enum PtyClientFrame {
    Attach { terminal: String, cols: u16, rows: u16, resume_seq: Option<u64> },
    Input { lease: TerminalLeaseId, data: String },
    Resize { lease: TerminalLeaseId, cols: u16, rows: u16 },
    Detach { lease: TerminalLeaseId },
    Interrupt { lease: TerminalLeaseId },
}

pub enum PtyServerFrame {
    Attached { lease: TerminalLeaseId, generation: u64, next_seq: u64 },
    Output { generation: u64, seq: u64, data: String },
    SnapshotRequired,
    AuthExpired,
    Error { code: String, detail: String },
}
```

**Lifetime.** The Instant host process owns `PtyStore`, tmux clients, browser sessions, and output
sequence counters. A WebSocket owns one attachment lease. A disconnect releases only the lease
after a short grace period; it does not invoke `kill_session` or inject terminal input. An explicit
Interrupt maps to the existing harness-aware interrupt or `boop_mux_send_keys(mode="escape")`.
An explicit Kill remains a separate destructive operation.

**Storage.** Store only a hash of each opaque browser session ID with subject and idle/absolute
expiry. Send the ID in a `Secure`, `HttpOnly`, `SameSite=Strict` cookie. Do not store Tailscale
headers, SSH private keys, agent sockets, terminal input, or PTY output in that session table.
Keep a bounded per-terminal sequence ring in memory, or reply `SnapshotRequired` and reconstruct
from tmux capture. Existing durable Boop transcripts remain the source for structured turns.

**Reads and writes.** Serve forwards an HTTPS request to loopback. Instant validates the trusted
Serve identity header against an allowlist, creates or refreshes the browser session, checks exact
Origin on mutating HTTP and WebSocket upgrades, then attaches. Each accepted PTY output chunk gets
one monotonically increasing sequence before fan-out. Client input and resize require the current
lease. Reconnect presents its last sequence; the server replays the bounded gap or sends a
snapshot-required response. An expired application session returns HTTP 401 before upgrade and
the client stops automatic retries until a new authenticated page load.

The browser `WebSocket` API exposes open/message/error/close and `bufferedAmount`; it has no
built-in backpressure or reconnect mechanism
([WebSocket API](https://developer.mozilla.org/en-US/docs/Web/API/WebSocket)). Use bounded queues,
an application heartbeat, exponential retry with jitter and a cap, online/visibility wakeups, and
the sequence handshake above. Retry network closure. Pause on 401, forbidden subject, protocol
error, or terminal-not-found.

### 4.2 Optional `boop-ssh` boundary

The current code grounds an SSH module at the command/PTY boundary below `Multiplexer`, not inside
`LaneChannel`:

```rust
pub struct SshTarget {
    pub host_alias: String,
    pub user: Option<String>,
    pub port: Option<u16>,
    pub proxy_jump: Option<String>,
}

pub enum SshFailure {
    HostKeyUnknown,
    HostKeyChanged,
    Authentication,
    Unreachable,
    RemoteExit(i32),
    Cancelled,
    Protocol(String),
}

pub trait SshExec: Send + Sync {
    fn output(&self, target: &SshTarget, command: &RemoteCommand, stdin: &[u8])
        -> Result<std::process::Output, SshFailure>;
    fn open_pty(&self, target: &SshTarget, command: &RemoteCommand, size: PtySize)
        -> Result<Box<dyn RemotePty>, SshFailure>;
}

pub trait RemotePty: Send {
    fn read(&mut self, dst: &mut [u8]) -> std::io::Result<usize>;
    fn write(&mut self, src: &[u8]) -> std::io::Result<()>;
    fn resize(&mut self, size: PtySize) -> Result<(), SshFailure>;
    fn close(&mut self) -> Result<(), SshFailure>;
}
```

`RemoteCommand` should be a closed enum of the tmux operations already present in
`Multiplexer`, with validation for host aliases, users, ports, socket names, and tmux targets. A
system-OpenSSH implementation builds an argv directly and inherits the caller's SSH config and
agent. `BatchMode=yes` belongs on noninteractive Boop probes. Interactive terminal attach owns a
real PTY and may prompt according to the user's SSH configuration. Never serialize key contents
or `SSH_AUTH_SOCK`; do not enable `ForwardAgent` implicitly.

**Lifetime.** Each one-shot tmux query owns one `ssh` process. Each interactive attach owns one
SSH process and local PTY. OpenSSH may reuse a user-configured ControlMaster, whose lifetime stays
owned by OpenSSH. The remote tmux server owns the remote session after disconnect.

**Storage and uniqueness.** A future endpoint key must be `(ssh destination identity, tmux socket,
tmux target)`. Current `(socket, target)` registry fields are insufficient across hosts. Store host
aliases and endpoint IDs, not expanded config, fingerprints, or credentials. A single-writer lease
can preserve Instant's current sizing rule. Multi-client mode requires an explicit tmux
`window-size` policy and removal or selection of current `-d`/`-D` behavior.

**Read/write sequence.** Resolve the endpoint, start `ssh` with direct argv, let OpenSSH verify the
host and authenticate, execute one closed tmux command, classify exit status/stderr, and record no
secret-bearing output. Interactive reconnect starts a new client and attaches the same exact tmux
target. Cancellation terminates the local SSH child. Session kill is a separate remote tmux
operation.

Do not implement `SshMultiplexer: Multiplexer` before making host identity explicit in the
interface. Its current `Option` results also collapse “remote host unreachable” and “tmux target
missing” in several methods. A bounded first implementation can add `boop-ssh` with the two
traits above, validate direct and ProxyJump argv generation, and use it only in an Instant remote
terminal prototype.

## 5. Android layout and input changes

1. Add `interactive-widget=resizes-content` to the viewport policy, use `100dvh` for the app
   height, and include `env(safe-area-inset-*)` in the outer insets. Chrome documents the three
   keyboard resize modes and the `interactive-widget` values
   ([Chrome Android viewport behavior](https://developer.chrome.com/blog/viewport-resize-behavior/)).
2. Observe `window.visualViewport.resize` and `scroll` to refit the active xterm and keep the
   focused textarea and command controls in the visible area. The visual viewport can shrink while
   the layout viewport remains unchanged
   ([VisualViewport](https://developer.mozilla.org/en-US/docs/Web/API/VisualViewport)).
3. Feature-detect `navigator.virtualKeyboard`. If overlay mode is selected, consume
   `geometrychange` and the `keyboard-inset-*` CSS environment values; otherwise retain the
   viewport-resize path
   ([VirtualKeyboard API](https://developer.chrome.com/docs/web-platform/virtual-keyboard)).
4. Add one narrow-screen rule that collapses or overlays the activity rail, stacks the terminal
   sidebar at the bottom, and makes one dock panel visible at a time. Preserve the existing
   persisted `placement`; store an explicit user override separately from automatic placement.
5. Measure every custom control in rendered tests against 24 by 24 CSS px plus the WCAG spacing
   exceptions. Use the 44 by 44 enhanced size for primary terminal, reconnect, interrupt, upload,
   and navigation controls where density permits.
6. Test xterm's hidden textarea with composition, Gboard text, voice input, emoji, backspace,
   Enter, long press, paste, selection, and hardware keyboards. The current keydown handler should
   bypass composition events and must not translate Android input into duplicate PTY bytes.
7. Add a visible browser upload button backed by `<input type="file">`; stream selected `File`
   bytes to an authenticated route with a size limit, generated server-side name, temporary file,
   and atomic completion. Return a server path or typed upload ID to the existing paste flow.
8. Add a manifest and service worker for the static shell. Offline mode can show cached navigation,
   session names, and a reconnect state. Terminal writes and uncached data remain disabled until
   the authenticated host is reachable.

## 6. Acceptance matrix

| Case | Expected result | Verification class |
|---|---|---|
| Desktop localhost | Loopback HTTP loads; authenticated PTY WSS opens; input, resize, output sequence, detach, and reattach work. | Future automated integration |
| Direct LAN | Explicit-interface HTTPS only; application auth required; no wildcard listener assertion; same-origin WSS works. | Future isolated-host recipe |
| Tailnet through Serve | Full MagicDNS HTTPS name loads from an allowed Android identity; backend remains loopback; WSS Upgrade retains expected identity/auth behavior. | Future tailnet staging and physical device |
| Network interruption | UI enters reconnecting state, sends no queued terminal input, reconnects with last sequence, and receives replay or snapshot. tmux process survives. | Playwright socket fault plus physical Android |
| Expired Instant session | Upgrade or command receives 401; retries pause; terminal remains alive; authenticated reload creates a new session. | Automated integration |
| Expired tailnet device key | Host becomes unreachable at the network layer; UI remains offline; no application credential fallback. | Tailnet staging, administrative observation |
| OpenSSH unknown key | Interactive enrollment path displays fingerprint; batch operation fails as `HostKeyUnknown`. | Disposable local `sshd` |
| OpenSSH changed key | Connection fails as `HostKeyChanged`; known-host data is not deleted and checking is not disabled. | Disposable local `sshd` |
| Android offline launch | Cached shell shows offline state; terminal input/upload disabled; online event starts bounded reconnect. Current Instant has no service worker, so this currently fails. | Playwright offline plus physical PWA |
| Portrait | One panel visible, rail collapsed/overlaid, terminal sidebar bottom/closed, no horizontal page overflow, safe areas clear. | Pixel emulation plus physical Android |
| Landscape | Terminal remains usable beside or above selected secondary pane; rotation refits PTY once settled. | Pixel emulation plus physical Android |
| Virtual keyboard | Focused terminal row and primary controls remain visible; terminal is resized to visible rows; composition emits one byte sequence. | Physical Android required for final result |
| Touch and accessibility | Navigation, tabs, resize handles, interrupt, reconnect, and upload have accessible names, visible focus, and measured target size/spacing. | Playwright assertions plus manual screen reader/touch |
| Upload | Browser selection succeeds within limit; cancellation leaves no completed file; oversize and expired auth return typed errors. | Playwright file chooser and integration |
| Safe cancellation | Socket close and navigation detach only; Interrupt stops a turn; Kill requires its distinct action and ends tmux. | Automated integration |
| Desktop and Android same session | Configured single-writer behavior is visible, or multi-client size policy is deterministic. Current `-d`/`-D` detaches the other client. | tmux integration plus physical Android |

## 7. Runnable recipes

### 7.1 Checks executed in this research

The following read-only or disposable checks ran successfully:

```sh
ssh -G -F /dev/null \
  -o BatchMode=yes \
  -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile=/tmp/boop-ssh-probe-known-hosts \
  direct.invalid

ssh -G -F /dev/null \
  -o BatchMode=yes \
  -o StrictHostKeyChecking=yes \
  -o UserKnownHostsFile=/tmp/boop-ssh-probe-known-hosts \
  -J jump.invalid target.invalid
```

Selected expansion showed port 22, batch mode, strict checking, the isolated known-hosts path,
and `proxyjump jump.invalid` for the second command. No network connection occurred.

A disposable tmux server was created with `tmux -L <unique-socket> new-session -d -s probe`, then
verified by `has-session`, `list-sessions`, and `capture-pane`. It reported one detached session,
one window, and the expected marker. `kill-server` removed it. No repository build or test ran.

### 7.2 Disposable OpenSSH and tmux recipe

Run this only in a temporary test fixture that generates its own client key, host key,
`authorized_keys`, `known_hosts`, high port, and `sshd_config`. The fixture must set its generated
host public key directly in `[127.0.0.1]:<port>` known-hosts form, use
`StrictHostKeyChecking=yes`, and clean up its process and directory with a trap. `/usr/sbin/sshd`
and `ssh-keyscan` are locally available.

Test sequence:

```sh
ssh -F ./client_config target \
  tmux -L boop-e2e new-session -d -s android-probe
ssh -F ./client_config target \
  tmux -L boop-e2e has-session -t '=android-probe'
ssh -tt -F ./client_config target \
  tmux -L boop-e2e attach-session -t '=android-probe'
ssh -F ./client_config target \
  tmux -L boop-e2e capture-pane -p -t '=android-probe:0.0'
```

Then replace the generated host key and restart only the disposable `sshd`; assert exit 255 and a
changed-host-key classification without modifying known-hosts. A ProxyJump fixture runs two
disposable daemons and gives the destination and jump host separate config stanzas and known-host
entries. This is a recipe; it was not executed in this research.

### 7.3 Browser device-emulation recipe

After the loopback application server and browser transport exist, add a Playwright project based
on a shipped Pixel device descriptor. Playwright device emulation sets user agent, screen,
viewport, and touch; `context.setOffline(true)` covers network loss
([Playwright emulation](https://playwright.dev/docs/emulation)).

```ts
test.use({ ...devices["Pixel 7"] });

test("portrait, landscape, reconnect, and upload", async ({ page, context }) => {
  await page.goto(process.env.INSTANT_E2E_URL!);
  await expect(page.getByRole("application", { name: "Instant" })).toBeVisible();
  await page.getByRole("textbox", { name: "Terminal input" }).fill("printf probe");
  await page.setViewportSize({ width: 915, height: 412 });
  await context.setOffline(true);
  await expect(page.getByRole("status")).toHaveText(/offline|reconnecting/i);
  await context.setOffline(false);
  await expect(page.getByRole("status")).toHaveText(/connected/i);
  await page.getByLabel("Upload file").setInputFiles("fixtures/small.txt");
});
```

Run against a disposable local host with `pnpm exec playwright test <spec>`. Add a test-only
server switch that expires the cookie, drops the WebSocket after a known sequence, rejects a
subject, and reports active tmux/PTY ownership. This recipe was not run because those interfaces
do not exist.

Device emulation does not reproduce Gboard composition, Android virtual-keyboard geometry,
Chrome process suspension, PWA installation/WebAPK behavior, rotation timing, physical safe
areas, biometric reauthentication, or real tailnet transitions. Final acceptance for those rows
requires a physical Android device. `adb` was unavailable during this research.

## 8. Bounded implementation sequence

1. In Instant, implement the already-planned authenticated loopback HTTP lifecycle and migrate one
   read-only route. Keep the activity listener separate.
2. Add the typed PTY WebSocket frames, server-side session expiry, exact-Origin checks, sequence
   replay/snapshot behavior, and single-writer lease. Test reconnect and detach without network or
   Tailscale.
3. Add the narrow layout rule, viewport and safe-area policy, xterm refit hooks, browser upload
   route, manifest, and offline shell. Run Pixel portrait, landscape, touch, offline, auth-expiry,
   upload, and accessibility assertions.
4. Run a private staging check through Tailscale Serve to validate HTTPS, identity headers, WSS
   Upgrade, reconnect, and Android suspension. This step requires separately authorized tailnet
   configuration and a physical device.
5. Add `boop-ssh` as a small system-OpenSSH command/PTY boundary with the signatures in section
   4.2. Verify direct, ProxyJump, unknown-key, changed-key, disconnect, reconnect, interrupt, and
   tmux persistence against disposable local `sshd` fixtures.
6. Before remote Boop lane ownership, replace the `(socket, target)` identity with an explicit host
   endpoint and preserve distinct unreachable, missing-target, authentication, and host-key
   failures through `Multiplexer` call sites.
