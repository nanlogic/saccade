#!/usr/bin/env node

const { spawn } = require("node:child_process");
const fs = require("node:fs/promises");
const net = require("node:net");
const path = require("node:path");

function usage() {
  console.error(`usage:
  node scripts/probe_reflex_live_commands.js \\
    --servoshell /path/to/servoshell \\
    [--url http://127.0.0.1:4173/] [--headed] \\
    [--window-size 1280x900] [--duration-ms 6500] \\
    [--output-dir runs/reflex_live/<name>]
`);
}

function parseArgs(argv) {
  const args = {
    url: "http://127.0.0.1:4173/",
    durationMs: 6500,
    sampleMs: 250,
    headless: true,
    windowSize: "1280x900",
    outputDir: null,
  };

  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--servoshell") {
      args.servoshell = argv[++i];
    } else if (arg === "--url") {
      args.url = argv[++i];
    } else if (arg === "--duration-ms") {
      args.durationMs = Number(argv[++i]);
    } else if (arg === "--sample-ms") {
      args.sampleMs = Number(argv[++i]);
    } else if (arg === "--window-size") {
      args.windowSize = argv[++i];
    } else if (arg === "--output-dir") {
      args.outputDir = argv[++i];
    } else if (arg === "--headed") {
      args.headless = false;
    } else if (arg === "--headless") {
      args.headless = true;
    } else if (arg === "--help" || arg === "-h") {
      usage();
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }

  if (!args.servoshell) {
    throw new Error("--servoshell is required");
  }
  if (!Number.isFinite(args.durationMs) || args.durationMs <= 0) {
    throw new Error("--duration-ms must be positive");
  }
  if (!Number.isFinite(args.sampleMs) || args.sampleMs <= 0) {
    throw new Error("--sample-ms must be positive");
  }
  if (!args.outputDir) {
    args.outputDir = path.join("runs", "reflex_live", `live_${Date.now()}`);
  }

  return args;
}

async function chooseLoopbackPort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer();
    server.listen(0, "127.0.0.1", () => {
      const port = server.address().port;
      server.close(() => resolve(port));
    });
    server.on("error", reject);
  });
}

function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

async function request(port, method, route, body, timeoutMs = 5000) {
  const ctrl = new AbortController();
  const timer = setTimeout(() => ctrl.abort(), timeoutMs);
  try {
    const response = await fetch(`http://127.0.0.1:${port}${route}`, {
      method,
      headers: body === undefined ? {} : { "content-type": "application/json" },
      body: body === undefined ? undefined : JSON.stringify(body),
      signal: ctrl.signal,
    });
    const text = await response.text();
    let json;
    try {
      json = text ? JSON.parse(text) : null;
    } catch {
      json = { raw: text };
    }
    return { status: response.status, body: json };
  } finally {
    clearTimeout(timer);
  }
}

async function waitForStatus(port, child, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let lastError = "";
  while (Date.now() < deadline) {
    if (child.exitCode !== null) {
      throw new Error(`servoshell exited before WebDriver ready: ${child.exitCode}`);
    }
    try {
      const response = await request(port, "GET", "/status");
      if (response.status >= 200 && response.status < 300) {
        return response.body;
      }
      lastError = JSON.stringify(response.body);
    } catch (error) {
      lastError = error.message;
    }
    await sleep(250);
  }
  throw new Error(`WebDriver status timeout: ${lastError}`);
}

async function newSession(port) {
  const response = await request(port, "POST", "/session", {
    capabilities: {
      alwaysMatch: {
        browserName: "servo",
        timeouts: { script: 120000, pageLoad: 300000, implicit: 0 },
      },
    },
  });
  const sessionId =
    response.body?.value?.sessionId ||
    response.body?.sessionId ||
    response.body?.value?.sessionId;
  if (!sessionId) {
    throw new Error(`new session response missing session id: ${JSON.stringify(response.body)}`);
  }
  return sessionId;
}

async function execute(port, sessionId, script) {
  const response = await request(port, "POST", `/session/${sessionId}/execute/sync`, {
    script,
    args: [],
  });
  return response.body?.value ?? null;
}

async function navigate(port, sessionId, url) {
  await request(port, "POST", `/session/${sessionId}/url`, { url }, 30000);
}

