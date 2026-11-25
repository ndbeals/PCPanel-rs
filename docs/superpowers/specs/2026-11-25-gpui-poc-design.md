# GPUI proof of concept — design

**Date:** 2026-11-25 · **Size:** small change (short design) · **Builds on:** `worktree-repo-tooling`

## Intent

Prove that gpui-kit is a workable GUI foundation for PCPanel-rs before building real UI on it.
It must show three things on Windows: it builds and runs in `pcpanel-gui`, it renders text and
working buttons, and it can draw a volume bar **over everything on the desktop** (like the Windows
volume popup), which the real app will show when a knob changes the volume.

## Goals / non-goals

- **Goals:** `mise run run:gui` opens a main window with text, a working button and a volume
  slider. Moving the slider shows an always-on-top volume overlay above all other apps. The overlay
  doesn't steal focus, lets clicks pass through to the app underneath, and disappears about 1.5s
  after the last change.
- **Non-goals:** Linux and macOS (not even compiling; the user will handle them much later),
  daemon/IPC wiring, real audio or HID, overlay styling beyond legible, fade animation, automated
  UI tests.
- **Anti-goals:** an overlay that only draws inside PCPanel's own window (gpui-kit's built-in
  overlay layers); an overlay that takes keyboard focus away from the app the user is in.
- **Known limit (accepted):** no normal app can draw over a game in *exclusive* fullscreen;
  borderless/windowed fullscreen is fine.

## Constraints

- gpui-kit `0.11.0` from crates.io; it bundles GPUI (`gpui-pre =0.3.14`). Don't add or override
  GPUI crates directly. Follow https://gpui-kit.com/docs/getting-started/ for setup
  (`application().with_assets(assets::Assets)`, `init(cx)` before any window, `open_window`).
- Prerequisites: Rust ≥ 1.92 (repo pins 1.100), MSVC toolchain, CMake on PATH, VS C++ build tools.
- Repo rule: every tool is pinned in the project `mise.toml`, so add CMake there (`cmake = "4.5.1"`,
  the version already installed globally).

## Approaches (overlay lifetime)

GPUI 0.3.14 has no per-window hide/show, so:

- **A. Open on demand, close after fade** — first slider change opens the overlay window, further
  changes update it, it's removed ~1.5s after the last change. Plain GPUI APIs only; nothing on
  screen while idle. Risk: window-creation cost may show as a delay or one-frame flash.
- **B. One persistent invisible overlay** — created at startup, transparent + click-through,
  draws nothing when idle. Instant, but a permanent invisible topmost window, and click-through
  becomes mandatory.
- (Dropped: gpui-kit in-window overlay layers — can't draw over other apps.)

**Chosen: A** (user decision). If A visibly lags or flashes, B is the known fallback; record that
finding rather than switching silently.

## Design

What GPUI provides (verified in `gpui-pre`/`gpui-pre-windows` 0.3.14 source):
`WindowKind::PopUp` → `WS_EX_TOOLWINDOW | WS_EX_TOPMOST`, borderless (topmost, no taskbar entry);
`WindowOptions { focus: false, .. }` → shown with `SW_SHOWNOACTIVATE`;
`WindowBackgroundAppearance::Transparent` is supported; the window exposes a `raw-window-handle`.

- **Dependencies:** add `gpui-kit = "0.11.0"` to `[workspace.dependencies]` and use it from
  `pcpanel-gui`. Click-through needs Win32 calls: use the `windows` crate at the major GPUI already
  uses (`0.62`) plus `raw-window-handle`, as Windows-only target dependencies.
- **Dev profile:** add the getting-started guide's `opt-level = 3` overrides for the GPUI crates
  (`gpui-pre`, `gpui-pre-platform`, `gpui-component`, `gpui-kit`, `gpui-kit-assets`, `rustybuzz`,
  `taffy`, `ttf-parser`) on top of the existing `"*"` = 2.
- **Main window** (normal window, titled "PCPanel"): a heading label; a button with a click
  counter label (visible proof clicks work, not just a `println!`); a volume slider 0–100; and a
  "Show overlay in 3s" button. The delayed button simulates a knob turn while the user is in
  another app, which is how focus-stealing and "over everything" actually get tested.
- **Overlay window** (`overlay.rs`): `PopUp`, `focus: false`, transparent background, not movable
  or resizable, about 320×64 px, centred horizontally near the bottom of the primary display. It
  shows a rounded dark panel with a filled bar and the percentage. One overlay at most: if it's
  open, update its level; if not, open it. Each change resets a ~1.5s timer; when it expires,
  remove the window.
- **Click-through:** after opening, get the HWND and add `WS_EX_LAYERED | WS_EX_TRANSPARENT` to
  the extended style.
- **Stub cleanup:** keep the `pcpanel_core`/`pcpanel_ipc` `use … as _` lines; nothing uses them yet.

## Verification (manual — this is a visual proof)

1. `mise install` brings in CMake; `mise run build` succeeds.
2. `mise run run:gui`: the main window shows the label, button and slider; clicking increments the
   counter.
3. Dragging the slider shows the overlay over other apps' windows; it tracks the value and
   disappears ~1.5s after release.
4. Click "Show overlay in 3s", then switch to Notepad and type: the overlay appears over Notepad,
   typing continues uninterrupted, and clicking through the overlay reaches Notepad.
5. Note any delay or flash when the overlay first appears (the A-vs-B question).

## Left to the builder

- Exact gpui-kit component APIs (slider, label, button); if there's no slider component, use −/+
  buttons that step by 5.
- **Risk to check first:** GPUI renders through DirectComposition (`WS_EX_NOREDIRECTIONBITMAP`).
  Adding `WS_EX_LAYERED` may break rendering. If it does, try `WS_EX_TRANSPARENT` alone, then
  report back. Don't drop click-through silently.
- Timer mechanism (e.g. a generation counter with a spawned delayed task), overlay colours and
  sizes, and module layout within `pcpanel-gui`.
