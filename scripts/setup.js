#!/usr/bin/env node
/**
 * Stremio Lightning - Dependency Setup Script
 *
 * Downloads server.cjs, ffmpeg, ffprobe, stremio-runtime, and Windows MPV files
 * for your platform. Run this once after cloning the repo before building or
 * running `tauri dev`.
 *
 * Usage:
 *   npm run setup
 *
 * Requirements:
 *   - gh CLI installed and authenticated (https://cli.github.com/)
 *   - curl
 *   - unzip / 7z  (Windows: 7-Zip; macOS: brew install p7zip; Linux: apt install unzip)
 */

import { spawnSync } from "child_process";
import { existsSync, statSync } from "fs";
import path from "path";
import { fileURLToPath } from "url";

// ── Paths ─────────────────────────────────────────────────────────────────────
const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const SCRIPT = path.join(ROOT, "src-tauri", "scripts", "download-deps.sh");

// ── Platform detection ────────────────────────────────────────────────────────
function detectPlatform() {
  const os = process.platform;
  const arch = process.arch;

  if (os === "win32") return "windows";
  if (os === "darwin") return arch === "arm64" ? "macos-arm64" : "macos-x86_64";
  if (os === "linux") return "linux";

  console.error(`[setup] Unsupported platform: ${os}`);
  process.exit(1);
}

function requiredDepsFor(platform) {
  const deps = [
    {
      file: path.join(ROOT, "src-tauri", "resources", "server.cjs"),
      minBytes: 100 * 1024,
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

// ── Prerequisite checks ───────────────────────────────────────────────────────
function checkCommand(cmd, installHint) {
  const result = spawnSync(cmd, ["--version"], { stdio: "pipe" });
  if (result.error || result.status !== 0) {
    console.error(`[setup] Missing required tool: ${cmd}`);
    console.error(`        ${installHint}`);
    return false;
  }
  return true;
}

function checkPrerequisites(platform) {
  let ok = true;

  ok = checkCommand("gh", "Install GitHub CLI: https://cli.github.com/") && ok;
  ok =
    checkCommand(
      "bash",
      "Install Git for Windows (includes bash): https://git-scm.com/ — or use WSL",
    ) && ok;
  ok = checkCommand("curl", "Install curl: https://curl.se/") && ok;
  ok = checkCommand("unzip", "Install unzip or ensure it is available on PATH") && ok;

  if (platform === "windows" || platform.startsWith("macos")) {
    const has7z =
      spawnSync("7z", ["--help"], { stdio: "pipe" }).status === 0 ||
      spawnSync("7zz", ["--help"], { stdio: "pipe" }).status === 0;
    if (!has7z) {
      console.error("[setup] Missing required tool: 7z or 7zz");
      console.error("        Windows: install 7-Zip; macOS: brew install p7zip");
      ok = false;
    }
  }

  if (platform === "linux") {
    ok = checkCommand("dpkg-deb", "Install dpkg-deb (usually provided by dpkg)") && ok;
    ok = checkCommand("tar", "Install tar") && ok;
  }

  if (!ok) {
    console.error("\n[setup] Please install the missing tools and try again.");
    process.exit(1);
  }
}

// ── Skip if already downloaded ────────────────────────────────────────────────
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

function missingDeps(platform) {
  return requiredDepsFor(platform).filter((dep) => !fileOk(dep.file, dep.minBytes));
}

// ── Main ──────────────────────────────────────────────────────────────────────
const forceFlag = process.argv.includes("--force");
const depsPlatform = detectPlatform();
const missing = missingDeps(depsPlatform);

console.log(`[setup] Platform : ${depsPlatform}`);

if (missing.length === 0 && !forceFlag) {
  console.log("[setup] Dependencies already present. Nothing to do.");
  console.log(
    "        Run with --force to re-download:  npm run setup -- --force",
  );
  process.exit(0);
}

if (missing.length > 0) {
  console.log("[setup] Missing dependencies:");
  for (const dep of missing) {
    console.log(`        - ${dep.label}`);
  }
}

console.log("[setup] Checking prerequisites...");
checkPrerequisites(depsPlatform);

console.log(
  `[setup] Running download-deps.sh --platform ${depsPlatform} ...\n`,
);

const result = spawnSync("bash", [SCRIPT, "--platform", depsPlatform], {
  stdio: "inherit",
  cwd: ROOT,
  env: { ...process.env },
});

if (result.error) {
  console.error("[setup] Failed to spawn bash:", result.error.message);
  process.exit(1);
}

if (result.status !== 0) {
  console.error(
    `\n[setup] download-deps.sh exited with code ${result.status}.`,
  );
  console.error(
    "        Make sure gh is authenticated: run `gh auth login` first.",
  );
  process.exit(result.status);
}

const stillMissing = missingDeps(depsPlatform);
if (stillMissing.length > 0) {
  console.error("\n[setup] Setup finished, but these files are still missing:");
  for (const dep of stillMissing) {
    console.error(`        - ${dep.label}`);
  }
  process.exit(1);
}

console.log("\n[setup] All dependencies downloaded successfully.");
console.log("        You can now run: npm run tauri dev");
