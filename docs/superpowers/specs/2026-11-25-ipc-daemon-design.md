# Daemon IPC and lifecycle: design

**Date:** 2026-11-25 · **Size:** project (full design)

## Intent

`pcpaneld` is the always-running, low-memory, headless process that owns the panels (HID) and OS
audio. Everything else talks to it: our GUI, our CLI, a future tray icon, and **third-party scripts
and plugins**. This design covers two things:

1. **The IPC contract and transport:** how clients connect, call methods, and receive events, and
   how outsiders learn the API well enough to generate their own client types.
2. **How the daemon runs:** starting at login, starting on demand, guaranteeing exactly one
   instance, stopping and restarting, and the tray process that watches it.

The user wants low-latency, efficient communication. The research below found that at our message
sizes (bytes to a few KB) and rates (up to a few hundred per second), latency comes from the OS hop,
not the encoding. A Windows named-pipe round trip is about 28 µs. One frame at 144 Hz is about 7 ms.
Zero-copy formats and shared memory save nanoseconds here, and the user agreed they're not worth the
complexity.

Prior art: the original PCPanel (`../PCPanel`) uses REST for commands and one JSON WebSocket
(`/ws/events`, about 14 event types) for live updates. It sends each new client a snapshot of current
state before live events, and broadcasts through a queue so a slow client can't stall HID input.
Its backend owns the tray icon. We keep the snapshot and slow-client ideas and change the rest.

## Goals, non-goals, anti-goals

**Goals**
- One local RPC endpoint, per user, that the GUI, CLI, tray and third parties all use the same way.
- Requests with replies, plus a pushed event stream, in plain JSON that any language can speak with
  a socket and a JSON library.
- A machine-readable API description (OpenRPC + JSON Schema), generated from the Rust code and
  committed, so third parties can generate types and API changes show up in review.
- Exactly one daemon per user, started at login and on demand, robust to crashes (no stale locks).
- The CLI can start, stop, restart and query the daemon. The GUI and tray can too, through the same
  shared client code.

**Non-goals (this pass)**
- The tray program itself. Its role is designed here (see "Tray") but it gets its own design and
  build later.
- Wiring the GUI to the daemon. The client library supports it; the GUI work comes later.
- A localhost WebSocket listener. Planned later as a second listener serving the same API with an
  access token (for web UIs, Stream Deck plugins). Nothing here may make it harder.
- A binary encoding. If measurements ever show JSON matters, add a negotiated binary encoding then.
- The real feature API (audio sessions, volume, profiles, lighting config). Methods and events get
  added as those features land; this pass ships a starter set (below).
- Linux and macOS implementation. Per `CLAUDE.md` they aren't built or kept compiling until the
  Windows beta works. Their behaviour is described here so the Windows code doesn't paint us into a
  corner, and OS-specific code sits behind `cfg(target_os = …)`.
- Network access or any authentication beyond "same OS user".

**Anti-goals** (failures even if it works)
- A slow, stuck or malicious client delaying HID input handling, audio control, or other clients.
- Two daemons running for the same user, or a crash leaving a lock or socket that blocks the next
  start.
- Another OS user (or a remote machine) being able to connect.
- A console window flashing at login or whenever a client starts the daemon.
- Third parties needing Rust or our crates to talk to the daemon.
- The published schema drifting from what the daemon actually sends.

## Constraints

- Windows-first (see above). tokio is fine in every crate (user decision; the daemon already runs on
  tokio per the HID design).
- The daemon must run as the logged-in user, not as a Windows service or system daemon: per-app
  volume, default devices and audio sessions live in the user's login session on every OS.
- Library versions as researched on 2026-11-25: `jsonrpsee` 0.26.2, `interprocess` 2.4.9 (feature
  `tokio`), `schemars` 1.2.3, `usage-lib` 7.4.1. Use current versions at build time.
- The daemon's command-line flags are parsed with usage-rs (`usage-lib`), the same way as the CLI
  (the CLI proof of concept is being built in another session; match its pattern).
- Repo is GPL-3.0-only. Code adapted from reth (MIT/Apache-2.0) keeps its notice.

## Approaches

### Protocol (user picked A)

- **A. JSON-RPC 2.0 over a local socket, using jsonrpsee.** The API is a Rust trait; jsonrpsee's
  `#[rpc]` macro generates a typed server and client. Subscriptions carry events. One JSON message
  per line over a named pipe / Unix socket. Cost: jsonrpsee has no raw-pipe transport, so we write
  roughly 200–400 lines of glue around its public dispatch API, following reth's IPC server (which
  does this on `interprocess`). First-party clients use exactly what third parties use.
