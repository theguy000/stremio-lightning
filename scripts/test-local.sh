#!/usr/bin/env bash
# Runs the native client's tests on this machine, the same way CI runs them.
#
# Why this exists: the native crate links against libmpv, and this box has no `libmpv-dev`. A stub
# `libmpv.so` stands in at link time, and `LD_LIBRARY_PATH` finds it at run time. The window tests
# need a display, so everything runs under `Xvfb`. The two web shells
# (`stremio-lightning-{linux,windows,macos}`) need GTK/WebKit and are not built here; CI covers
# them, plus the clean-container build and the Windows and macOS targets.
#
# Usage:
#   scripts/test-local.sh                          # whole native crate, every test
#   scripts/test-local.sh --test native_mods_logs  # one test binary
#   scripts/test-local.sh check                    # `cargo check --tests` only, faster
#
# Run this BEFORE every push. CI is the last gate, not the way to find out.
set -euo pipefail
root=$(git rev-parse --show-toplevel)
cd "$root"

export PATH="$HOME/.cargo/bin:$PATH"
stub="$HOME/.cache/mpvstub"
if [ ! -f "$stub/libmpv.so" ]; then
    echo "missing $stub/libmpv.so: rebuild the mpv stub before running tests here" >&2
    exit 1
fi
export LD_LIBRARY_PATH="$stub${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
# Rust flags are part of the build fingerprint, so the link path must be in them or cargo rebuilds
# the world without it and the link fails.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-fuse-ld=mold -L $stub"

# `stremio-lightning-core` `include_str!`s the web shells' UI bundle, so a clean checkout cannot
# compile the workspace without it. The native client does not use it; build it only when absent.
if [ ! -f src/dist/mod-ui-svelte.iife.js ]; then
    if [ -d node_modules ]; then
        npx vite build --logLevel error
    else
        echo "src/dist/mod-ui-svelte.iife.js is missing: run 'npm ci && npm run build:ui' once" >&2
        exit 1
    fi
fi

if [ "${1:-}" = "check" ]; then
    shift
    exec cargo check --tests -p stremio-lightning "$@"
fi

# CI fails the run on a formatting diff before it builds anything, so this catches that in a
# second instead of after a seven minute round trip.
cargo fmt --all --check

exec xvfb-run -a cargo test -p stremio-lightning "$@"
