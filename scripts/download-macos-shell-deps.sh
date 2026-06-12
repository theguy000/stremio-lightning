#!/usr/bin/env bash
set -euo pipefail

# Downloads the macOS streaming server shell dependencies for Stremio Lightning.
#
# Usage:
#   scripts/download-macos-shell-deps.sh [arm64|x86_64]
#
# Defaults to the host architecture when no argument is given.
#
# x86_64 (Intel): all four runtime artifacts come from the official upstream
#   Stremio service release asset (stremio-service-macos.zip), which ships
#   Intel-only binaries.
# arm64 (Apple Silicon): the upstream zip only ships x86_64 binaries, so the
#   server runtime is a pinned stock Node.js darwin-arm64 build renamed to
#   stremio-runtime-macos, ffmpeg/ffprobe come from the pinned jellyfin-ffmpeg
#   portable macarm64 build, and server.cjs comes from the same upstream
#   Stremio service zip (architecture-independent JavaScript).

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
MACOS_CRATE="${REPO_ROOT}/crates/stremio-lightning-macos"
BINARIES_DIR="${MACOS_CRATE}/binaries"
RESOURCES_DIR="${MACOS_CRATE}/resources"

SERVICE_VERSION="v0.1.21"
SERVICE_REPO="Stremio/stremio-service"
SERVICE_ASSET="stremio-service-macos.zip"

NODE_VERSION="v20.18.1"
NODE_ARM64_URL="https://nodejs.org/dist/${NODE_VERSION}/node-${NODE_VERSION}-darwin-arm64.tar.gz"
NODE_ARM64_SHA256="9e92ce1032455a9cc419fe71e908b27ae477799371b45a0844eedb02279922a4"

JELLYFIN_FFMPEG_VERSION="7.1.4-3"
JELLYFIN_FFMPEG_ARM64_URL="https://github.com/jellyfin/jellyfin-ffmpeg/releases/download/v${JELLYFIN_FFMPEG_VERSION}/jellyfin-ffmpeg_${JELLYFIN_FFMPEG_VERSION}_portable_macarm64-gpl.tar.xz"
JELLYFIN_FFMPEG_ARM64_SHA256="99d689816a41075574928a0b3059101fd454fc58f465c99105a73b5c415ac86d"

ARCH="${1:-$(uname -m)}"
case "${ARCH}" in
	arm64 | aarch64) ARCH="arm64" ;;
	x86_64 | x64 | amd64 | intel) ARCH="x86_64" ;;
	*)
		echo "ERROR: unsupported macOS architecture: ${ARCH} (expected arm64 or x86_64)" >&2
		exit 1
		;;
esac

require_command() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "ERROR: missing required command: $1" >&2
		echo "       $2" >&2
		exit 1
	fi
}

require_command gh "install GitHub CLI (brew install gh) and authenticate or set GH_TOKEN"
require_command curl "install curl"
require_command tar "install tar"
require_command unzip "install unzip"
require_command shasum "shasum ships with macOS"

TEMP_DIR="$(mktemp -d)"
cleanup() {
	rm -rf "${TEMP_DIR}"
}
trap cleanup EXIT

verify_sha256() {
	local file="$1"
	local expected="$2"
	local actual
	actual="$(shasum -a 256 "${file}" | awk '{print $1}')"
	if [ "${actual}" != "${expected}" ]; then
		echo "ERROR: checksum mismatch for ${file}" >&2
		echo "       expected: ${expected}" >&2
		echo "       actual:   ${actual}" >&2
		exit 1
	fi
}

check_macho_arch() {
	local file="$1"
	if command -v lipo >/dev/null 2>&1; then
		local archs
		archs="$(lipo -archs "${file}" 2>/dev/null || true)"
		if [ -n "${archs}" ]; then
			case " ${archs} " in
			*" ${ARCH} "*) return 0 ;;
			esac
			echo "ERROR: ${file} is built for '${archs}', expected ${ARCH}" >&2
			exit 1
		fi
	fi
	if command -v file >/dev/null 2>&1; then
		local description
		description="$(file -b "${file}")"
		case "${description}" in
		*"${ARCH}"*) return 0 ;;
		esac
		echo "ERROR: ${file} architecture mismatch: ${description} (expected ${ARCH})" >&2
		exit 1
	fi
}

find_service_file() {
	local name="$1"
	local match
	match="$(find "${SERVICE_DIR}" -type f -name "${name}" | head -n 1)"
	if [ -z "${match}" ]; then
		echo "ERROR: ${name} not found inside ${SERVICE_ASSET}" >&2
		exit 1
	fi
	printf '%s\n' "${match}"
}

