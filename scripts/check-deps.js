#!/usr/bin/env node
/**
 * Stremio Lightning - Pre-build dependency check
 *
 * Verifies that required downloaded artifacts exist before Tauri builds.
 * Called automatically by beforeBuildCommand / beforeDevCommand via npm scripts.
 *
 * If deps are missing, it prints a clear actionable error instead of letting
 * the build fail cryptically later.
 */

import { existsSync, statSync } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");

function detectPlatform() {
  const os = process.platform;
  const arch = process.arch;

  if (os === "win32") return "windows";
  if (os === "darwin") return arch === "arm64" ? "macos-arm64" : "macos-x86_64";
  if (os === "linux") return "linux";

  console.error(`[check-deps] Unsupported platform: ${os}`);
  process.exit(1);
}

function requiredDepsFor(platform) {
  const deps = [
    {
      file: path.join(ROOT, "src-tauri", "resources", "server.cjs"),
      minBytes: 100 * 1024, // ~6 MB in practice; 100 KB is a safe lower bound
      label: "src-tauri/resources/server.cjs",
    },
  ];

  if (platform === "windows") {
    deps.push(
      {
        file: path.join(
          ROOT,
          "src-tauri",
          "binaries",
          "stremio-runtime-x86_64-pc-windows-msvc.exe",
        ),
        minBytes: 100 * 1024,
        label: "src-tauri/binaries/stremio-runtime-x86_64-pc-windows-msvc.exe",
      },
      {
        file: path.join(ROOT, "src-tauri", "resources", "ffmpeg.exe"),
        minBytes: 100 * 1024,
        label: "src-tauri/resources/ffmpeg.exe",
      },
      {
        file: path.join(ROOT, "src-tauri", "resources", "ffprobe.exe"),
        minBytes: 100 * 1024,
        label: "src-tauri/resources/ffprobe.exe",
      },
      {
        file: path.join(ROOT, "src-tauri", "resources", "libmpv-2.dll"),
        minBytes: 100 * 1024,
        label: "src-tauri/resources/libmpv-2.dll",
      },
      {
        file: path.join(ROOT, "src-tauri", "mpv-dev", "mpv.lib"),
        minBytes: 1024,
        label: "src-tauri/mpv-dev/mpv.lib",
      },
    );
  } else {
    const targetTriple =
      platform === "macos-arm64"
        ? "aarch64-apple-darwin"
        : platform === "macos-x86_64"
          ? "x86_64-apple-darwin"
          : "x86_64-unknown-linux-gnu";

    deps.push(
      {
        file: path.join(ROOT, "src-tauri", "binaries", `stremio-runtime-${targetTriple}`),
        minBytes: 100 * 1024,
        label: `src-tauri/binaries/stremio-runtime-${targetTriple}`,
      },
      {
        file: path.join(ROOT, "src-tauri", "resources", "ffmpeg"),
        minBytes: 100 * 1024,
        label: "src-tauri/resources/ffmpeg",
      },
      {
        file: path.join(ROOT, "src-tauri", "resources", "ffprobe"),
        minBytes: 100 * 1024,
        label: "src-tauri/resources/ffprobe",
      },
    );
  }

  return deps;
}

function fileOk(filePath, minBytes) {
  if (!existsSync(filePath)) return false;
  if (minBytes) {
    try {
      return statSync(filePath).size >= minBytes;
    } catch {
      return false;
    }
  }
  return true;
}

const platform = detectPlatform();
const required = requiredDepsFor(platform);
const missing = required.filter((dep) => !fileOk(dep.file, dep.minBytes));

if (missing.length > 0) {
  console.error("");
  console.error("╔══════════════════════════════════════════════════════════╗");
  console.error("║          Stremio Lightning — Missing Dependencies        ║");
  console.error("╚══════════════════════════════════════════════════════════╝");
  console.error("");
  console.error(`  Platform: ${platform}`);
  console.error("");
  console.error("  The following required files are missing or incomplete:");
  console.error("");
  for (const dep of missing) {
    console.error(`  ✗  ${dep.label}`);
  }
  console.error("");
  console.error("  Run the setup script to download them:");
  console.error("");
  console.error("      npm run setup");
  console.error("");
  console.error("  Requirements: gh CLI (authenticated), curl, bash, unzip/7z");
  console.error("  GitHub CLI:   https://cli.github.com/");
  console.error("");
  process.exit(1);
}

console.log(`[check-deps] All required dependencies are present for ${platform}. ✓`);
