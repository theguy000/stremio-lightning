#!/bin/bash
set -euo pipefail

exec cargo xtask package-linux-appimage "$@"
