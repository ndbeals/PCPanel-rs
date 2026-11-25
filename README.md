# PCPanel-rs
A reimplementation of github.com/nvdweem/PCPanel in rust

## Getting started

Prerequisites: [mise](https://mise.jdx.dev). On Windows, also the Visual Studio Build Tools with
the "Desktop development with C++" workload (MSVC x64 libraries and a Windows SDK).

```sh
mise install          # Rust (from rust-toolchain.toml) and every project tool
mise run build        # build the workspace
mise run run:daemon   # run the daemon (pcpaneld)
mise tasks            # list all tasks
```

See [CLAUDE.md](CLAUDE.md) for the crate layout, toolchain rules and conventions.
