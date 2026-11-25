# Repo & tooling setup — design

**Date:** 2026-11-25 · **Size:** small change (short design)

## Intent

Set up the foundation for PCPanel-rs, a Rust reimplementation of PCPanel (`../PCPanel`), before any
product code exists. Priority is **fast local iteration** until there are usable builds; quality
gates come later, driven by real usage. Ideas come from the original repo, but matching it is not a goal.

## Goals / non-goals

- **Goals:** a multi-crate Cargo workspace that builds and runs on Windows, Linux and macOS; mise
  pins the toolchain and defines all dev tasks; fast debug rebuilds.
- **Non-goals (this pass):** CI, cargo-deny, pre-commit hooks, strict lint gates, IPC transport,
  GUI implementation, any device/audio code.
- **Anti-goals:** anything that slows the local loop; OS-specific config committed to the repo;
  shell-specific task scripts.

## Constraints

- Windows-first for development and use, but Linux and macOS support is maintained throughout.
  Platform-facing choices (IPC, GUI, HID, audio) must use libraries that abstract across platforms.
- Architecture: a low-memory headless **daemon**, with **GUI** and **CLI** clients talking to it.
- GUI will be **gpui-kit** (`gpui-kit` on crates.io, which pins its own GPUI snapshot `gpui-pre`;
  don't override GPUI crates). It requires Rust ≥ 1.92, the MSVC toolchain on Windows, CMake on
  PATH, and per-OS system packages (see https://gpui-kit.com/docs/installation). Not added in this pass.

## Approaches considered

- **A. Thin workspace, one app crate** — fastest start, split crates as real seams appear.
- **B. Pre-split by the original's domains** — clear layout, but guessed seams.
- **C. A plus quality gates (CI, deny, hooks)** — premature for the current priority.

**Chosen:** A, adjusted for seams that are already known: the daemon/client split, plus
core/hid/audio, which the user confirmed as good boundaries. CI was deferred explicitly.

## Design

```
Cargo.toml          virtual workspace: members = ["crates/*"], resolver 3, edition 2024,
                    [workspace.package], [workspace.dependencies], [workspace.lints]
rust-toolchain.toml [toolchain] channel = "1.100.0", profile = "minimal", components = clippy, rustfmt
mise.toml           [settings] idiomatic_version_file_enable_tools = ["rust"]  (mise reads Rust from
                    rust-toolchain.toml; no `rust` in [tools])
                    [tools] bacon, cargo-nextest; [tasks]
crates/
  pcpanel-core/     lib   shared domain types (devices, controls, profiles)
  pcpanel-hid/      lib   PCPanel USB HID protocol over a cross-platform HID library
  pcpanel-audio/    lib   one OS-audio API; per-OS backends behind cfg(target_os) inside
  pcpanel-ipc/      lib   daemon↔client contract; transport designed separately
  pcpanel-daemon/   bin `pcpaneld`     → core, hid, audio, ipc
  pcpanel-cli/      bin `pcpanel`      → core, ipc
  pcpanel-gui/      bin `pcpanel-gui`  → core, ipc  (gpui-kit added later)
```

- **Rust version source of truth is `rust-toolchain.toml`** (user decision). The project-level opt-in is
  required: mise sets `RUSTUP_TOOLCHAIN`, so a global `rust` entry (e.g. `rust = "stable"` in a parent
  `mise.toml`) would otherwise override the file. Verify with `mise exec -- rustup show active-toolchain`.
  rustup and IDEs also read the file directly, so non-mise users get the same toolchain.
- **Every tool the project uses is pinned in the project `mise.toml`** (user rule), even if it is also
  installed globally — a fresh clone plus `mise install` must be enough. Rust is the one exception in
  form only: it's pinned via `rust-toolchain.toml`, which mise reads. Versions: latest stable Rust
  (1.100.0 at time of writing) and Cargo resolver 3.
- **Stubs only:** libraries are empty, binaries print and exit; the dependency edges above are declared
  so the graph is real from day one.
- **mise tasks** (plain `cargo …` bodies, so they're shell-agnostic): `build`, `check`, `test` (nextest),
  `fmt`, `lint` (clippy), `dev` (bacon watch), `run:daemon`, `run:cli`, `run:gui`.
- **Dev-profile speed:** `debug = "line-tables-only"` for workspace crates; `opt-level` raised for
  dependencies only (keeps debug builds usable for hardware/audio/GUI work). All in `Cargo.toml`
  profiles, which are portable. Machine-specific linker tweaks stay in uncommitted local config.
- **Lints:** `[workspace.lints]` with rust/clippy defaults (warn), no `-D warnings` gate.
- A short `CLAUDE.md` describing layout and tasks, grown as conventions emerge.

## Left to the builder

Exact dependency opt-levels, the bacon config, the README task list, and the wording of the stub binaries.
