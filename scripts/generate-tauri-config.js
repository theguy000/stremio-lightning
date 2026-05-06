#!/usr/bin/env node
/**
 * Stremio Lightning - Dynamic Tauri Config Generator
 *
 * Generates a platform-aware tauri.conf.json from tauri.conf.base.json
 * by adjusting resource paths, sidecar names, and platform-specific settings.
 *
 * Called by the npm `tauri` wrapper before the Tauri CLI reads its config.
 */

import { readFileSync, writeFileSync, existsSync } from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");
const BASE_CONFIG = path.join(ROOT, "src-tauri", "tauri.conf.base.json");
const OUTPUT_CONFIG = path.join(ROOT, "src-tauri", "tauri.conf.json");

function detectPlatform() {
  const os = process.platform;
  const arch = process.arch;

  if (os === "win32") return "windows";
  if (os === "darwin") return arch === "arm64" ? "macos-arm64" : "macos-x86_64";
  if (os === "linux") return "linux";

  console.error(`[generate-config] Unsupported platform: ${os}`);
  process.exit(1);
}

const platform = detectPlatform();

// Read base config
if (!existsSync(BASE_CONFIG)) {
  console.error(`[generate-config] Base config not found: ${BASE_CONFIG}`);
  console.error(`  Create tauri.conf.base.json with platform-agnostic settings.`);
  process.exit(1);
}

const config = JSON.parse(readFileSync(BASE_CONFIG, "utf-8"));

// --- Platform-specific resource paths ---
const resources = ["resources/server.cjs"];

if (platform === "windows") {
  resources.push("resources/ffmpeg.exe", "resources/ffprobe.exe", "resources/libmpv-2.dll");
} else {
  // macOS and Linux
  resources.push("resources/ffmpeg", "resources/ffprobe");
}

config.bundle.resources = resources;

// Write output
writeFileSync(OUTPUT_CONFIG, JSON.stringify(config, null, 2) + "\n");

console.log(`[generate-config] Platform: ${platform}`);
console.log(`[generate-config] Resources: ${resources.join(", ")}`);
console.log(`[generate-config] Generated: ${OUTPUT_CONFIG}`);
