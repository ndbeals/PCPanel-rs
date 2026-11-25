# Windows audio control (`pcpanel-audio`) and a first CLI: design

**Date:** 2026-11-25 · **Size:** project (full design)

## Intent

Give PCPanel-rs real OS audio control, starting with Windows, by porting what the original PCPanel's
`SndCtrl` (`../PCPanel`: `src/main/cpp/SndCtrl/*` plus the Java in
`src/main/java/com/getpcpanel/integration/volume/platform/`) can do. We port **its capabilities and the
Windows behaviour it learned the hard way**, not its structure: the C++ DLL, JNI glue and Java event bus
all go away. Rust calls the Windows Core Audio APIs directly through the `windows` crate.

The first visible result is the `pcpanel` CLI listing audio devices and changing the default device.
Everything else in the crate (app volume, mute, per-app routing, change events) is what the daemon will
use to turn knob input into audio changes once the USB work (another session, branch
`worktree-hid-protocol-design`) lands.

## Goals, non-goals, anti-goals

**Goals**
- `pcpanel-audio` exposes one async API on every OS, with a complete Windows backend covering everything
  SndCtrl does for audio:
  - list output and input devices (name, hardware name, volume, mute) and app sessions per device;
  - set device volume and mute; set app volume and mute, by exe name or pid, on all outputs or one;
  - read and set default devices per direction and role;
  - per-app routing: send an app to a specific output or input, or back to the default;
  - describe an endpoint id, including one that is no longer present;
  - a stream of change events (devices, defaults, sessions, volumes, mutes).
- Shared audio types live in `pcpanel-core`, so the CLI now and IPC later speak the same types.
- `pcpanel devices` lists devices and marks the defaults. `pcpanel set-default <device>` changes one.
- Linux and macOS compile and run, returning a clear "not supported on this OS yet" error.

**Non-goals (this pass)**
- **Focus detection.** Working out which app has focus needs the windowing system, so that's the
  daemon's job. `pcpanel-audio` is told which app (exe name or pid) to change and has no focus API.
  (SndCtrl's `FocusListener` and `SetFocusVolume` don't get ported.)
