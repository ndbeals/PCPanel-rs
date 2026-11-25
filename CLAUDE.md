# CLAUDE.md

Guidance for Claude Code (and humans) working in this repo. This file is the shared source of
truth: update it when conventions change or emerge, and prefer it over private memory.

## What this is

A Rust reimplementation of [PCPanel](https://github.com/nvdweem/PCPanel) (local checkout:
`../PCPanel`), the controller software for PCPanel USB audio-control devices. The original is a
source of ideas, not a spec to match.

## Architecture

A low-memory headless **daemon** owns the device and audio control; **GUI** and **CLI** clients
talk to it over IPC.

| crate | kind | role | depends on |
|---|---|---|---|
| `pcpanel-core` | lib | shared domain types (devices, controls, profiles) | — |
| `pcpanel-hid` | lib | PCPanel USB HID protocol | core |
| `pcpanel-audio` | lib | one OS-audio API, per-OS backends inside | core |
| `pcpanel-ipc` | lib | daemon↔client contract | core |
| `pcpanel-daemon` | bin `pcpaneld` | the daemon | core, hid, audio, ipc |
| `pcpanel-cli` | bin `pcpanel` | CLI client | core, ipc |
| `pcpanel-gui` | bin `pcpanel-gui` | GUI client | core, ipc |

All crates are stubs for now. Stub crates mark their declared dependencies with
`use <crate> as _;` to silence cargo's unused-dependency warning; delete those lines once real
code uses the crate. Split new crates out when a real boundary appears.

Decided so far: GUI = [gpui-kit](https://gpui-kit.com/docs/getting-started/) with its `gpui-fast`
backend (don't add or override GPUI crates directly). IPC transport: not designed yet.

## Toolchain

- **Rust** is pinned in `rust-toolchain.toml` (the single source of truth). `mise.toml` sets
  `idiomatic_version_file_enable_tools = ["rust"]` so mise reads it; never add `rust` to
  `mise.toml` `[tools]`. Check with `mise exec -- rustup show active-toolchain`.
- **Every other tool** the project uses is pinned in the project `mise.toml`, even if it's
  installed globally. A fresh clone plus `mise install` must be enough.
- **Building the GUI** needs the Visual Studio C++ build tools (MSVC + a Windows SDK); CMake
  comes from `mise.toml`.
- Workspace-wide settings (edition 2024, resolver 3, shared deps, lints) live in the root
  `Cargo.toml`; crates inherit them with `*.workspace = true`.

## Tasks

| `mise run …` | does |
|---|---|
| `build` / `check` | build / type-check the workspace |
| `test` | nextest (passes with zero tests) |
| `fmt` / `lint` | rustfmt / clippy |
| `dev` | bacon watch loop |
| `run:daemon` / `run:cli` / `run:gui` | run a binary; args after `--` go to it |

## Cross-platform

Windows-first (developed and used there). Linux and macOS are eventual targets, but we don't spend
time on them, not even keeping them compiling, until the Windows beta works. Keep the habits below
so porting stays cheap later.

- Platform-facing code (IPC, GUI, HID, audio) uses libraries that abstract across platforms.
- OS-specific code sits behind `cfg(target_os = …)` inside the crate that needs it; each crate
  exposes the same API everywhere.
- No OS- or shell-specific committed config; task bodies are plain `cargo` commands.
  Machine-specific tweaks (linkers, env) go in uncommitted local config.

## Priorities

Fast local iteration until there are usable builds. Quality gates (CI, cargo-deny, hooks, strict
lints) come later, driven by real usage. Lints warn; warnings never fail a build.

## Docs and worktrees

Design docs and plans live in `docs/superpowers/{specs,plans}/`. When brainstorming in the
foreground, write them in the main checkout so they're visible in the editor; a worktree starts
when implementation starts. Background jobs work in a worktree: push the branch early and share
GitHub links to the docs.
