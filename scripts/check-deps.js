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

// Files that must be present before a build can succeed.
const REQUIRED = [
  {
    file: path.join(ROOT, "src-tauri", "resources", "server.cjs"),
    minBytes: 100 * 1024, // ~6 MB in practice; 100 KB is a safe lower bound
    label: "src-tauri/resources/server.cjs",
  },
];

// Optional: only warn, don't fail (platform-specific binaries may differ)
const OPTIONAL = [
  {
    file: path.join(ROOT, "src-tauri", "resources", "ffmpeg.exe"),
    label: "src-tauri/resources/ffmpeg.exe (Windows)",
  },
  {
    file: path.join(ROOT, "src-tauri", "resources", "ffprobe.exe"),
    label: "src-tauri/resources/ffprobe.exe (Windows)",
  },
  {
    file: path.join(ROOT, "src-tauri", "resources", "ffmpeg"),
    label: "src-tauri/resources/ffmpeg (macOS/Linux)",
  },
  {
    file: path.join(ROOT, "src-tauri", "resources", "ffprobe"),
    label: "src-tauri/resources/ffprobe (macOS/Linux)",
  },
  {
    file: path.join(ROOT, "src-tauri", "resources", "libmpv-2.dll"),
    label: "src-tauri/resources/libmpv-2.dll (Windows)",
  },
];

// ── Helpers ───────────────────────────────────────────────────────────────────

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

// ── Check required files ──────────────────────────────────────────────────────

let failed = false;

for (const dep of REQUIRED) {
  if (!fileOk(dep.file, dep.minBytes)) {
    if (!failed) {
      console.error("");
      console.error(
        "╔══════════════════════════════════════════════════════════╗",
      );
      console.error(
        "║          Stremio Lightning — Missing Dependencies        ║",
      );
      console.error(
        "╚══════════════════════════════════════════════════════════╝",
      );
      console.error("");
      console.error(
        "  The following required files are missing or incomplete:",
      );
      console.error("");
    }
    console.error(`  ✗  ${dep.label}`);
    failed = true;
  }
}

if (failed) {
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

// ── Warn about optional files ─────────────────────────────────────────────────

const missingOptional = OPTIONAL.filter((dep) => !fileOk(dep.file));

if (missingOptional.length > 0 && missingOptional.length < OPTIONAL.length) {
  // Only warn if some (not all) optional files are missing — all missing likely
  // means the platform just doesn't need them.
  console.warn("");
  console.warn(
    "[check-deps] Warning: some optional platform files are missing:",
  );
  for (const dep of missingOptional) {
    console.warn(`             -  ${dep.label}`);
  }
  console.warn(
    "             This is expected if you are not targeting that platform.",
  );
  console.warn("");
}

console.log("[check-deps] All required dependencies are present. ✓");
