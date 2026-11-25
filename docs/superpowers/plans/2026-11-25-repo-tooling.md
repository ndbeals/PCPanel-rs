# Repo & Tooling Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A mise-managed, cross-platform Rust workspace of seven stub crates (daemon, CLI, GUI and four libraries) that builds, runs and tests with one-word mise tasks.

**Architecture:** A virtual Cargo workspace (`crates/*`). The Rust version lives in `rust-toolchain.toml`, which mise reads through a project-level setting. Every other tool is pinned in the project `mise.toml`. Tasks are plain `cargo` commands, so they run the same in any shell. No product logic.

**Tech Stack:** Rust 1.100.0 (edition 2024, resolver 3), mise 2026.11.x, cargo-nextest 0.9.152, bacon 3.26.0.

**Spec:** `docs/superpowers/specs/2026-11-25-repo-tooling-design.md`

## Global Constraints

- Rust `1.100.0`, set **only** in `rust-toolchain.toml`. No `rust` entry in `mise.toml` `[tools]`.
- `mise.toml` has `[settings] idiomatic_version_file_enable_tools = ["rust"]`, because the user's global `rust = "stable"` otherwise overrides the file.
- Every tool the project uses is pinned in the project `mise.toml`, even if it's installed globally.
- Edition `2024`, resolver `"3"`, `rust-version = "1.100"`.
- Nothing OS-specific or shell-specific in committed config or task bodies.
- Warnings never fail a build. No `-D warnings`, no CI, no hooks, no cargo-deny.
- Crate → binary names: `pcpanel-daemon` → `pcpaneld`, `pcpanel-cli` → `pcpanel`, `pcpanel-gui` → `pcpanel-gui`.
- No gpui-kit, CMake, IPC transport, HID or audio code in this pass.

## Review Focus

1. **Fresh clone on Linux/macOS:** `mise install` must fetch bacon. `github:Canop/bacon` ships one zip containing every platform, which was verified only on Windows. If it picks the wrong binary elsewhere, switch to `cargo:bacon`. (Task 1 notes this in `mise.toml`.)
2. **Global Rust override:** inside the repo, `mise exec -- rustup show active-toolchain` must say `1.100.0`, not `stable`. (Task 1, Step 3.)
3. **Zero tests:** `mise run test` must succeed while no crate has tests. nextest fails on "no tests" by default. (Task 3 passes `--no-tests=pass`.)
4. **Running tasks from a subdirectory:** `mise run build` from `crates/pcpanel-cli/` must build the whole workspace. (Task 3, Step 3.)
5. **Line endings:** after adding `.gitattributes`, files committed with CRLF get renormalized to LF, so Linux/macOS checkouts see clean files. (Task 1, Step 4.)

---

### Task 1: Toolchain pinning and repo hygiene

**Files:**
- Create: `rust-toolchain.toml`, `mise.toml`, `.gitattributes`

**Interfaces:**
- Produces: `mise install` gives Rust 1.100.0 (clippy, rustfmt), `cargo-nextest`, and `bacon` on PATH inside the repo.

- [ ] **Step 1: Write `rust-toolchain.toml`**

```toml
[toolchain]
channel = "1.100.0"
profile = "minimal"
components = ["clippy", "rustfmt"]
```

- [ ] **Step 2: Write `mise.toml`** with `min_version = "2026.11.12"`, `[settings] idiomatic_version_file_enable_tools = ["rust"]`, and `[tools]`:
  `"aqua:nextest-rs/nextest/cargo-nextest" = "0.9.152"` and `"github:Canop/bacon" = "3.26.0"`. Add a comment above bacon: it's not in the mise registry, the release zip holds every platform, it's verified on Windows only, and the fallback is `"cargo:bacon"`. Also add a comment saying that Rust comes from `rust-toolchain.toml`. Leave `[tasks]` for Task 3.

- [ ] **Step 3: Install and verify**

Run: `mise trust && mise install && mise exec -- rustup show active-toolchain && mise exec -- cargo nextest --version && mise exec -- bacon --version`
Expected: the active toolchain line starts with `1.100.0-x86_64-pc-windows-msvc` (not `stable`); `cargo-nextest 0.9.152`; `bacon 3.26.0`.

- [ ] **Step 4: Write `.gitattributes`** (`* text=auto eol=lf`), then run `git add --renormalize .` and check `git status` shows only the intended files.

- [ ] **Step 5: Commit** — `git add rust-toolchain.toml mise.toml .gitattributes` (plus any renormalized files) and `git commit -m "Pin toolchain via rust-toolchain.toml and project mise.toml"`

---

### Task 2: Workspace and stub crates

**Files:**
- Create: `Cargo.toml`
- Create: `crates/pcpanel-{core,hid,audio,ipc}/{Cargo.toml,src/lib.rs}`
- Create: `crates/pcpanel-{daemon,cli,gui}/{Cargo.toml,src/main.rs}`

**Interfaces:**
- Consumes: the Task 1 toolchain.
- Produces: packages `pcpanel-core`, `pcpanel-hid`, `pcpanel-audio`, `pcpanel-ipc`, `pcpanel-daemon`, `pcpanel-cli`, `pcpanel-gui`, and binaries `pcpaneld`, `pcpanel`, `pcpanel-gui`.

