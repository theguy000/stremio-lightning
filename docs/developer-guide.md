# Developer Guide

This guide covers initial setup and the commands used for day-to-day Stremio
Lightning development. Platform setup, packaging, plugin APIs, and runtime
internals are documented separately.

## Prerequisites

Install:

- Node.js, preferably the current LTS release
- The stable Rust toolchain
- Native dependencies for your platform:
  [Linux](platforms/linux.md), [macOS](platforms/macos.md), or
  [Windows](platforms/windows.md)

## Initial Setup

Install frontend dependencies:

```bash
npm install
```

On Linux or Windows, download native runtime dependencies for the current host:

```bash
cargo xtask setup
```

Use an explicit setup command when preparing another supported platform:

```bash
cargo xtask setup-linux
cargo xtask setup-windows
```

There is no macOS setup xtask. Follow the [macOS guide](platforms/macos.md) for
its runtime requirements.

## Common Commands

Project workflows are exposed through `cargo xtask`:

```bash
cargo xtask help
```

| Task | Command |
| --- | --- |
| Run the Linux shell | `cargo linux` |
| Run the macOS shell | `cargo macos` |
| Run the Windows shell | `cargo windows` |
| Watch the UI bundle | `npm run dev:ui` |
| Test the UI | `cargo xtask test-ui` |
| Build the UI | `cargo xtask build-ui` |
| Run full validation | `cargo xtask validate` |
| Build release artifacts | See the [packaging guide](packaging.md) |

Use the platform shell aliases for local iteration. Use plain `cargo build` when
you only need a compile check, and `cargo xtask package-*` when you need a
distributable artifact under `dist/`.

## UI Development

Run frontend tests:

```bash
cargo xtask test-ui
```

Build the injected Svelte/Vite bundle:

```bash
cargo xtask build-ui
```

Use watch mode while editing UI source:

```bash
npm run dev:ui
```

The generated bundle is written to
`src/dist/mod-ui-svelte.iife.js`.

## Validation

Run the complete validation suite before submitting changes:

```bash
cargo xtask validate
```

This runs Rust formatting checks, Clippy across the workspace, Rust tests,
Vitest frontend tests, and a production UI build.

Run narrower checks while iterating when appropriate:

```bash
cargo test -p stremio-lightning-core
cargo test -p stremio-lightning-linux
cargo test -p stremio-lightning-macos
cargo test -p stremio-lightning-windows
cargo test -p xtask
cargo xtask test-ui
cargo xtask build-ui
```

Packaging changes also need artifact-specific validation from the
[packaging guide](packaging.md#package-validation).

## Further Reading

- [Linux development](platforms/linux.md)
- [macOS development](platforms/macos.md)
- [Windows development](platforms/windows.md)
- [Packaging](packaging.md)
- [Plugin API](plugin-api.md)
- [Runtime architecture](runtime-architecture.md)
