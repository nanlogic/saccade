#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const {
  ReflexLiveBridge,
  appendJsonl,
  sampleLocalGame,
  sleep,
  summarizeGameSamples,
  summarizeReceipts,
  summarizeReflexEvents,
  waitForLocalGameDebug,
} = require("./lib/reflex_live_bridge");

function usage() {
  console.error(`usage:
  node scripts/run_local_game_reflex_loop.js \\
    --servoshell /path/to/servoshell \\
    [--url http://127.0.0.1:4173/] [--headed] \\
    [--window-size 1280x900] [--duration-ms 15000] \\
    [--output-dir runs/local_game_reflex/<name>]
`);
}

function parseArgs(argv) {
  const args = {
    url: "http://127.0.0.1:4173/",
    durationMs: 15000,
    tickMs: 250,
    headless: true,
    windowSize: "1280x900",
    outputDir: path.join("runs", "local_game_reflex", `loop_${Date.now()}`),
  };

  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--servoshell") {
      args.servoshell = argv[++i];
    } else if (arg === "--url") {
      args.url = argv[++i];
    } else if (arg === "--duration-ms") {
      args.durationMs = Number(argv[++i]);
    } else if (arg === "--tick-ms") {
      args.tickMs = Number(argv[++i]);
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
  if (!Number.isFinite(args.tickMs) || args.tickMs <= 0) {
    throw new Error("--tick-ms must be positive");
  }

  return args;
}

function nowMs(startedAtMs) {
  return Date.now() - startedAtMs;
}

function summarizeObservation(sample) {
  const debug = sample.debug || {};
  return {
    mode: debug.mode || null,
    hp: debug.hp ?? null,
    fill: debug.fill ?? null,
    fillCap: debug.fillCap ?? null,
    blendCount: debug.blendCount ?? null,
    enemies: debug.enemies ?? null,
    smallEnemies: debug.smallEnemies ?? null,
    drops: debug.drops ?? null,
    hazards: debug.hazards ?? null,
    weapon: debug.weapon ?? null,
    boss: debug.boss ?? null,
    camera: debug.camera || null,
    time: debug.time ?? null,
    upgradeVisible: sample.upgrade?.visible || false,
    endVisible: sample.end?.visible || false,
  };
}

function chooseMotorCommand(sample, tickIndex, commandSeq) {
  const dpr = sample.dpr || 1;
  if (sample.upgrade?.visible && sample.upgrade.firstCard?.rect) {
    const rect = sample.upgrade.firstCard.rect;
    return {
      id: `upgrade-${commandSeq}`,
      type: "click",
      x: (rect.x + rect.w / 2) * dpr,
      y: (rect.y + rect.h / 2) * dpr,
      basis: "upgrade_first_card",
      label: sample.upgrade.firstCard.text,
    };
  }

  const canvas = sample.canvas;
  if (!canvas?.clientWidth || !canvas?.clientHeight) {
    return null;
  }

  const centerX = (canvas.rect?.x || 0) + canvas.clientWidth / 2;
  const centerY = (canvas.rect?.y || 0) + canvas.clientHeight / 2;
  const radius = Math.min(canvas.clientWidth, canvas.clientHeight) * 0.30;
  const angle = tickIndex * 0.72;
  const hp = sample.debug?.hp ?? 5;
  const panic = hp <= 2 ? Math.PI : 0;
  const endX = centerX + Math.cos(angle + panic) * radius;
  const endY = centerY + Math.sin(angle + panic) * radius * 0.72;

  return {
    id: `orbit-${commandSeq}`,
    type: "drag",
    start: { x: centerX * dpr, y: centerY * dpr },
    end: { x: endX * dpr, y: endY * dpr },
    frames: 10,
    basis: "orbit_policy_v0",
    angle,
    hp,
  };
}

function commandForBridge(command) {
  if (!command) {
    return null;
  }
  if (command.type === "click") {
    return {
      id: command.id,
      type: "click",
      x: command.x,
      y: command.y,
    };
  }
  return {
    id: command.id,
    type: "drag",
    start: command.start,
    end: command.end,
    frames: command.frames,
  };
}

async function writeReplay(replayPath, value) {
  await appendJsonl(replayPath, value);
}

async function appendNewReceipts(bridge, replayPath, seenReceiptCount, startedAtMs) {
  const receipts = await bridge.receipts();
  for (const receipt of receipts.slice(seenReceiptCount)) {
    await writeReplay(replayPath, {
      kind: "receipt_observed",
      t_ms: nowMs(startedAtMs),
      receipt,
    });
  }
  return receipts.length;
}