- [ ] **Step 1: Write the root `Cargo.toml`.** It's a virtual workspace with `members = ["crates/*"]` and `resolver = "3"`.
  - `[workspace.package]`: `version = "0.1.0"`, `edition = "2024"`, `rust-version = "1.100"`, `license = "GPL-3.0-only"`, `repository = "https://github.com/ndbeals/PCPanel-rs"`.
  - `[workspace.dependencies]`: a path entry for each of the four libraries, for example `pcpanel-core = { path = "crates/pcpanel-core" }`.
  - `[workspace.lints.clippy]`: `all = { level = "warn", priority = -1 }`.
  - `[profile.dev]`: `debug = "line-tables-only"`.
  - `[profile.dev.package."*"]`: `opt-level = 2` and `debug = false`. This applies to third-party dependencies only.

- [ ] **Step 2: Write the four library crates.**
  - Each `Cargo.toml` takes the shared package fields (`version.workspace = true`, and the same for edition, rust-version, license and repository) and sets `[lints] workspace = true`.
  - Each `lib.rs` is just a `//!` doc line giving the crate's role, taken from the spec's layout table.
  - `pcpanel-hid` and `pcpanel-audio` depend on `pcpanel-core`. `pcpanel-ipc` depends on `pcpanel-core`.

- [ ] **Step 3: Write the three binary crates.**
  - Each sets a `[[bin]] name` to its binary name (above) and uses the same shared fields and lints as the libraries.
  - Dependencies: the daemon uses core, hid, audio and ipc. The CLI and the GUI use core and ipc. Each uses the form `pcpanel-core.workspace = true`.
  - Each `main.rs` prints one line naming the binary and saying it's a stub.

- [ ] **Step 4: Verify the build and dependency graph.**

Run: `mise exec -- cargo build --workspace` → it finishes with no errors and no warnings.
Run: `mise exec -- cargo run -q -p pcpanel-daemon`, then the same for `-p pcpanel-cli` and `-p pcpanel-gui` → each prints its stub line.
Run: `mise exec -- cargo tree -p pcpanel-daemon --depth 1` → it lists `pcpanel-audio`, `pcpanel-core`, `pcpanel-hid` and `pcpanel-ipc`.

- [ ] **Step 5: Commit** — `git add Cargo.toml Cargo.lock crates` and `git commit -m "Add workspace with daemon, cli, gui and library stub crates"`. The lockfile gets committed because the workspace produces binaries.

---

### Task 3: mise tasks

**Files:**
- Modify: `mise.toml` (add `[tasks]`)

**Interfaces:**
- Consumes: the Task 2 package names.
- Produces: the `mise run` task names `build`, `check`, `test`, `fmt`, `lint`, `dev`, `run:daemon`, `run:cli` and `run:gui`. Task 4 documents them.

- [ ] **Step 1: Add the tasks.** Each has a `description` and a single `run` line:

| task | run |
|---|---|
| `build` | `cargo build --workspace` |
| `check` | `cargo check --workspace --all-targets` |
| `test` | `cargo nextest run --workspace --no-tests=pass` |
| `fmt` | `cargo fmt --all` |
| `lint` | `cargo clippy --workspace --all-targets` |
| `dev` | `bacon` |
| `run:daemon` / `run:cli` / `run:gui` | `cargo run -p pcpanel-daemon` / `-p pcpanel-cli` / `-p pcpanel-gui` |

- [ ] **Step 2: Verify each task.**

Run: `mise run build`, `mise run check`, `mise run test`, `mise run fmt` and `mise run lint` → each exits 0. `test` reports zero tests without failing.
Run: `mise run run:cli -- --help` → it prints the stub line. This confirms extra arguments pass through, even though the stub ignores them.
Run: `mise tasks` → it lists all nine tasks with descriptions.
Start `mise run dev` and stop it once bacon shows a passing check (this one is interactive, so check it by eye).

- [ ] **Step 3: Verify from a subdirectory.** Run `cd crates/pcpanel-cli && mise run build`. It should build the whole workspace and exit 0.

- [ ] **Step 4: Commit** — `git add mise.toml` and `git commit -m "Add mise tasks for build, test, lint, dev and run"`

---

### Task 4: Docs

**Files:**
- Create: `CLAUDE.md`
- Modify: `README.md`

**Interfaces:**
- Consumes: the Task 3 task names and the Task 2 crate table.

- [ ] **Step 1: Write `CLAUDE.md`, aiming for under 80 lines.** It covers:
  - what the project is (the Rust reimplementation of `../PCPanel`, which serves as a source of ideas, not a spec to match)
  - the architecture (daemon plus GUI/CLI clients) and the crate table with dependency edges
  - the toolchain rules (Rust is pinned in `rust-toolchain.toml`; every tool goes in the project `mise.toml`)
  - the mise tasks
  - the cross-platform rules (Windows-first, Linux and macOS kept working; platform code behind `cfg(target_os)` inside the crate; no OS-specific committed config)
  - the current priority (fast iteration; quality gates come later) and decisions made so far (GUI = gpui-kit; IPC still to be designed; no CI yet)
  - a line saying to update this file as conventions emerge

- [ ] **Step 2: Extend `README.md`** with a short getting-started section: install mise, run `mise install`, then `mise run build` / `mise run run:daemon`. Link to `CLAUDE.md` for the layout.

- [ ] **Step 3: Verify** that every task name and crate name in both files matches `mise tasks` and `crates/` exactly.

- [ ] **Step 4: Commit** — `git add CLAUDE.md README.md` and `git commit -m "Document layout, toolchain and tasks"`