async function sampleGame(port, sessionId) {
  return await execute(
    port,
    sessionId,
    `return (() => {
      const canvas = document.getElementById("game");
      let debug = null;
      try { debug = JSON.parse(canvas && canvas.dataset.debug || "null"); }
      catch (error) { debug = { parseError: String(error), raw: canvas && canvas.dataset.debug }; }
      return {
        observedAtMs: Date.now(),
        performanceNow: performance.now(),
        title: document.title,
        readyState: document.readyState,
        hasCanvas: Boolean(canvas),
        canvas: canvas ? {
          width: canvas.width,
          height: canvas.height,
          clientWidth: canvas.clientWidth,
          clientHeight: canvas.clientHeight
        } : null,
        debug
      };
    })();`,
  );
}

async function waitForGameDebug(port, sessionId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await sampleGame(port, sessionId);
    if (last?.hasCanvas && typeof last.debug?.time === "number") {
      return last;
    }
    await sleep(250);
  }
  throw new Error(`game debug did not become ready: ${JSON.stringify(last)}`);
}

async function appendCommand(commandPath, command) {
  await fs.appendFile(commandPath, `${JSON.stringify(command)}\n`);
}

async function readJsonl(filePath) {
  try {
    const text = await fs.readFile(filePath, "utf8");
    return text
      .split("\n")
      .map((line) => line.trim())
      .filter(Boolean)
      .map((line) => {
        try {
          return JSON.parse(line);
        } catch (error) {
          return { parse_error: String(error), raw: line };
        }
      });
  } catch (error) {
    if (error.code === "ENOENT") {
      return [];
    }
    throw error;
  }
}

function summarizeNumbers(values) {
  const nums = values.filter((value) => Number.isFinite(value)).sort((a, b) => a - b);
  if (!nums.length) {
    return null;
  }
  const pct = (p) => nums[Math.min(nums.length - 1, Math.floor((nums.length - 1) * p))];
  return {
    count: nums.length,
    min: nums[0],
    p50: pct(0.5),
    p95: pct(0.95),
    max: nums[nums.length - 1],
  };
}

function summarizeSamples(samples, startedAtMs, endedAtMs) {
  const debugSamples = samples.filter((sample) => sample.debug && typeof sample.debug.time === "number");
  const first = debugSamples[0] || null;
  const last = debugSamples[debugSamples.length - 1] || null;
  const sampleWallTimeDeltaSec = first && last ? (last.wall_ms - first.wall_ms) / 1000 : null;
  const gameTimeDeltaSec = first && last ? last.debug.time - first.debug.time : null;
  const timeScale =
    typeof gameTimeDeltaSec === "number" && sampleWallTimeDeltaSec > 0
      ? gameTimeDeltaSec / sampleWallTimeDeltaSec
      : null;
  return {
    sample_count: samples.length,
    debug_sample_count: debugSamples.length,
    first_debug: first?.debug || null,
    last_debug: last?.debug || null,
    sample_wall_time_delta_sec: sampleWallTimeDeltaSec,
    process_wall_time_delta_sec: (endedAtMs - startedAtMs) / 1000,
    game_time_delta_sec: gameTimeDeltaSec,
    time_scale: timeScale,
    camera_delta:
      first?.debug?.camera && last?.debug?.camera
        ? {
            x: last.debug.camera.x - first.debug.camera.x,
            y: last.debug.camera.y - first.debug.camera.y,
          }
        : null,
  };
}

function summarizeReceipts(receipts) {
  const byType = {};
  for (const receipt of receipts) {
    const key = `${receipt.type || "unknown"}:${receipt.status || "unknown"}`;
    byType[key] = (byType[key] || 0) + 1;
  }
  const dispatchMs = receipts
    .map((receipt) =>
      typeof receipt.dispatch_ns === "number" ? receipt.dispatch_ns / 1_000_000 : null,
    )
    .filter((value) => value !== null);
  return {
    count: receipts.length,
    by_type_status: byType,
    dispatch_ms: summarizeNumbers(dispatchMs),
  };
}