- Level meters, loopback capture, and the playback gate. They only matter once there are LEDs to drive.
  When they arrive, the OS-reading half (peak meters, capture streams) belongs in `pcpanel-audio`,
  perhaps with capture behind a cargo feature or in its own crate, since it's heavy. The deciding half
  (the playback gate's "what counts as playing", turning levels into colours) belongs in the daemon.
- Linux (PulseAudio/PipeWire) and macOS (CoreAudio) backends. The API has to fit them (see Roles), but
  only stubs get built.
- IPC, the daemon, and USB.
- Endpoint recovery: rewriting saved device ids after Windows replaces an endpoint
  (`EndpointRecovery`, `EndpointReplacements`). That's daemon/config logic. This crate only supplies the
  facts it needs: hardware name, and `describe_endpoint` for missing ids.
- New-session volume restore and "force volume" (`NewSessionVolumeService`). These are daemon policy
  built on session events.
- Listing every running process (`getRunningApplications`). The GUI's app picker needs it later. It
  isn't audio, so it doesn't belong here.
- Completions and man pages for the CLI (the `usage` CLI tool generates them; add it to `mise.toml`
  when wanted).

**Anti-goals** (failures even if it "works")
- **Blocking or heavy work inside a Windows notification callback.** Windows documents these callbacks
  as no place to block. PCPanel hit real deadlocks from doing so.
- **Releasing a COM object or unregistering a sink from inside a callback.** Releasing an
  `IAudioEndpointVolume` waits for in-flight callbacks to finish, so it waits on itself and never
  returns. Unregistering an `IAudioSessionEvents` sink from its own `OnStateChanged` deadlocks WASAPI.
  PCPanel works around both with detached threads (`ReleaseOffCallbackThread`,
  `AudioDevice::SessionRemoved`).
- A global lock that every volume call waits behind. One slow endpoint stalls all knobs (PCPanel's
  `g_audioMutex`, and the AB-BA deadlock described in `SndCtrlWindows.callNativeForEach`).
- One bad device or session (a failed property read, a vanished endpoint) taking down the whole
  backend.
- An async call that hangs forever because the audio thread died. Calls must fail with an error
  instead.
- Windows types or `unsafe` leaking into the public API, or `unsafe` spread across the crate.

## Constraints

- Windows-first; Linux and macOS must still build and run (`CLAUDE.md`). Same public API everywhere,
  OS code behind `cfg(target_os = …)` inside the crate.
- tokio is the project's async runtime (`pcpanel-hid` uses it). `pcpanel-audio` depends only on
  `tokio`'s `sync` feature (no runtime needed); the CLI starts a current-thread runtime.
- Two of the Windows APIs we need are **undocumented** and absent from the `windows` crate, so we
  declare them ourselves:
  - `IPolicyConfig` (set default endpoint). IID `f8679f50-850a-41cf-9c72-430f290290c8`, CLSID
    `CPolicyConfigClient` `870af99c-171d-4f9e-af0d-e63df40c2bc9`. Vtable from PCPanel's
    `policyconfig.h`. It also declares `IPolicyConfigVista` (`568b9108-…` / `294935CE-…`), which is
    what PCPanel actually calls. Either works on Windows 10/11.
  - `IAudioPolicyConfigFactory` (per-app routing). Activated with
    `RoGetActivationFactory("Windows.Media.Internal.AudioPolicyConfig", iid)`. IID
    `ab3d4648-e242-459f-b02f-541c70306324` on Windows 10 21H2 and later, falling back to
    `2a59116d-6c4f-45e0-a74f-707e3fef9258` on `E_NOINTERFACE`. Vtable from PCPanel's
    `audiopolicyconfigfactory.h` (after EarTrumpet's): `SetPersistedDefaultAudioEndpoint`,
    `GetPersistedDefaultAudioEndpoint` and `ClearAllPersistedApplicationDefaultEndpoints` follow 25
    placeholder slots, of which the first six are the `IUnknown`/`IInspectable` methods.
  - Both can disappear or change in a Windows update. Their absence must turn into a capability flag and
    an error, never a crash.
- Workspace dependencies go in the root `Cargo.toml` `[workspace.dependencies]`. The HID branch adds
  `tokio = "1.54"`, `thiserror = "2"`, `tracing = "0.1.44"` there. Use the same entries verbatim, so the
  two branches merge cleanly.

## Approaches

How the Windows backend tracks the system was the real choice:

- **A. Look everything up fresh on each call.** No state, no thread, smallest code. But the daemon sets
  app volume dozens of times a second while a knob turns, and enumerating every session on every call is
  slow. Change events would need polling.
- **B. One background audio thread that owns everything.** It keeps a live map of devices and sessions,
  updated by Windows' change notifications. The public API sends requests to it. Volume changes are
  fast and change events come for free. This is how SndCtrl works, minus its global lock. It's more code
  (notification handlers).
- **C. A now, B later.** Fastest to a working CLI, but the core gets rewritten when the daemon needs
  events.

**Picked: B.** The full feature set is in scope and the daemon needs events and fast repeated calls.
Keeping all COM objects on one thread also removes the class of locking bugs PCPanel fought.

Also decided:
- **The API is async rather than blocking.** The daemon runs tokio, and an async API means it never
  blocks a runtime thread waiting on Windows. tokio's `sync` channels need no runtime, so this costs the
  crate nothing.
- **The CLI calls `pcpanel-audio` directly for now**, not through the daemon. IPC isn't designed yet. This
  is a temporary exception to "clients only use IPC". `CLAUDE.md`'s crate table gets updated to say so,
  and a comment in the CLI's `Cargo.toml` says so too.

## Design

### Roles

Each OS has a different set of default devices. The API models a default as a **direction** (output or
input) plus a **role**:

| Role | Windows | macOS (later) | Linux (later) |
|---|---|---|---|
| `Media` | `eMultimedia` (+ `eConsole`, which follows it) | default output / input | default sink / source |
| `Communications` | `eCommunications` | — | — |
| `System` | — | default system (alert) output | — |

Windows' `eConsole` is hidden. On current Windows it moves with `eMultimedia`, and the Sound panel sets
the two together as "Default Device". When reading, Windows reports `Media` from `eMultimedia`. When
setting `Media`, call `SetDefaultEndpoint` for both `eConsole` and `eMultimedia`, as the Sound panel
does. (Designer's call: PCPanel sets only `eMultimedia` and relies on `eConsole` following; setting both costs
nothing.)

Each backend reports which (direction, role) pairs it supports. Setting an unsupported one returns
`AudioError::Unsupported`.

### Shared types (`pcpanel-core`, new module `audio`)

Plain data, `Clone + Debug + PartialEq`. Names are indicative:

- `DeviceId(String)`: the OS endpoint id (on Windows `{0.0.0.00000000}.{guid}`).
- `Direction { Output, Input }`, `Role { Media, Communications, System }`.
- `AudioDevice { id, name, hardware_name: Option<String>, direction, volume: f32, muted: bool }`. `name`
  is the friendly name ("Speakers (Schiit Modi 3+)"). `hardware_name` is
  `PKEY_DeviceInterface_FriendlyName`, which survives a rename. Volume is the 0.0–1.0 scalar that
  matches the Windows slider.
- `AppSession { device: DeviceId, pid: u32, exe_path: Option<PathBuf>, display_name: String, icon:
  Option<String>, volume: f32, muted: bool, system_sounds: bool }`. One per (device, pid). Windows can
  give one process several session handles on a device; they merge into one `AppSession`, as PCPanel's
  `WindowsAudioDevice.addSession` does. `display_name` is the first non-empty of: the session's display
  name, the exe's version-info `ProductName`, the exe file name. System sounds are always shown as
  "System Sounds".
- `Defaults`: (direction, role) → `Option<DeviceId>`.
- `MuteAction { Mute, Unmute, Toggle }`. Toggle is resolved against the current state, per target.
- `AppTarget { Name(String), Pid(u32), SystemSounds }`, plus the one matching rule, a pure function: a
  name matches a session case-insensitively, ignoring a trailing `.exe` on either side, against the exe
  file name, the full exe path, or the display name. This keeps PCPanel's `AudioSession.matches` rule,
  so "Deadlock" and `deadlock.exe` name the same app.
- `AppScope { AllOutputs, DefaultOutput, Device(DeviceId) }`: where an app volume, mute or route call
  applies. `AllOutputs` is every output device's sessions. (Designer's call: PCPanel's volume path uses outputs
  but its mute path uses every device; outputs for both is more consistent.)
- `resolve_device(devices, query, direction: Option<Direction>) -> Result<&AudioDevice, ResolveError>`,
  a pure function. It tries an exact id, then a case-insensitive exact name, then a case-insensitive
  substring of the name, taking the first stage that has any match. So "USB Audio" keeps working after
  Windows renumbers a device to "Speakers (2- USB Audio)", and "Headset" picks "Headset" over "Headset
  Earphone". A stage with several matches is `ResolveError::Ambiguous(candidates)`, and no match is
  `NotFound`. (Designer's call: PCPanel takes the first of several; for a CLI an error that lists them is safer.
  The daemon can decide differently later.)
- `AudioEvent`:
  - `DeviceAdded(AudioDevice)`, `DeviceRemoved(DeviceId)`, `DeviceChanged(AudioDevice)`;
  - `DefaultChanged { direction, role, device: Option<DeviceId> }`;
  - `SessionAdded(AppSession)`, `SessionRemoved { device, pid }`, `SessionChanged(AppSession)`.

  Volume and mute changes carry `ours: bool`, which is true when the change came from this process. On
  Windows: pass a fixed event-context GUID on every set call and compare it in callbacks. The daemon needs
  this to tell its own writes apart from the user's. PCPanel's force-volume logic struggles without it.

### `pcpanel-audio` public API

One concrete type, with the backend chosen at build time by `cfg`:

```rust
pub struct Audio { /* handle to the backend */ }

impl Audio {
    /// Starts the backend; resolves once the initial devices and sessions are read.
    pub async fn start() -> Result<Audio, AudioError>;
    pub fn capabilities(&self) -> &Capabilities; // supported (direction, role) pairs; per-app routing
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<AudioEvent>;

    pub async fn devices(&self) -> Result<Vec<AudioDevice>, AudioError>;
    pub async fn sessions(&self) -> Result<Vec<AppSession>, AudioError>;
    pub async fn defaults(&self) -> Result<Defaults, AudioError>;
    pub async fn describe_endpoint(&self, id: &DeviceId) -> Result<Option<EndpointInfo>, AudioError>;

    pub async fn set_device_volume(&self, id: &DeviceId, volume: f32) -> Result<(), AudioError>;
    pub async fn set_device_mute(&self, id: &DeviceId, action: MuteAction) -> Result<(), AudioError>;
    pub async fn set_default(&self, id: &DeviceId, role: Role) -> Result<(), AudioError>;

    /// Return how many sessions they changed; 0 is not an error.
    pub async fn set_app_volume(&self, apps: &[AppTarget], scope: &AppScope, volume: f32) -> Result<usize, AudioError>;
    pub async fn set_app_mute(&self, apps: &[AppTarget], scope: &AppScope, action: MuteAction) -> Result<usize, AudioError>;

    /// Per-app routing: `None` sends the apps back to the default device.
    pub async fn route_apps(&self, apps: &[AppTarget], direction: Direction, to: Option<&DeviceId>) -> Result<usize, AudioError>;
    pub async fn app_route(&self, pid: u32, direction: Direction) -> Result<Option<DeviceId>, AudioError>;
}
```

- `set_default`'s direction comes from the device. Volumes are clamped to 0.0–1.0.
- `AudioError` uses thiserror. Variants: `Unsupported` (OS or capability), `DeviceNotFound`,
  `Stopped` (the backend thread is gone), and `Os { context, code }` for a failed OS call.
- `EndpointInfo { name, hardware_name, direction, present: bool }`.
- The broadcast channel has a bounded capacity (designer's call: 256). A receiver that lags gets tokio's
  `Lagged` and should re-read with `devices()`/`sessions()`. The docs say so.
- `route_apps` resolves targets to pids from the current sessions (only processes that have a session),
  as PCPanel's `WindowsAppOutputRouter` does.

### Windows backend

**Thread and messages.**
- `Audio::start` spawns one OS thread named `pcpanel-audio`, which enters the multithreaded COM
  apartment (`CoInitializeEx(COINIT_MULTITHREADED)`) for its whole life and owns every COM object.
- It reads a `std::sync::mpsc` channel of one internal message type: either a request (with a
  `tokio::sync::oneshot` reply) or a notification from a callback.
- `std::sync::mpsc` is used for its `recv_timeout`, which drives the deferred work below. Sending on it
  never blocks, so async callers and COM callbacks can both use it.
- If the thread exits or panics, the senders drop, so pending and future calls get `Stopped` and nothing
  hangs. (Catch a panic per message and log it, so one bad message doesn't end the thread.)

**Callbacks only post.**
- Implement `IMMNotificationClient`, `IAudioEndpointVolumeCallback`, `IAudioSessionNotification` and
  `IAudioSessionEvents` with `windows`' `#[implement]`.
- Each holds a sender plus the key it reports for (device id, session handle key), copies the callback's
  arguments into a message, sends it and returns `S_OK`.
- All state changes, COM releases and sink unregistrations happen on the audio thread. A callback never
  blocks on it, so a release there waits on nothing. This removes the deadlocks in the anti-goals.

**State.**
- The thread keeps devices by id. Each device holds its `IMMDevice`, its `IAudioEndpointVolume` and
  registered volume callback, its `IAudioSessionManager2` and registered session notification, and its
  session handles grouped by pid. Each handle has its `IAudioSessionControl2`, `ISimpleAudioVolume` and
  registered `IAudioSessionEvents`.
- Keep the map-and-merge logic (handles → `AppSession`, notification → events) in plain Rust, separate
  from the COM calls, so it can be unit-tested without Windows.
- Start-up enumerates `DEVICE_STATE_ACTIVE` endpoints for both directions, then their sessions, then
  every default (direction × role), then reports ready.

**Notifications handled.**

| Callback | Effect |
|---|---|
| device added, or state changed to active | add the device and its sessions; `DeviceAdded` |
| device removed, or state changed to anything else | drop it (releasing on the audio thread); `DeviceRemoved` |
| default device changed | update `Defaults` (ignore `eConsole`); `DefaultChanged` |
| property changed (friendly name) | re-read the name; `DeviceChanged` (designer's call: PCPanel ignores renames) |
| endpoint volume `OnNotify` | `DeviceChanged` with the new volume and mute |
| session `OnSessionCreated` | add the handle; `SessionAdded` if it's the pid's first on that device |
| session `OnSimpleVolumeChanged`, `OnDisplayNameChanged`, `OnIconPathChanged` | `SessionChanged` |
| session `OnStateChanged(Expired)` | drop the handle; `SessionRemoved` when the pid has no handles left |
| session `OnSessionDisconnected` | see "Invalidated sessions" |

**Invalidated sessions.** This is the most important lesson from SndCtrl (`sndctrl.cpp`,
`AudioDevice::RebuildSessions`):
- When Windows renegotiates an endpoint's format, all of its session handles fail for good with
  `AUDCLNT_E_DEVICE_INVALIDATED`, while the sessions themselves live on. A Bluetooth headset does this
  about 150 ms after reconnecting, then re-announces sessions up to 1.6 s later.
- On `OnSessionDisconnected` with any reason other than `DisconnectReasonDeviceRemoval`, schedule one
  rebuild of that device's sessions 2 s later. Repeats while one is pending add nothing.
- If a set call gets `AUDCLNT_E_DEVICE_INVALIDATED`, rebuild that device's sessions right away and retry
  the call once.
- A rebuild re-enumerates the sessions and attaches the fresh handles before dropping the stale ones.
  It then diffs by pid, so an app that's still there produces no `SessionRemoved`/`SessionAdded` flicker.

**Operations.**
- Device volume and mute: `IAudioEndpointVolume::SetMasterVolumeLevelScalar` / `SetMute`.
- App volume and mute: `ISimpleAudioVolume::SetMasterVolume` / `SetMute` on every matching handle.
  Toggle resolves per session from its current mute.
- Default: `IPolicyConfig::SetDefaultEndpoint(id, role)`, created per call (as PCPanel does) or once.
- Per-app routing (`route_apps`, `app_route`): `SetPersistedDefaultAudioEndpoint(pid, flow, role,
  hstring)` for both `eConsole` and `eMultimedia`, and `GetPersistedDefaultAudioEndpoint(pid, flow,
  eMultimedia | eConsole)` to read.
  - The device id is wrapped as `\\?\SWD#MMDEVAPI#<id>#{e6327cad-dcec-4949-ae8a-991e976a79d2}` for
    output, or `…#{2eef81be-33fa-4800-9670-1cd474972c3f}` for input. A null `HSTRING` clears.
  - Unwrap on read.
  - If the factory couldn't be built, `capabilities().app_routing` is false and the calls return
    `Unsupported`.
- `describe_endpoint`: `IMMDeviceEnumerator::GetDevice` works for endpoints that are no longer present.
  Read the friendly name, the interface name and the direction (`IMMEndpoint::GetDataFlow`), plus
  `GetState` for `present`.
- Process facts for a session:
  - pid: `IAudioSessionControl2::GetProcessId`;
  - system sounds: `IsSystemSoundsSession`, which is `S_OK` for it;
  - exe path: `QueryFullProcessImageNameW` with `PROCESS_QUERY_LIMITED_INFORMATION`, which can open more
    processes than PCPanel's `PROCESS_QUERY_INFORMATION`;
  - `ProductName`: `GetFileVersionInfoW`/`VerQueryValueW`, bounding the read by the returned length.
    PCPanel notes the value comes from an arbitrary exe and carries no guaranteed terminator.
- Property strings: only accept `VT_LPWSTR` from `IPropertyStore::GetValue`, and clear the
  `PROPVARIANT` otherwise. PCPanel's `StringProperty` explains why: driver-supplied values of another
  type would be misread as pointers.

**Unsafe.** All COM and Win32 calls live in the Windows backend module tree. The rest of the crate,
including the state logic, is safe Rust.

### Linux and macOS

A stub backend. `Audio::start()` succeeds with empty capabilities, and every other call returns
`AudioError::Unsupported`. That way a daemon on those OSes can still run its USB side later. The CLI
prints "audio control isn't supported on this OS yet".

### `pcpanel` CLI

- Built on **usage-rs** (`usage-rs = "7"`): its `#[derive(Cli)]` parser, not clap.
- Depends on `pcpanel-core`, `pcpanel-audio` (temporary, as above) and `tokio` (`rt`, `macros`). Runs on a
  current-thread runtime.
- **`pcpanel devices`** prints outputs then inputs. Each row shows a default marker (`M` media,
  `C` communications, `S` system), name, volume %, muted, and id. For example:

  ```
  Outputs
    MC  Speakers (Schiit Modi 3+)        42%          {0.0.0.00000000}.{…}
        Headphones (2- USB Audio)        80%  muted   {0.0.0.00000000}.{…}
  Inputs
    MC  Microphone (Yeti Stereo)         65%          {0.0.1.00000000}.{…}
  ```

  Exact formatting is the builder's.
- **`pcpanel set-default <DEVICE> [--role media|communications|system] [--input | --output]`**:
  - The role defaults to `media`.
  - `<DEVICE>` goes through `resolve_device`; `--input`/`--output` narrow it.
  - An ambiguous or unknown device prints the candidates (or "no device matches") and exits non-zero.
  - On success it prints what it switched, e.g. "Default media output → Speakers (Schiit Modi 3+)".
  - An unsupported role exits non-zero with a message.
- Errors go to stderr with a non-zero exit code.

### Testing

- **Pure logic, unit-tested on every OS:** `resolve_device`, app matching, session merge and diff
  (including rebuild without flicker), the Windows state transitions driven by fake notifications, the
  role mapping, and MMDEVAPI id wrap/unwrap.
- **Windows-only integration tests,** `#[cfg(windows)]` and `#[ignore]` (run manually on a machine with
  audio): start, list devices, read defaults, subscribe. Tests that change system state (set default,
  volume, routing) must restore what they changed.
- `mise run test` must pass on all three OSes with the ignored tests skipped.

### Docs to update

`CLAUDE.md`: the crate table (CLI temporarily depends on audio), and a line that focus detection is the
daemon's. If the `windows` crate or others bring a convention worth recording, add it too.

## Left to the builder

- Module layout inside `pcpanel-audio` and `pcpanel-core::audio`; exact type and field names.
- The `windows` crate version and feature list (pin to the current release, under
  `[target.'cfg(windows)'.dependencies]`).
- Whether `IPolicyConfig` or `IPolicyConfigVista` is used, and whether it's created per call or once.
- Logging details (use `tracing`, as `pcpanel-hid` does), and whether the CLI installs a subscriber.
- CLI output formatting.
- Whether reads (`devices()`, …) go through the thread's request channel (the simple default) or a
  shared snapshot.
