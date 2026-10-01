// Run both streaming engines sequentially on the shared port 11470 and diff
// their HTTP surface.
//
// Usage:
//   node scripts/parity/run.mjs [options]
//
// Options:
//   --legacy-root <dir>     folder holding resources/ for the legacy engine
//                           (default: crates/stremio-lightning-windows)
//   --stream-server <path>  override the stream-server binary
//                           (default: the shell's resources/stream-server.exe,
//                            or $STREMIO_PARITY_STREAM_SERVER)
//   --only <legacy|stream-server>  run a single engine and snapshot it
//   --profile <windows|linux|macos|all>  which shell's entry points gate the diff
//                           (default: windows)
//   --out-dir <dir>         where snapshots/reports are written (default: target/parity)
//   --timeout <ms>          readiness timeout per engine (default: 60000)
//
// Windows setup no longer installs the legacy engine, so a full comparison
// needs --legacy-root pointing at a folder that still has it. Snapshot just the
// current engine with --only stream-server.
//
// Both engines bind the fixed port 11470 one at a time, so close the desktop
// app (or stop its streaming server) before running this.

import { spawn } from "node:child_process";
import { createConnection } from "node:net";
import { createWriteStream, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import {
  DEFAULT_BASE_URL,
  probeAll,
  diffSnapshots,
  fetchWithTimeout,
} from "./endpoints.mjs";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..", "..");

const LEGACY_HELP = [
  "The Windows setup no longer installs the legacy engine, so it must be supplied.",
  "Point --legacy-root at a folder that still contains the legacy engine files:",
  "",
  "  node scripts/parity/run.mjs --legacy-root <legacy-engine-folder>",
  "",
  "To snapshot only the current engine instead:",
  "",
  "  npm run test:parity -- --only stream-server",
].join("\n");

function parseArgs(argv) {
  const args = {
    legacyRoot: join(repoRoot, "crates", "stremio-lightning-windows"),
    streamServer: process.env.STREMIO_PARITY_STREAM_SERVER ?? null,
    only: null,
    profile: "windows",
    outDir: join(repoRoot, "target", "parity"),
    timeout: 60000,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const value = argv[i];
    if (value === "--legacy-root") args.legacyRoot = resolve(argv[++i]);
    else if (value === "--stream-server") args.streamServer = resolve(argv[++i]);
    else if (value === "--only") args.only = argv[++i];
    else if (value === "--profile") args.profile = argv[++i];
    else if (value === "--out-dir") args.outDir = resolve(argv[++i]);
    else if (value === "--timeout") args.timeout = Number(argv[++i]);
  }
  return args;
}

const args = parseArgs(process.argv.slice(2));
mkdirSync(args.outDir, { recursive: true });

const isolateHome = join(args.outDir, "home");
mkdirSync(join(isolateHome, "AppData", "Roaming"), { recursive: true });
mkdirSync(join(isolateHome, "AppData", "Local"), { recursive: true });

// Mirrors how the shell launches the engine: resources/stream-server.exe with
// --no-tray, and the resources directory prepended to PATH for ffmpeg/ffprobe.
function streamServerSpec() {
  const resourcesDir = join(repoRoot, "crates", "stremio-lightning-windows", "resources");
  return {
    name: "stream-server",
    label: "stream-server (open source)",
    command: args.streamServer ?? join(resourcesDir, "stream-server.exe"),
    args: ["--no-tray"],
    env: { PATH: `${resourcesDir};${process.env.PATH ?? ""}` },
  };
}

function legacySpec() {
  const resourcesDir = join(args.legacyRoot, "resources");
  return {
    name: "legacy",
    label: "legacy server.cjs (stremio-runtime)",
    command: join(resourcesDir, "stremio-runtime.exe"),
    args: [join(resourcesDir, "server.cjs")],
    env: {
      NO_CORS: "1",
      FFMPEG_BIN: join(resourcesDir, "ffmpeg.exe"),
      FFPROBE_BIN: join(resourcesDir, "ffprobe.exe"),
    },
  };
}

function selectedSpecs() {
  if (args.only === "stream-server") return [streamServerSpec()];
  if (args.only === "legacy") return [legacySpec()];
  if (args.only) {
    console.error(`Unknown --only value '${args.only}'. Use 'legacy' or 'stream-server'.`);
    process.exit(2);
  }
  return [legacySpec(), streamServerSpec()];
}

function startEngine(spec) {
  const stdoutLog = join(args.outDir, `${spec.name}.stdout.log`);
  const stderrLog = join(args.outDir, `${spec.name}.stderr.log`);
  let spawnError = null;
  const child = spawn(spec.command, spec.args, {
    cwd: repoRoot,
    env: {
      ...process.env,
      ...spec.env,
      APPDATA: join(isolateHome, "AppData", "Roaming"),
      LOCALAPPDATA: join(isolateHome, "AppData", "Local"),
    },
    windowsHide: true,
  });
  child.once("error", (error) => {
    spawnError = error;
  });
  if (child.stdout) child.stdout.pipe(createWriteStream(stdoutLog, { flags: "w" }));
  if (child.stderr) child.stderr.pipe(createWriteStream(stderrLog, { flags: "w" }));
  return { child, stdoutLog, stderrLog, spawnError: () => spawnError };
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function heartbeatOk(baseUrl) {
  try {
    const res = await fetchWithTimeout(`${baseUrl}/heartbeat`, {}, 2000);
    return res.ok;
  } catch {
    return false;
  }
}

async function waitReady(baseUrl, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (await heartbeatOk(baseUrl)) return true;
    await sleep(500);
  }
  return false;
}

function portIsFree(url) {
  const { hostname, port } = new URL(url);
  return new Promise((res) => {
    const socket = createConnection({ host: hostname, port: Number(port) });
    socket.setTimeout(1000);
    socket.once("connect", () => {
      socket.destroy();
      res(false);
    });
    socket.once("timeout", () => {
      socket.destroy();
      res(true);
    });
    socket.once("error", () => res(true));
  });
}

async function waitPortFree(baseUrl) {
  const deadline = Date.now() + 15000;
  while (Date.now() < deadline) {
    if (await portIsFree(baseUrl)) return true;
    await sleep(500);
  }
  return false;
}

async function runEngine(spec) {
  if (!existsSync(spec.command)) {
    console.error(`\n${spec.label}: engine not found at:\n  ${spec.command}`);
    if (spec.name === "legacy") {
      console.error(`\n${LEGACY_HELP}`);
    }
    return { spec, ready: false, snapshot: null, snapshotPath: null, missing: true };
  }

  if (!(await portIsFree(DEFAULT_BASE_URL))) {
    console.error(
      `\nPort 11470 is busy. Close the Stremio Lightning app (or stop its streaming server) before running parity.`,
    );
    process.exit(2);
  }

  console.log(`\n==> Starting ${spec.label}`);
  const { child, stdoutLog, stderrLog, spawnError } = startEngine(spec);
  let exited = false;
  child.once("exit", () => {
    exited = true;
  });
  child.once("error", () => {
    exited = true;
  });

  const ready = await waitReady(DEFAULT_BASE_URL, args.timeout);
  if (!ready) {
    console.error(`Engine ${spec.name} did not become ready within ${args.timeout} ms.`);
    if (spawnError()) {
      console.error(`  Could not start it: ${spawnError().message}`);
    }
    console.error(`  stdout: ${stdoutLog}\n  stderr: ${stderrLog}`);
  } else {
    console.log(`    ready at ${DEFAULT_BASE_URL}`);
  }

  const snapshot = ready ? await probeAll(DEFAULT_BASE_URL) : null;

  if (!exited) child.kill();
  await waitPortFree(DEFAULT_BASE_URL);

  const snapshotPath = join(args.outDir, `${spec.name}.snapshot.json`);
  if (snapshot) writeFileSync(snapshotPath, JSON.stringify(snapshot, null, 2));
  return { spec, ready, snapshot, snapshotPath };
}

function printReport(baseline, candidate) {
  const rows = diffSnapshots(baseline.snapshot, candidate.snapshot, args.profile);
  console.log(`\n==> Endpoint parity (legacy vs stream-server) [profile: ${args.profile}]\n`);
  let criticalFailures = 0;
  for (const row of rows) {
    if (!row.applicable) {
      console.log(`n/a    ${row.id}`);
      continue;
    }
    const state = row.match ? "MATCH" : "DIFF*";
    if (!row.match) criticalFailures += 1;
    console.log(`${state}  ${row.id}`);
    if (!row.match) {
      console.log(`        legacy: ${JSON.stringify(row.baseline)}`);
      console.log(`        ss:     ${JSON.stringify(row.candidate)}`);
    }
  }
  console.log(
    `\n${criticalFailures} critical mismatch(es) for profile '${args.profile}'. (n/a = not used by this shell)`,
  );
  const diffPath = join(args.outDir, "diff.json");
  writeFileSync(
    diffPath,
    JSON.stringify(
      { baseline: baseline.spec.name, candidate: candidate.spec.name, profile: args.profile, rows },
      null,
      2,
    ),
  );
  console.log(`Report: ${diffPath}`);
  return criticalFailures;
}

const selected = selectedSpecs();

const results = [];
for (const spec of selected) {
  const result = await runEngine(spec);
  results.push(result);
  if (result.missing) break;
}

if (results.some((result) => result.missing)) {
  process.exit(2);
}

if (results.length === 1) {
  console.log(`\nSnapshot: ${results[0].snapshotPath}`);
  process.exit(0);
}

const [baseline, candidate] = results;
if (!baseline.snapshot || !candidate.snapshot) {
  console.error("\nOne engine did not produce a snapshot; see logs above.");
  process.exit(1);
}
process.exit(printReport(baseline, candidate) > 0 ? 1 : 0);