- **B. Own envelope, encoding negotiated per connection (postcard or JSON).** Binary fast path for
  our clients, but we'd write RPC plumbing (ids, errors, subscriptions, cancellation), keep types
  within what both encodings support, and test both.
- **C. Localhost HTTP + WebSocket, like the original.** Easiest for third parties, but TCP has no
  per-user access control (token file needed), heavier HTTP stack, port conflicts. A can grow a
  WebSocket listener later anyway.
- Dropped: gRPC/tonic (`protoc`, custom Windows pipe connector, awkward from scripts); tarpc (no
  server push); remoc (Rust-only wire format); servo's ipc-channel (each server accepts exactly one
  client); shared memory / iceoryx2 (no visible gain, pre-release, Windows tier 2); rkyv /
  rkyv_versioned / FlatBuffers / Cap'n Proto (zero-copy not needed; the latter two need external
  codegen tools); bincode (development ceased 2025).

### Schema direction (user picked Rust-first)

- **Rust → schema.** Rust types derive `schemars::JsonSchema`, which honours serde attributes, so
  the schema matches the real JSON. A generator writes the spec files; a test fails if they're stale.
- **Schema → Rust** (hand-written schema, Rust generated with typify). Rejected: typify reads only
  draft-07 and can panic on the `#/components/schemas/` refs OpenRPC uses; its output isn't
  idiomatic; the daemon would use generated types everywhere; the jsonrpsee trait is still written
  by hand. Worth it only when non-Rust parties co-own the contract.
