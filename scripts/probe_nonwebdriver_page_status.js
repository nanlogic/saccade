#!/usr/bin/env node

const { spawn } = require("node:child_process");
const fs = require("node:fs/promises");
const path = require("node:path");

function parseArgs(argv) {
  const args = {
    headless: true,
    timeoutMs: 20000,
    windowSize: "1280x900",
    outputDir: path.join("runs", "nonwebdriver_page_status", `probe_${Date.now()}`),
  };
  for (let index = 2; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--servoshell") args.servoshell = argv[++index];
    else if (arg === "--url") args.url = argv[++index];
    else if (arg === "--output-dir") args.outputDir = argv[++index];
    else if (arg === "--timeout-ms") args.timeoutMs = Number(argv[++index]);
    else if (arg === "--window-size") args.windowSize = argv[++index];
    else if (arg === "--headed") args.headless = false;
    else if (arg === "--headless") args.headless = true;
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!args.servoshell) throw new Error("--servoshell is required");
  if (!args.url) throw new Error("--url is required");
  if (!Number.isFinite(args.timeoutMs) || args.timeoutMs <= 0) {
    throw new Error("--timeout-ms must be positive");
  }
  return args;
}

async function readJsonl(filePath) {
  try {
    return (await fs.readFile(filePath, "utf8"))
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => JSON.parse(line));
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
}

function waitForExit(child, timeoutMs) {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return Promise.race([
    new Promise((resolve) => child.once("exit", resolve)),
    new Promise((resolve) => setTimeout(resolve, timeoutMs)),
  ]);
}

function safeUrl(value) {
  try {
    const parsed = new URL(value);
    parsed.search = "";
    parsed.hash = "";
    return parsed.href;
  } catch {
    return String(value || "").split("?", 1)[0].split("#", 1)[0];
  }
}

function sanitizeLogLine(line) {
  return String(line).replace(/(?:https?|file):\/\/[^\s"')]+/g, (value) => safeUrl(value));
}

async function main() {
  const args = parseArgs(process.argv);
  const outputDir = path.resolve(args.outputDir);
  const commandsPath = path.join(outputDir, "commands.jsonl");
  const receiptsPath = path.join(outputDir, "receipts.jsonl");
  const framesPath = path.join(outputDir, "frames.jsonl");
  const reportPath = path.join(outputDir, "report.json");
  await fs.mkdir(outputDir, { recursive: true });
  let commandSequence = 1;
  await fs.writeFile(
    commandsPath,
    `${JSON.stringify({ id: `page-status-${commandSequence}`, type: "page_status" })}\n`,
  );

  const commandArgs = [`--window-size=${args.windowSize}`];
  if (args.headless) commandArgs.unshift("--headless");
  commandArgs.push(args.url);
  const startedAt = Date.now();
  let stdout = "";
  let stderr = "";
  const child = spawn(args.servoshell, commandArgs, {
    env: {
      ...process.env,
      RUST_LOG: process.env.RUST_LOG || "error",
      SACCADE_REFLEX_COMMANDS_PATH: commandsPath,
      SACCADE_REFLEX_RECEIPTS_PATH: receiptsPath,
      SACCADE_REFLEX_OBSERVE_PATH: framesPath,
      SACCADE_REFLEX_OBSERVE_EVERY_N: "5",
      SACCADE_REFLEX_OBSERVE_MAX_FRAMES: "120",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  child.stdout.on("data", (chunk) => (stdout += chunk.toString()));
  child.stderr.on("data", (chunk) => (stderr += chunk.toString()));

  let receipt = null;
  let lastCommandAt = startedAt;
  const deadline = Date.now() + args.timeoutMs;
  while (Date.now() < deadline) {
    const receipts = await readJsonl(receiptsPath);
    const pageReceipts = receipts.filter((item) => item.type === "page_status");
    receipt = pageReceipts[pageReceipts.length - 1] || null;
    if (
      receipt?.status === "ok" &&
      receipt.page?.ready_state !== "loading" &&
      receipt.page?.cloudflare_challenge !== true
    ) {
      break;
    }
    if (child.exitCode !== null) break;
    if (receipt && Date.now() - lastCommandAt >= 250) {
      commandSequence += 1;
      await fs.appendFile(
        commandsPath,
        `${JSON.stringify({ id: `page-status-${commandSequence}`, type: "page_status" })}\n`,
      );
      lastCommandAt = Date.now();
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }

  child.kill("SIGTERM");
  await waitForExit(child, 2000);
  if (child.exitCode === null && child.signalCode === null) child.kill("SIGKILL");

  const report = {
    ok:
      receipt?.status === "ok" &&
      receipt.page?.ready_state !== "loading" &&
      receipt.page?.cloudflare_challenge !== true,
    engine: "saccade-servoshell-inprocess-page-status-v0",
    url: safeUrl(args.url),
    elapsed_ms: Date.now() - startedAt,
    webdriver_enabled: commandArgs.some((arg) => arg.startsWith("--webdriver")),
    page_status_attempts: commandSequence,
    command: [args.servoshell, ...commandArgs.slice(0, -1), safeUrl(commandArgs.at(-1))],
    receipt,
    process: {
      exit_code: child.exitCode,
      signal_code: child.signalCode,
      stdout_tail: stdout.split("\n").slice(-20).map(sanitizeLogLine),
      stderr_tail: stderr.split("\n").slice(-40).map(sanitizeLogLine),
    },
    artifacts: { commands: commandsPath, receipts: receiptsPath, frames: framesPath },
  };
  await fs.writeFile(reportPath, `${JSON.stringify(report, null, 2)}\n`);
  process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  process.exitCode = report.ok ? 0 : 1;
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exitCode = 1;
});