async function main() {
  const args = parseArgs(process.argv);
  await fs.mkdir(args.outputDir, { recursive: true });
  const replayPath = path.join(args.outputDir, "replay.jsonl");
  await fs.writeFile(replayPath, "");

  const bridge = new ReflexLiveBridge({
    servoshell: args.servoshell,
    url: args.url,
    headless: args.headless,
    windowSize: args.windowSize,
    outputDir: args.outputDir,
    observeMaxFrames: 1400,
  });

  const startedAtMs = Date.now();
  const samples = [];
  let seenReceiptCount = 0;
  let commandSeq = 0;
  let commandCount = 0;
  let ok = false;
  let finalReason = "not_finished";

  await writeReplay(replayPath, {
    kind: "run_started",
    t_ms: 0,
    url: args.url,
    controller: "local_game_debug_policy_v0",
    note: "Uses public canvas.dataset.debug for v0 policy; browser input still enters via ServoShell bridge.",
  });

  try {
    await bridge.start();
    await bridge.open(args.url);
    await waitForLocalGameDebug(bridge.port, bridge.sessionId, 15000);

    const deadline = Date.now() + args.durationMs;
    let tickIndex = 0;
    while (Date.now() <= deadline) {
      const sampleAtMs = nowMs(startedAtMs);
      const sample = await sampleLocalGame(bridge.port, bridge.sessionId);
      samples.push({ wall_ms: sampleAtMs, ...sample });
      const observation = summarizeObservation(sample);
      await writeReplay(replayPath, {
        kind: "detector_observation",
        t_ms: sampleAtMs,
        tick: tickIndex,
        observation,
      });

      const mode = sample.debug?.mode;
      if (mode === "gameover" || mode === "victory") {
        finalReason = mode;
        break;
      }

      const action = chooseMotorCommand(sample, tickIndex, commandSeq + 1);
      if (action) {
        commandSeq += 1;
        const bridgeCommand = commandForBridge(action);
        await bridge.appendCommand(bridgeCommand);
        commandCount += 1;
        await writeReplay(replayPath, {
          kind: "motor_action",
          t_ms: nowMs(startedAtMs),
          tick: tickIndex,
          action,
          bridge_command: bridgeCommand,
        });
      }

      seenReceiptCount = await appendNewReceipts(
        bridge,
        replayPath,
        seenReceiptCount,
        startedAtMs,
      );

      tickIndex += 1;
      await sleep(args.tickMs);
    }
    seenReceiptCount = await appendNewReceipts(bridge, replayPath, seenReceiptCount, startedAtMs);
    finalReason = finalReason === "not_finished" ? "duration_complete" : finalReason;
    ok = true;
  } finally {
    await bridge.stop();
  }

  const endedAtMs = Date.now();
  const receipts = await bridge.receipts();
  const frames = await bridge.frames();
  const sampleSummary = summarizeGameSamples(samples, startedAtMs, endedAtMs);
  const receiptSummary = summarizeReceipts(receipts);
  const frameSummary = summarizeReflexEvents(frames);
  const firstDebug = sampleSummary.first_debug || {};
  const lastDebug = sampleSummary.last_debug || {};
  const healthOk = (lastDebug.hp ?? 0) > 0 || ["victory", "upgrade", "blending"].includes(lastDebug.mode);
  const commandReceipts =
    (receiptSummary.by_type_status["drag:scheduled"] || 0) +
    (receiptSummary.by_type_status["click:dispatched"] || 0);
  const pass =
    ok &&
    sampleSummary.time_scale !== null &&
    sampleSummary.time_scale > 0.75 &&
    commandCount >= 8 &&
    commandReceipts >= 4 &&
    frameSummary.readback_ok > 0 &&
    healthOk;

  const report = {
    ok: pass,
    final_reason: finalReason,
    args,
    command: args.servoshell,
    command_args: bridge.commandArgs,
    webdriver_port: bridge.port,
    paths: {
      ...bridge.paths,
      replay: replayPath,
    },
    started_at_ms: startedAtMs,
    ended_at_ms: endedAtMs,
    summary: {
      samples: sampleSummary,
      receipts: receiptSummary,
      frames: frameSummary,
      command_count: commandCount,
      command_receipts: commandReceipts,
      hp_delta:
        typeof firstDebug.hp === "number" && typeof lastDebug.hp === "number"
          ? lastDebug.hp - firstDebug.hp
          : null,
      fill_delta:
        typeof firstDebug.fill === "number" && typeof lastDebug.fill === "number"
          ? lastDebug.fill - firstDebug.fill
          : null,
      blend_delta:
        typeof firstDebug.blendCount === "number" && typeof lastDebug.blendCount === "number"
          ? lastDebug.blendCount - firstDebug.blendCount
          : null,
    },
    samples,
    receipts,
    frames,
    process: bridge.processInfo(),
  };

  await writeReplay(replayPath, {
    kind: "run_finished",
    t_ms: nowMs(startedAtMs),
    ok: pass,
    final_reason: finalReason,
    summary: report.summary,
  });

  const reportPath = path.join(args.outputDir, "report.json");
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(
    JSON.stringify(
      {
        ok: pass,
        report: reportPath,
        replay: replayPath,
        final_reason: finalReason,
        summary: report.summary,
        stderr_tail: report.process.stderr_tail.split("\n").slice(-8).join("\n"),
      },
      null,
      2,
    ),
  );

  if (!pass) {
    process.exitCode = 1;
  }
}

main().catch(async (error) => {
  console.error(error.stack || error.message);
  usage();
  process.exit(1);
});