- **openrpsee** (Zcash's OpenRPC generator for jsonrpsee) rejected: Zcash-specific conventions,
  pinned to jsonrpsee 0.24, no subscriptions, parameters described in doc comments.
- **OpenAPI** rejected: it describes HTTP paths and verbs, which we don't have.

### Describing events (user picked 1)

OpenRPC (1.4.1) has no standard way to describe server-pushed messages.
- **1. One event stream, documented in OpenRPC.** One `subscribe` method; every pushed message
  carries one `Event` tagged union, which is in the JSON Schema file so code generators produce it.
  The link "subscribe sends `Event`s" is one sentence plus an `x-` extension field.
- 2. AsyncAPI 3.1 (alongside or instead of OpenRPC): built for events, but no JSON-RPC binding, thin
  Rust tooling, and two specs to maintain. Can be generated later from the same types if it matures.
- 3. Own protocol fully described in AsyncAPI: reopens the protocol decision for marginally better
  docs.

### Tray placement (user picked 4)

1. Daemon owns the tray (like the original): fewest processes, but puts an OS UI loop in the daemon
   (on macOS it must own the main thread) and a tray bug can take down knob handling.
2. GUI owns it and starts hidden at login: GPUI's memory all day for an icon.
3. No tray.
4. **A tiny separate `pcpanel-tray` process** (no GPUI), an ordinary IPC client that can also watch
   and restart the daemon. Same pattern as Tailscale's Windows tray, KDE Connect's indicator, and
   Syncthing's tray wrappers.

## The design

### Crates

- **`pcpanel-ipc`**: the contract and both ends of the plumbing.
  - Wire types (requests, results, `Event`), separate from internal types so a daemon refactor
    can't silently break third parties. Conversions from `pcpanel-core` types live at the edge.
  - The jsonrpsee API trait(s).
  - Transport: endpoint naming, listener with access control, line framing, the server glue to
    jsonrpsee, and the client transport.
  - Client helpers shared by CLI, GUI and tray: connect, connect-or-start, protocol version check.
  - The schema generator and the spec-is-current test.
- **`pcpanel-daemon`** (`pcpaneld`): flags, paths, single-instance lock, logging, serving the API,
  graceful shutdown, and the starter methods. Wires HID events in once `pcpanel-hid` lands on
  `main` (it's on `worktree-hid-protocol-design`; whichever lands second does the hookup).
- **`pcpanel-cli`** (`pcpanel`): `daemon` subcommands (below), using the shared client helpers.
- Later: **`pcpanel-tray`** (new bin crate), GUI wiring.

### Transport and framing

- `interprocess` local sockets: a named pipe on Windows, a Unix domain socket on Linux/macOS.
- **Endpoint name, per user** (two people logged in at once each get their own daemon):
  - Windows: `\\.\pipe\pcpanel-<per-user id>`; the id must be unique per user and pipe-name safe
    (e.g. derived from the user's SID).
  - Linux: `$XDG_RUNTIME_DIR/pcpanel/pcpaneld.sock` (directory mode 0700, socket 0600).
  - macOS: under the per-user `$TMPDIR` (mind the ~104-byte socket path limit).
  - Overridable by `--socket <name>` on the daemon and `PCPANEL_SOCKET` for every client, so dev runs
    and tests can run isolated daemons.
- **Access control:** Windows pipe security descriptor allowing only the current user (plus SYSTEM),
  remote clients rejected. Unix: the 0700 directory and 0600 socket. If the pipe name already exists
  while we hold the lock, refuse to start and log it (possible squatting).
- **Framing:** one compact JSON-RPC message per line (`\n`-terminated; compact JSON never contains a
  raw newline). JSON-RPC batches allowed. Maximum message size enforced (1 MiB is a reasonable start);
  oversized input closes that connection.

### RPC

- JSON-RPC 2.0 via jsonrpsee. Method names are `namespace.method` (jsonrpsee's
  `namespace_separator = "."`), e.g. `daemon.info`.
- **Starter methods (this pass):**
  - `daemon.info` → daemon version, protocol version, pid, uptime.
  - `daemon.shutdown` → acknowledges, then shuts down gracefully.
  - `devices.list` → connected panels (serial, model, display name).
  - `subscribe(topics)` / `unsubscribe`, delivering `Event` notifications (below).
  - `rpc.discover` → the OpenRPC document, built at runtime from the same code as the committed file.
- **Errors:** application errors use stable JSON-RPC error codes from one Rust enum, documented in
  the OpenRPC document.
- **Protocol version:** semver, in `daemon.info` and the OpenRPC `info.version`. Breaking change =
  major bump. Clients compare majors on connect: the CLI fails with a hint to run
  `pcpanel daemon restart`; the GUI will offer a restart.

### Events

- One subscription: `subscribe` takes a list of topics (this pass: `devices`, `input`; later `audio`,
  `profile`, …) and returns a subscription id. Notifications are method `event`, payload `Event`.
- `Event` is one internally tagged union (`"type": "device_connected"`, `"device_disconnected"`,
  `"input"`, …). Each variant belongs to one topic.
- **Snapshot first:** right after subscribing, the client receives events describing current state
  for its topics (e.g. a `device_connected` per panel, current positions), then live events. A
  client never has to query and subscribe separately or worry about the gap between them.
- **Slow clients:** each subscription has a bounded buffer. If it fills, the daemon ends that
  subscription with a "lagged" error; the client resubscribes and gets a fresh snapshot. Producers
  never block on a client.

Example exchange (third-party view):

```
→ {"jsonrpc":"2.0","id":1,"method":"daemon.info"}
← {"jsonrpc":"2.0","id":1,"result":{"version":"0.1.0","protocol_version":"0.1.0","pid":4242,"uptime_secs":12}}
→ {"jsonrpc":"2.0","id":2,"method":"subscribe","params":{"topics":["devices","input"]}}
← {"jsonrpc":"2.0","id":2,"result":"s1"}
← {"jsonrpc":"2.0","method":"event","params":{"subscription":"s1","result":{"type":"device_connected","serial":"…","model":"pro"}}}
```

(Field names are illustrative; the generated schema is the authority.)

### Wire-type rules

So code generated in other languages comes out clean:
- Enums are internally tagged with `"type"`; no untagged enums; no `flatten`.
- `snake_case` field and variant names.
- Optional fields are explicit (`Option<T>`), documented in the schema.
- Doc comments on wire types and methods become schema descriptions.

### Published spec and how it stays current

- Files, committed: `schema/pcpanel.openrpc.json` (OpenRPC 1.4) and `schema/pcpanel.schema.json`
  (every wire type, JSON Schema draft-07, for quicktype and similar tools). Both come from one
  generator so they can't disagree.
- Draft-07, because OpenRPC specifies it and most tools handle it (`schemars`
  `SchemaSettings::draft07()`).
- The OpenRPC document marks `subscribe` with an `x-` extension naming the notification method
  (`event`) and pointing at the `Event` schema. Exact extension name is the builder's choice.
- **Workflow:** edit Rust → `mise run schema` rewrites the files → commit. A test in `mise run test`
  regenerates them in memory and fails ("run `mise run schema`") when the committed copies differ.
  The generator itself can be that test with an env var to write instead of compare; `mise run
  schema` sets the env var in the task definition (no shell-specific syntax).
- **Method list can't drift:** each method is declared once, feeding both the jsonrpsee trait and
  the OpenRPC metadata. Acceptable minimal version: a test that compares the server's registered
  method names (`RpcModule::method_names()`) with the OpenRPC document's methods.

### Daemon process

Startup, in order:
1. Parse flags (usage-rs): at least `--socket`, `--log-level`; others as needed.
2. Resolve per-user paths (data, logs, lock) with a cross-platform directories crate.
3. **Single-instance lock:** open `<data dir>/pcpaneld.lock` and `File::try_lock()` it (std, stable
   since Rust 1.89, same on every OS). If it's held, print/log "already running (pid N)" and exit
   with a distinct exit code. The OS releases the lock when the process dies, crash included, so
   there's no stale-lock cleanup. Write the PID into the file for diagnostics and `--force` stop.
4. Holding the lock, (Unix) remove any leftover socket file, then bind the listener.
5. Start subsystems (HID manager once available; audio later), then serve.

Shutdown: on `daemon.shutdown`, Ctrl+C / console close, logoff, or SIGTERM: stop accepting, end
subscriptions, stop subsystems (release devices, save state), exit 0 within a bounded timeout.

- **No console window:** on Windows the daemon must not open a console when started at login or by a
  client, yet `mise run run:daemon` from a terminal should still show logs. Suggested:
  `windows_subsystem = "windows"` plus attaching to the parent console when there is one.
- **Logging:** `tracing` to a rolling file in the per-user log directory, plus the terminal when
  attached to one.
- **Config ownership:** the daemon alone reads and writes profile/config files; clients change them
  through RPC.

### Starting, stopping, autostart

- **Shared client helper `connect_or_start()`** (in `pcpanel-ipc`): try to connect; if nothing is
  listening, launch `pcpaneld` from the client's own executable directory (fallback: `PATH`),
  detached, no console window, stdio to null; retry connecting with backoff for a few seconds; on
  failure, report with the log file location. If two clients start it at once, the lock picks a
  winner and the loser exits quietly; both clients then connect to the winner.
- **CLI commands:**
  - `pcpanel daemon start` → start if not running (no-op if it is).
  - `pcpanel daemon stop` → `daemon.shutdown`, then wait until the endpoint is gone.
    `--force` → kill the PID from the lock file, only while the lock is still held (proving that PID
    is still the daemon, not a reused PID).
  - `pcpanel daemon restart` → stop, then start (so after an upgrade, the new binary runs).
  - `pcpanel daemon status` → `daemon.info`, or "not running".
  - `pcpanel daemon autostart on|off|status` → manage start at login.
  - Other CLI commands use `connect_or_start()`.
- **Start at login (per user):**
  - Windows: a value under `HKCU\Software\Microsoft\Windows\CurrentVersion\Run` launching
    `pcpaneld`.
  - Linux (later): a systemd user unit with `Restart=on-failure` (so a clean stop stays stopped);
    XDG autostart `.desktop` as fallback.
  - macOS (later): a LaunchAgent with `RunAtLoad` and `KeepAlive` = `{SuccessfulExit: false}`.
  - The daemon is registered on its own, independent of the tray, because it must always run. When
    the tray exists it gets its own login entry too; the lock settles any start-up race.
- **Crash restart:** on Linux/macOS the supervisor restarts after a crash. On Windows, the tray (once
  built) restarts it; until then, the next client call does.

### Tray (designed here, built later)

- `pcpanel-tray`: a small separate process with a tray library and no GPUI, starting at login.
- An ordinary IPC client: shows daemon status, offers start / stop / restart.
- **Watches the daemon:** when its connection drops without a requested stop, it restarts the daemon
  via `connect_or_start()`, with crash-loop backoff (give up and show an error after repeated
  crashes in a short window).
- **Clicking the icon launches the GUI as a separate process** (user decision); if the GUI is already
  running, bring it forward instead of starting a second one.

## Verification

- Unit tests: framing (split/merged lines, oversize), lock behaviour (second `try_lock` fails; lock
  freed after the holder exits), wire-type JSON shapes.
- Integration test on Windows: start a daemon on an isolated `PCPANEL_SOCKET`, call `daemon.info`,
  subscribe and receive the snapshot, call `daemon.shutdown`, confirm the process exits and the
  endpoint is gone. Starting a second daemon while one runs exits with the "already running" code.
- Spec test: committed `schema/*.json` match the generator.
- Manual: `pcpanel daemon start/status/stop/restart`; kill `pcpaneld` from Task Manager and confirm
  `pcpanel daemon start` works immediately; a third-party smoke test from a short Python or
  PowerShell script that opens the pipe, sends `daemon.info`, and subscribes.

## Left to the builder

- Exact jsonrpsee integration (`raw_json_request` vs. reth-style internals) and how much of reth's IPC
  code to adapt.
- How the single method declaration is implemented (macro vs. cross-check test).
- The directories crate, the per-user pipe id derivation, exact exit codes, buffer sizes, timeouts.
- The `x-` extension name and shape for subscriptions.
- Which autostart crate, if any (e.g. `auto-launch`), versus direct registry writes.
- Exact `Event` and `devices.list` field sets, guided by `pcpanel-core` and the HID design.
