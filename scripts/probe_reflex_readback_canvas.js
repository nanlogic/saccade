#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const { pathToFileURL } = require("node:url");
const {
  ReflexLiveBridge,
  sleep,
  summarizeNumbers,
} = require("./lib/reflex_live_bridge");

function usage() {
  console.error(`usage:
  node scripts/probe_reflex_readback_canvas.js \\
    --servoshell /path/to/servoshell \\
    [--variant bare-gradient2-size-1152x648] \\
    [--duration-ms 2500] [--window-size 1440x900] [--headed] \\
    [--output-dir runs/webgl_runtime/reflex_readback_canvas_<ts>]
`);
}

function parseArgs(argv) {
  const args = {
    variant: "bare-gradient2-size-1152x648",
    durationMs: 2500,
    windowSize: "1440x900",
    headless: true,
    outputDir: path.join(
      "runs",
      "webgl_runtime",
      `reflex_readback_canvas_${Date.now()}`,
    ),
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--servoshell") {
      args.servoshell = argv[++i];
    } else if (arg === "--variant") {
      args.variant = argv[++i];
    } else if (arg === "--duration-ms") {
      args.durationMs = Number(argv[++i]);
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
  return args;
}

function canvasFixtureUrl(variant) {
  const fixture = path.resolve(__dirname, "..", "test_pages", "canvas_runtime", "index.html");
  const url = pathToFileURL(fixture);
  url.searchParams.set("variant", variant);
  return url.href;
}

async function waitForRuntime(bridge, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await bridge.execute(`return window.__saccadeCanvasRuntime || null;`);
    if (last?.canvas2d === "ok" && Number(last.frame || 0) > 0) {
      return last;
    }
    await sleep(120);
  }
  throw new Error(`canvas runtime did not become ready: ${JSON.stringify(last)}`);
}

function summarizeFrames(frames) {
  const frameEvents = frames.filter((event) => event.kind === "saccade_reflex_frame");
  const okFrames = frameEvents.filter((frame) => frame.readback_ok === true);
  const saturatedRatios = okFrames
    .map((frame) =>
      Number.isFinite(frame.sample_saturated) && Number.isFinite(frame.sample_count)
        ? frame.sample_saturated / Math.max(1, frame.sample_count)
        : null,
    )
    .filter((value) => value !== null);
  const maxChannelRange = Math.max(
    0,
    ...okFrames.map((frame) => Number(frame.sample_max_channel_range || 0)),
  );
  const maxLumaRange = Math.max(0, ...okFrames.map((frame) => Number(frame.sample_luma_range || 0)));
  const maxSaturatedRatio = Math.max(0, ...saturatedRatios);
  const readbackMs = okFrames
    .map((frame) =>
      typeof frame.readback_ns === "number" ? frame.readback_ns / 1_000_000 : null,
    )
    .filter((value) => value !== null);
  const foregroundPresent =
    maxChannelRange >= 80 && maxLumaRange >= 35 && maxSaturatedRatio >= 0.001;
  return {
    event_count: frames.length,
    frame_count: frameEvents.length,
    readback_ok: okFrames.length,
    readback_ms: summarizeNumbers(readbackMs),
    max_channel_range: maxChannelRange,
    max_luma_range: maxLumaRange,
    max_saturated_ratio: Number(maxSaturatedRatio.toFixed(6)),
    foreground_present: foregroundPresent,
    route: foregroundPresent ? "readback_foreground_present" : "readback_blank_or_flat",
  };
}

async function main() {
  const args = parseArgs(process.argv);
  const outputDir = path.resolve(args.outputDir);
  await fs.mkdir(outputDir, { recursive: true });
  const url = canvasFixtureUrl(args.variant);
  const bridge = new ReflexLiveBridge({
    servoshell: args.servoshell,
    url,
    headless: args.headless,
    windowSize: args.windowSize,
    outputDir,
    observeMaxFrames: 240,
  });

  let report;
  try {
    await bridge.start();
    await bridge.open(url);
    const runtime = await waitForRuntime(bridge, 10000);
    await sleep(args.durationMs);
    const frames = await bridge.frames();
    const summary = summarizeFrames(frames);
    report = {
      ok: summary.foreground_present,
      engine: "saccade-reflex-readback-canvas-v0",
      url,
      args,
      runtime,
      summary,
      paths: {
        ...bridge.paths,
        report: path.join(outputDir, "report.json"),
      },
      process: bridge.processInfo(),
    };
  } finally {
    await bridge.stop().catch(() => {});
  }

  await fs.writeFile(path.join(outputDir, "report.json"), JSON.stringify(report, null, 2) + "\n");
  console.log(
    "REFLEX_READBACK_CANVAS " +
      `route=${report.summary.route} ` +
      `ok=${report.ok} ` +
      `frames=${report.summary.frame_count} ` +
      `readback_ok=${report.summary.readback_ok} ` +
      `max_channel_range=${report.summary.max_channel_range} ` +
      `max_luma_range=${report.summary.max_luma_range} ` +
      `max_saturated_ratio=${report.summary.max_saturated_ratio} ` +
      `report=${report.paths.report}`,
  );
  process.exit(report.ok ? 0 : 1);
}

main().catch((error) => {
  console.error(error && error.stack ? error.stack : String(error));
  process.exit(1);
});
