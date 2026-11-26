# Developer Guide

This guide covers initial setup and day-to-day commands for Stremio Lightning development.
Platform setup, packaging, mods/theming, and architecture internals are documented separately.

The native client lives in `crates/stremio-lightning`. Its four-layer architecture (`plumbing/`,
`screens/`, `app/`, `shell/`) and Slint UI layout are described in the
[runtime architecture](runtime-architecture.md).

## Prerequisites

Install:

- The stable Rust toolchain
- Native dependencies for your platform:
  [Linux](platforms/linux.md), [macOS](platforms/macos.md), or
  [Windows](platforms/windows.md)

## Initial Setup

On Linux or Windows, download native runtime dependencies (`stream-server`, `ffmpeg`, `ffprobe`,
and `libmpv` on Windows) for the current host:

```bash
cargo xtask setup
```

Use an explicit setup command when preparing another supported platform:

```bash
cargo xtask setup-linux
cargo xtask setup-windows
```

There is no macOS setup xtask. Follow the [macOS guide](platforms/macos.md) for its runtime
requirements.

## Common Commands

Project workflows are exposed through `cargo xtask`:

```bash
cargo xtask help
```

| Task | Command |
| --- | --- |
| Run the native client | `cargo run -p stremio-lightning` |
| Compile-check workspace and tests | `cargo check --workspace --all-targets` |
| Run full validation | `cargo xtask validate` |
| Bump version | `cargo xtask bump-version <version>` |
| Build release artifacts | See the [packaging guide](packaging.md) |

Use `cargo build` or `cargo check --workspace --all-targets` for fast compile checks, and
`cargo xtask package-*` when you need a distributable artifact under `dist/`.

## Validation

Run the complete validation suite before submitting changes:

```bash
cargo xtask validate
```

This runs:

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets`
3. `cargo test --workspace`

Run narrower checks while iterating when appropriate:

```bash
cargo test -p stremio-lightning
cargo test -p xtask
```

Packaging changes also need artifact-specific validation from the
[packaging guide](packaging.md#package-validation).

## Further Reading

- [Linux development](platforms/linux.md)
- [macOS development](platforms/macos.md)
- [Windows development](platforms/windows.md)
- [Packaging](packaging.md)
- [Mods & Plugin Contract](plugin-api.md)
- [Runtime architecture](runtime-architecture.md)
