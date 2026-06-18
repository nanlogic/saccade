const { spawn } = require("node:child_process");
const fs = require("node:fs/promises");
const net = require("node:net");
const path = require("node:path");

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

async function appendJsonl(filePath, value) {
  await fs.appendFile(filePath, `${JSON.stringify(value)}\n`);
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

function summarizeReflexEvents(events) {
  const frames = events.filter(
    (event) => event.kind === "saccade_reflex_frame" || "readback_ok" in event,
  );
  const okFrames = frames.filter((frame) => frame.readback_ok === true);
  const bridgeInputs = events.filter(
    (event) =>
      event.kind === "saccade_reflex_test_drag" ||
      event.kind === "saccade_reflex_test_click",
  );
  const readbackMs = okFrames
    .map((frame) => (typeof frame.readback_ns === "number" ? frame.readback_ns / 1_000_000 : null))
    .filter((value) => value !== null);
  const saturatedRatios = okFrames
    .map((frame) =>
      Number.isFinite(frame.sample_saturated) && Number.isFinite(frame.sample_count)
        ? frame.sample_saturated / Math.max(1, frame.sample_count)
        : null,
    )
    .filter((value) => value !== null);
  const maxChannelRange = Math.max(
    0,
    ...okFrames.map((frame) => frame.sample_max_channel_range || 0),
  );
  const maxLumaRange = Math.max(0, ...okFrames.map((frame) => frame.sample_luma_range || 0));
  const maxSaturatedRatio = Math.max(0, ...saturatedRatios);
  const foregroundPresent =
    maxChannelRange >= 80 && maxLumaRange >= 35 && maxSaturatedRatio >= 0.001;
  return {
    event_count: events.length,
    count: frames.length,
    readback_ok: okFrames.length,
    bridge_input_events: bridgeInputs.length,
    readback_ms: summarizeNumbers(readbackMs),
    max_channel_range: maxChannelRange,
    max_luma_range: maxLumaRange,
    max_saturated_ratio: Number(maxSaturatedRatio.toFixed(6)),
    foreground_present: foregroundPresent,
    foreground_route: foregroundPresent ? "readback_foreground_present" : "readback_blank_or_flat",
    foreground_thresholds: {
      max_channel_range_min: 80,
      max_luma_range_min: 35,
      max_saturated_ratio_min: 0.001,
    },
    saturated_ratio: summarizeNumbers(saturatedRatios),
    dropped_logs_max: Math.max(0, ...frames.map((frame) => frame.dropped_logs || 0)),
  };
}

function summarizeGameSamples(samples, startedAtMs, endedAtMs) {
  const debugSamples = samples.filter(
    (sample) => sample.debug && typeof sample.debug.time === "number",
  );
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

async function sampleLocalGame(port, sessionId) {
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
        dpr: window.devicePixelRatio || 1,
        hasCanvas: Boolean(canvas),
        canvas: canvas ? {
          width: canvas.width,
          height: canvas.height,
          clientWidth: canvas.clientWidth,
          clientHeight: canvas.clientHeight,
          rect: (() => {
            const rect = canvas.getBoundingClientRect();
            return { x: rect.x, y: rect.y, w: rect.width, h: rect.height };
          })()
        } : null,
        upgrade: (() => {
          const panel = document.getElementById("upgradePanel");
          const cards = [...document.querySelectorAll("#upgradeCards .upgrade-card")];
          const visible = Boolean(panel && !panel.classList.contains("hidden"));
          return {
            visible,
            count: cards.length,
            firstCard: cards[0] ? (() => {
              const rect = cards[0].getBoundingClientRect();
              return {
                text: cards[0].textContent.trim().replace(/\\s+/g, " ").slice(0, 120),
                rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height }
              };
            })() : null
          };
        })(),
        end: (() => {
          const panel = document.getElementById("endScreen");
          return {
            visible: Boolean(panel && !panel.classList.contains("hidden")),
            title: document.getElementById("endTitle")?.textContent || null,
            detail: document.getElementById("endDetail")?.textContent || null
          };
        })(),
        debug
      };
    })();`,
  );
}

async function waitForLocalGameDebug(port, sessionId, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await sampleLocalGame(port, sessionId);
    if (last?.hasCanvas && typeof last.debug?.time === "number") {
      return last;
    }
    await sleep(250);
  }
  throw new Error(`game debug did not become ready: ${JSON.stringify(last)}`);
}

class ReflexLiveBridge {
  constructor(options) {
    this.servoshell = options.servoshell;
    this.url = options.url;
    this.headless = options.headless ?? true;
    this.windowSize = options.windowSize || "1280x900";
    this.outputDir = options.outputDir || path.join("runs", "reflex_live", `live_${Date.now()}`);
    this.observeMaxFrames = options.observeMaxFrames || 420;
    this.extraEnv = options.env || {};
    this.port = null;
    this.child = null;
    this.sessionId = null;
    this.stdout = "";
    this.stderr = "";
    this.commandArgs = [];
    this.paths = {
      commands: path.join(this.outputDir, "commands.jsonl"),
      receipts: path.join(this.outputDir, "receipts.jsonl"),
      frames: path.join(this.outputDir, "frames.jsonl"),
    };
  }

  async start() {
    if (!this.servoshell) {
      throw new Error("servoshell is required");
    }

    this.port = await chooseLoopbackPort();
    await fs.mkdir(this.outputDir, { recursive: true });
    await fs.writeFile(this.paths.commands, "");

    this.commandArgs = [
      `--webdriver=${this.port}`,
      "--temporary-storage",
      `--window-size=${this.windowSize}`,
    ];
    if (this.headless) {
      this.commandArgs.unshift("-z");
    }
    if (this.url) {
      this.commandArgs.push(this.url);
    }

    this.child = spawn(this.servoshell, this.commandArgs, {
      env: {
        ...process.env,
        ...this.extraEnv,
        SACCADE_REFLEX_COMMANDS_PATH: this.paths.commands,
        SACCADE_REFLEX_RECEIPTS_PATH: this.paths.receipts,
        SACCADE_REFLEX_OBSERVE_PATH: this.paths.frames,
        SACCADE_REFLEX_OBSERVE_MAX_FRAMES: String(this.observeMaxFrames),
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    this.child.stdout.on("data", (chunk) => {
      this.stdout += chunk.toString();
    });
    this.child.stderr.on("data", (chunk) => {
      this.stderr += chunk.toString();
    });

    await waitForStatus(this.port, this.child, 25000);
  }

  async open(url = this.url) {
    this.sessionId = await newSession(this.port);
    if (url) {
      await navigate(this.port, this.sessionId, url);
    }
    return this.sessionId;
  }

  async execute(script) {
    if (!this.sessionId) {
      throw new Error("session not open");
    }
    return await execute(this.port, this.sessionId, script);
  }

  async appendCommand(command) {
    await appendJsonl(this.paths.commands, command);
  }

  async receipts() {
    return await readJsonl(this.paths.receipts);
  }

  async frames() {
    return await readJsonl(this.paths.frames);
  }

  async stop() {
    if (this.sessionId) {
      try {
        await request(this.port, "DELETE", `/session/${this.sessionId}`);
      } catch {}
      this.sessionId = null;
    }
    if (this.child) {
      this.child.kill("SIGTERM");
      await sleep(750);
    }
  }

  processInfo() {
    return {
      exit_code: this.child?.exitCode ?? null,
      signal_code: this.child?.signalCode ?? null,
      stdout_tail: this.stdout.split("\n").slice(-20).join("\n"),
      stderr_tail: this.stderr.split("\n").slice(-80).join("\n"),
    };
  }
}

module.exports = {
  ReflexLiveBridge,
  appendJsonl,
  chooseLoopbackPort,
  execute,
  navigate,
  newSession,
  readJsonl,
  request,
  sampleLocalGame,
  sleep,
  summarizeGameSamples,
  summarizeNumbers,
  summarizeReceipts,
  summarizeReflexEvents,
  waitForLocalGameDebug,
  waitForStatus,
};