mkdir -p "${BINARIES_DIR}" "${RESOURCES_DIR}"

echo "==> Downloading ${SERVICE_ASSET} (${SERVICE_REPO} ${SERVICE_VERSION})..."
gh release download "${SERVICE_VERSION}" \
	--repo "${SERVICE_REPO}" \
	--pattern "${SERVICE_ASSET}" \
	--dir "${TEMP_DIR}" \
	--clobber

SERVICE_DIR="${TEMP_DIR}/stremio-service"
mkdir -p "${SERVICE_DIR}"
unzip -o -q "${TEMP_DIR}/${SERVICE_ASSET}" -d "${SERVICE_DIR}"

SERVER_JS="$(find_service_file "server.js")"
cp "${SERVER_JS}" "${RESOURCES_DIR}/server.cjs"

if [ "${ARCH}" = "x86_64" ]; then
	echo "==> Copying Intel runtime, ffmpeg and ffprobe from ${SERVICE_ASSET}..."
	SERVICE_RUNTIME="$(find_service_file "stremio-runtime")"
	SERVICE_FFMPEG="$(find_service_file "ffmpeg")"
	SERVICE_FFPROBE="$(find_service_file "ffprobe")"
	cp "${SERVICE_RUNTIME}" "${BINARIES_DIR}/stremio-runtime-macos"
	cp "${SERVICE_FFMPEG}" "${RESOURCES_DIR}/ffmpeg"
	cp "${SERVICE_FFPROBE}" "${RESOURCES_DIR}/ffprobe"
else
	echo "==> Downloading Node.js ${NODE_VERSION} (darwin-arm64) server runtime..."
	curl -L --fail --retry 3 -o "${TEMP_DIR}/node.tar.gz" "${NODE_ARM64_URL}"
	verify_sha256 "${TEMP_DIR}/node.tar.gz" "${NODE_ARM64_SHA256}"
	mkdir -p "${TEMP_DIR}/node"
	tar -xzf "${TEMP_DIR}/node.tar.gz" -C "${TEMP_DIR}/node" --strip-components 1
	if [ ! -s "${TEMP_DIR}/node/bin/node" ]; then
		echo "ERROR: node binary not found in Node.js tarball" >&2
		exit 1
	fi
	cp "${TEMP_DIR}/node/bin/node" "${BINARIES_DIR}/stremio-runtime-macos"

	echo "==> Downloading jellyfin-ffmpeg ${JELLYFIN_FFMPEG_VERSION} (macarm64)..."
	curl -L --fail --retry 3 -o "${TEMP_DIR}/ffmpeg.tar.xz" "${JELLYFIN_FFMPEG_ARM64_URL}"
	verify_sha256 "${TEMP_DIR}/ffmpeg.tar.xz" "${JELLYFIN_FFMPEG_ARM64_SHA256}"
	mkdir -p "${TEMP_DIR}/ffmpeg"
	tar -xJf "${TEMP_DIR}/ffmpeg.tar.xz" -C "${TEMP_DIR}/ffmpeg"
	ARM_FFMPEG="$(find "${TEMP_DIR}/ffmpeg" -type f -name ffmpeg | head -n 1)"
	ARM_FFPROBE="$(find "${TEMP_DIR}/ffmpeg" -type f -name ffprobe | head -n 1)"
	if [ -z "${ARM_FFMPEG}" ] || [ -z "${ARM_FFPROBE}" ]; then
		echo "ERROR: ffmpeg/ffprobe not found inside jellyfin-ffmpeg archive" >&2
		exit 1
	fi
	cp "${ARM_FFMPEG}" "${RESOURCES_DIR}/ffmpeg"
	cp "${ARM_FFPROBE}" "${RESOURCES_DIR}/ffprobe"
fi

chmod +x "${BINARIES_DIR}/stremio-runtime-macos" "${RESOURCES_DIR}/ffmpeg" "${RESOURCES_DIR}/ffprobe"

REQUIRED_FILES=(
	"${BINARIES_DIR}/stremio-runtime-macos"
	"${RESOURCES_DIR}/server.cjs"
	"${RESOURCES_DIR}/ffmpeg"
	"${RESOURCES_DIR}/ffprobe"
)
for required in "${REQUIRED_FILES[@]}"; do
	if [ ! -s "${required}" ]; then
		echo "ERROR: required file missing or empty: ${required}" >&2
		exit 1
	fi
done

check_macho_arch "${BINARIES_DIR}/stremio-runtime-macos"
check_macho_arch "${RESOURCES_DIR}/ffmpeg"
check_macho_arch "${RESOURCES_DIR}/ffprobe"

echo "==> macOS shell dependencies ready (${ARCH})"