function summarizeFrames(events) {
  const frames = events.filter(
    (event) => event.kind === "saccade_reflex_frame" || "readback_ok" in event,
  );
  const bridgeInputs = events.filter((event) => event.kind === "saccade_reflex_test_drag");
  const readbackMs = frames
    .map((frame) => (typeof frame.readback_ns === "number" ? frame.readback_ns / 1_000_000 : null))
    .filter((value) => value !== null);
  return {
    event_count: events.length,
    count: frames.length,
    readback_ok: frames.filter((frame) => frame.readback_ok === true).length,
    bridge_input_events: bridgeInputs.length,
    readback_ms: summarizeNumbers(readbackMs),
    dropped_logs_max: Math.max(0, ...frames.map((frame) => frame.dropped_logs || 0)),
  };
}

async function main() {
  const args = parseArgs(process.argv);
  const port = await chooseLoopbackPort();
  await fs.mkdir(args.outputDir, { recursive: true });

  const commandPath = path.join(args.outputDir, "commands.jsonl");
  const receiptPath = path.join(args.outputDir, "receipts.jsonl");
  const framePath = path.join(args.outputDir, "frames.jsonl");
  await fs.writeFile(commandPath, "");

  const commandArgs = [
    `--webdriver=${port}`,
    "--temporary-storage",
    `--window-size=${args.windowSize}`,
  ];
  if (args.headless) {
    commandArgs.unshift("-z");
  }
  commandArgs.push(args.url);

  const child = spawn(args.servoshell, commandArgs, {
    env: {
      ...process.env,
      SACCADE_REFLEX_COMMANDS_PATH: commandPath,
      SACCADE_REFLEX_RECEIPTS_PATH: receiptPath,
      SACCADE_REFLEX_OBSERVE_PATH: framePath,
      SACCADE_REFLEX_OBSERVE_MAX_FRAMES: "420",
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout += chunk.toString();
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk.toString();
  });

  const startedAtMs = Date.now();
  const samples = [];
  let sessionId = null;
  let commandsIssued = false;
  let ok = false;

  try {
    await waitForStatus(port, child, 25000);
    sessionId = await newSession(port);
    await navigate(port, sessionId, args.url);
    await waitForGameDebug(port, sessionId, 15000);

    const deadline = Date.now() + args.durationMs;
    while (Date.now() <= deadline) {
      const observedAtMs = Date.now();
      const sample = await sampleGame(port, sessionId);
      samples.push({ wall_ms: observedAtMs - startedAtMs, ...sample });

      if (!commandsIssued && samples.length >= 3) {
        await appendCommand(commandPath, {
          id: "live-ping-1",
          type: "ping",
        });
        await appendCommand(commandPath, {
          id: "live-drag-1",
          type: "drag",
          start: { x: 640, y: 450 },
          end: { x: 1000, y: 450 },
          frames: 8,
        });
        commandsIssued = true;
      }

      await sleep(args.sampleMs);
    }
    ok = true;
  } finally {
    if (sessionId) {
      try {
        await request(port, "DELETE", `/session/${sessionId}`);
      } catch {}
    }
    child.kill("SIGTERM");
    await sleep(750);
  }

  const endedAtMs = Date.now();
  const receipts = await readJsonl(receiptPath);
  const frames = await readJsonl(framePath);
  const report = {
    ok,
    args,
    command: args.servoshell,
    command_args: commandArgs,
    webdriver_port: port,
    paths: {
      commands: commandPath,
      receipts: receiptPath,
      frames: framePath,
    },
    started_at_ms: startedAtMs,
    ended_at_ms: endedAtMs,
    summary: {
      samples: summarizeSamples(samples, startedAtMs, endedAtMs),
      receipts: summarizeReceipts(receipts),
      frames: summarizeFrames(frames),
    },
    samples,
    receipts,
    frames,
    process: {
      exit_code: child.exitCode,
      signal_code: child.signalCode,
      stdout_tail: stdout.split("\n").slice(-20).join("\n"),
      stderr_tail: stderr.split("\n").slice(-80).join("\n"),
    },
  };

  const reportPath = path.join(args.outputDir, "report.json");
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(
    JSON.stringify(
      {
        ok,
        report: reportPath,
        summary: report.summary,
        stderr_tail: report.process.stderr_tail.split("\n").slice(-8).join("\n"),
      },
      null,
      2,
    ),
  );
}

main().catch((error) => {
  console.error(error.stack || error.message);
  usage();
  process.exit(1);
});
