#!/bin/bash
set -euo pipefail

exec cargo xtask build-linux-appimage "$@"
