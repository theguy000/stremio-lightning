#!/usr/bin/env node
/**
 * Downloads native-shell runtime dependencies for the current platform.
 */

import { spawnSync } from "child_process";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, "..");

function scriptForCurrentPlatform() {
  if (process.platform === "linux") return "scripts/download-linux-shell-deps.sh";
  if (process.platform === "win32") return "scripts/download-windows-shell-deps.sh";

  console.error(
    `[setup] Unsupported platform for native shell dependencies: ${process.platform}`,
  );
  console.error("        Supported platforms: linux, win32");
  process.exit(1);
}

const script = scriptForCurrentPlatform();
console.log(`[setup] Running ${script} ...\n`);

const result = spawnSync("bash", [script], {
  stdio: "inherit",
  cwd: ROOT,
  env: { ...process.env },
});

if (result.error) {
  console.error("[setup] Failed to spawn bash:", result.error.message);
  process.exit(1);
}

if (result.status !== 0) {
  console.error(`\n[setup] ${script} exited with code ${result.status}.`);
  process.exit(result.status ?? 1);
}

console.log("\n[setup] Native shell dependencies downloaded successfully.");
