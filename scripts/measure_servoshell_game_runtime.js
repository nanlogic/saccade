#!/usr/bin/env node

const { spawn } = require("node:child_process");
const fs = require("node:fs/promises");
const net = require("node:net");
const path = require("node:path");

function usage() {
  console.error(`usage:
  node scripts/measure_servoshell_game_runtime.js \\
    --servoshell /path/to/servoshell \\
    --url http://127.0.0.1:4173/ \\
    [--headless] [--window-size 1280x900] \\
    [--duration-ms 6000] [--sample-ms 500] \\
    [--output-dir runs/servoshell_runtime/<name>]
`);
}

function parseArgs(argv) {
  const args = {
    url: "http://127.0.0.1:4173/",
    durationMs: 6000,
    sampleMs: 500,
    headless: false,
    windowSize: "1280x900",
    outputDir: null,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--headless") {
      args.headless = true;
    } else if (arg === "--servoshell") {
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
    const safeName = path
      .basename(args.servoshell)
      .replace(/[^a-z0-9_-]+/gi, "_")
      .toLowerCase();
    args.outputDir = path.join(
      "runs",
      "servoshell_runtime",
      `${safeName}_${Date.now()}`,
    );
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
    response.body?.value?.["sessionId"];
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

async function waitForGameCanvas(port, sessionId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await execute(
      port,
      sessionId,
      `return {
        title: document.title,
        readyState: document.readyState,
        hasCanvas: Boolean(document.getElementById("game")),
        hasDebug: Boolean(document.getElementById("game")?.dataset.debug)
      };`,
    );
    if (last?.hasCanvas) {
      return last;
    }
    await sleep(250);
  }
  throw new Error(`game canvas did not become ready: ${JSON.stringify(last)}`);
}

function summarize(samples, startedAtMs, endedAtMs) {
  const debugSamples = samples.filter((sample) => sample.debug && typeof sample.debug.time === "number");
  const first = debugSamples[0] || null;
  const last = debugSamples[debugSamples.length - 1] || null;
  const sampleWallTimeDeltaSec =
    first && last ? (last.wall_ms - first.wall_ms) / 1000 : null;
  const processWallTimeDeltaSec = (endedAtMs - startedAtMs) / 1000;
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
    process_wall_time_delta_sec: processWallTimeDeltaSec,
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

async function main() {
  const args = parseArgs(process.argv);
  const port = await chooseLoopbackPort();
  await fs.mkdir(args.outputDir, { recursive: true });

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
    env: process.env,
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

  let sessionId = null;
  let ok = false;
  const startedAtMs = Date.now();
  const samples = [];
  try {
    await waitForStatus(port, child, 25000);
    sessionId = await newSession(port);
    await navigate(port, sessionId, args.url);
    await waitForGameCanvas(port, sessionId, 15000);
    const deadline = Date.now() + args.durationMs;
    while (Date.now() <= deadline) {
      const observedAtMs = Date.now();
      const sample = await execute(
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
            timeText: document.getElementById("timeReadout")?.textContent || null,
            waveText: document.getElementById("waveReadout")?.textContent || null,
            bossText: document.getElementById("bossReadout")?.textContent || null,
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
      samples.push({ wall_ms: observedAtMs - startedAtMs, ...sample });
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
    await sleep(500);
  }

  const endedAtMs = Date.now();
  const report = {
    ok,
    args,
    command: args.servoshell,
    command_args: commandArgs,
    webdriver_port: port,
    started_at_ms: startedAtMs,
    ended_at_ms: endedAtMs,
    summary: summarize(samples, startedAtMs, endedAtMs),
    samples,
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
