#!/usr/bin/env node
/**
 * Stremio Lightning - Dependency Setup Script
 *
 * Downloads server.cjs, ffmpeg, ffprobe, and stremio-runtime for your platform.
 * Run this once after cloning the repo before building or running `tauri dev`.
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
const SERVER_CJS = path.join(ROOT, "src-tauri", "resources", "server.cjs");

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

function checkPrerequisites() {
  let ok = true;

  ok = checkCommand("gh", "Install GitHub CLI: https://cli.github.com/") && ok;
  ok =
    checkCommand(
      "bash",
      "Install Git for Windows (includes bash): https://git-scm.com/ — or use WSL",
    ) && ok;
  ok = checkCommand("curl", "Install curl: https://curl.se/") && ok;

  if (!ok) {
    console.error("\n[setup] Please install the missing tools and try again.");
    process.exit(1);
  }
}

// ── Skip if already downloaded ────────────────────────────────────────────────
function alreadySetUp() {
  if (!existsSync(SERVER_CJS)) return false;
  try {
    // server.cjs is ~6 MB; 100 KB is a safe lower bound to confirm it's real
    return statSync(SERVER_CJS).size > 100 * 1024;
  } catch {
    return false;
  }
}

// ── Main ──────────────────────────────────────────────────────────────────────
const forceFlag = process.argv.includes("--force");
const depsPlatform = detectPlatform();

console.log(`[setup] Platform : ${depsPlatform}`);

if (alreadySetUp() && !forceFlag) {
  console.log("[setup] Dependencies already present. Nothing to do.");
  console.log(
    "        Run with --force to re-download:  npm run setup -- --force",
  );
  process.exit(0);
}

console.log("[setup] Checking prerequisites...");
checkPrerequisites();

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

console.log("\n[setup] All dependencies downloaded successfully.");
console.log("        You can now run: npx tauri dev");
