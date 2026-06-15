#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const {
  drainBrowserFacts,
  installBrowserFactStream,
  sampleBrowserVisualObjects,
  summarizeFacts,
  writeFactsJsonl,
} = require("./lib/browser_fact_stream");
const {
  classifyLocalGameFacts,
  summarizeSemanticFacts,
} = require("./lib/local_game_fact_classifier");
const {
  buildReviewForRunDir,
} = require("./build_local_game_reflex_review");
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
    [--policy visual|orbit] \\
    [--no-browser-facts] [--visual-fact-interval-ms 1000] \\
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
    policy: "visual",
    browserFacts: true,
    visualFactIntervalMs: 1000,
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
    } else if (arg === "--policy") {
      args.policy = argv[++i];
    } else if (arg === "--no-browser-facts") {
      args.browserFacts = false;
    } else if (arg === "--visual-fact-interval-ms") {
      args.visualFactIntervalMs = Number(argv[++i]);
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
  if (!Number.isFinite(args.visualFactIntervalMs) || args.visualFactIntervalMs <= 0) {
    throw new Error("--visual-fact-interval-ms must be positive");
  }
  if (!["visual", "orbit"].includes(args.policy)) {
    throw new Error("--policy must be visual or orbit");
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

function distance(a, b) {
  return Math.hypot((a?.x || 0) - (b?.x || 0), (a?.y || 0) - (b?.y || 0));
}

function clamp(value, min, max) {
  return Math.max(min, Math.min(max, value));
}

function canvasRect(sample = {}) {
  const rect = sample.canvas?.rect || {};
  return {
    x: Number(rect.x || 0),
    y: Number(rect.y || 0),
    w: Number(rect.w || sample.canvas?.clientWidth || 0),
    h: Number(rect.h || sample.canvas?.clientHeight || 0),
  };
}

function insideRect(point, rect, pad = 0) {
  return (
    point &&
    point.x >= rect.x + pad &&
    point.x <= rect.x + rect.w - pad &&
    point.y >= rect.y + pad &&
    point.y <= rect.y + rect.h - pad
  );
}

function clampPointToRect(point, rect, pad = 18) {
  return {
    x: clamp(point.x, rect.x + pad, rect.x + rect.w - pad),
    y: clamp(point.y, rect.y + pad, rect.y + rect.h - pad),
  };
}

function objectCenter(object) {
  return object?.visual_object?.center_css || object?.visual_object?.bbox_css || null;
}

function createLiveVisualState() {
  return {
    objects: [],
    latestReceivedAtMs: null,
  };
}

function semanticObjectForState(fact, receivedAtMs) {
  if (fact?.fact_type !== "semantic_object_seen" || !fact.visual_object || !fact.semantic) {
    return null;
  }
  const center = objectCenter(fact);
  const bbox = fact.visual_object.bbox_css || {};
  if (!center || !Number.isFinite(center.x) || !Number.isFinite(center.y)) {
    return null;
  }
  return {
    sourceSeq: fact.source_fact_seq ?? fact.seq ?? null,
    sourceObjectId: fact.source_object_id || null,
    receivedAtMs,
    tMs: fact.t_ms ?? null,
    center,
    bbox,
    area: Number(fact.visual_object.area_px || 0),
    rgba: fact.visual_object.avg_rgba || null,
    label: fact.semantic.label || "unknown_visual_object",
    role: fact.semantic.role?.name || "unknown",
    roleConfidence: fact.semantic.role?.confidence || 0,
    palette: fact.semantic.palette?.name || "unknown",
    paletteConfidence: fact.semantic.palette?.confidence || 0,
    shape: fact.semantic.shape || {},
  };
}

function updateLiveVisualState(state, semanticFacts, receivedAtMs) {
  const incoming = semanticFacts
    .map((fact) => semanticObjectForState(fact, receivedAtMs))
    .filter(Boolean);
  if (!incoming.length) {
    state.objects = state.objects.filter((object) => receivedAtMs - object.receivedAtMs <= 2200);
    return state;
  }

  state.latestReceivedAtMs = receivedAtMs;
  state.objects.push(...incoming);
  state.objects = state.objects
    .filter((object) => receivedAtMs - object.receivedAtMs <= 2200)
    .sort((a, b) => b.receivedAtMs - a.receivedAtMs || b.area - a.area)
    .slice(0, 140);
  return state;
}

function summarizeLiveVisualState(state, sample) {
  const rect = sample ? canvasRect(sample) : null;
  const objects = rect ? state.objects.filter((object) => insideRect(object.center, rect)) : state.objects;
  const byRole = {};
  for (const object of objects) byRole[object.role] = (byRole[object.role] || 0) + 1;
  return {
    object_count: objects.length,
    latest_received_at_ms: state.latestReceivedAtMs,
    by_role: byRole,
    player: findPlayerAnchor(state, sample),
    candidate_drops: candidateDrops(state, sample).slice(0, 5).map((object) => ({
      center: object.center,
      role: object.role,
      palette: object.palette,
      area: object.area,
      score: Math.round((object.score || 0) * 10) / 10,
      clusterCount: object.clusterCount || 0,
      persistenceCount: object.persistenceCount || 0,
    })),
    dangers: dangerObjects(state, sample).slice(0, 5).map((object) => ({
      center: object.center,
      role: object.role,
      palette: object.palette,
      area: object.area,
      radius: Math.round((object.dangerRadius || 0) * 10) / 10,
    })),
  };
}

function findPlayerAnchor(state, sample) {
  const rect = canvasRect(sample);
  if (!rect.w || !rect.h) return null;
  const fallback = { x: rect.x + rect.w / 2, y: rect.y + rect.h / 2 };
  const centerBias = fallback;
  const candidates = state.objects
    .filter((object) => object.role === "player" && insideRect(object.center, rect, 4))
    .map((object) => ({
      ...object,
      anchorScore:
        object.roleConfidence * 1000 -
        distance(object.center, centerBias) * 1.5 +
        object.receivedAtMs * 0.001,
    }))
    .sort((a, b) => b.anchorScore - a.anchorScore);
  return candidates[0]
    ? {
        x: candidates[0].center.x,
        y: candidates[0].center.y,
        confidence: candidates[0].roleConfidence,
        source: "semantic_player_anchor",
      }
    : { ...fallback, confidence: 0.35, source: "canvas_center_fallback" };
}

function dangerObjects(state, sample) {
  const rect = canvasRect(sample);
  if (!rect.w || !rect.h) return [];
  return state.objects
    .filter((object) => ["enemy", "hazard"].includes(object.role))
    .filter((object) => insideRect(object.center, rect, 2))
    .map((object) => {
      const maxDim = Number(object.shape.maxDim || Math.max(object.bbox.w || 0, object.bbox.h || 0));
      const dangerRadius = object.role === "hazard" ? Math.max(95, maxDim * 1.5 + 45) : Math.max(72, maxDim * 1.65 + 38);
      return { ...object, dangerRadius };
    })
    .sort((a, b) => b.area - a.area);
}

function candidateDrops(state, sample) {
  const rect = canvasRect(sample);
  if (!rect.w || !rect.h) return [];
  const player = findPlayerAnchor(state, sample);
  if (!player) return [];
  const dangers = dangerObjects(state, sample);
  const now = state.latestReceivedAtMs || 0;
  const pickupObjects = state.objects.filter((object) =>
    ["drop", "projectile_or_particle"].includes(object.role),
  );
  return pickupObjects
    .filter((object) => ["drop", "projectile_or_particle"].includes(object.role))
    .filter((object) => insideRect(object.center, rect, 10))
    .filter((object) => object.center.y > rect.y + 78)
    .filter((object) => distance(object.center, player) > 92)
    .filter((object) => now - object.receivedAtMs <= 1800)
    .map((object) => {
      const distToPlayer = distance(object.center, player);
      const clusterCount = pickupObjects.filter(
        (other) =>
          other !== object &&
          other.role === "drop" &&
          now - other.receivedAtMs <= 1800 &&
          distance(other.center, object.center) <= 72,
      ).length;
      const persistenceCount = pickupObjects.filter(
        (other) =>
          other !== object &&
          other.role === object.role &&
          Math.abs(other.receivedAtMs - object.receivedAtMs) >= 450 &&
          distance(other.center, object.center) <= 34,
      ).length;
      let dangerPenalty = 0;
      let nearestDanger = Infinity;
      for (const danger of dangers) {
        const d = distance(object.center, danger.center);
        nearestDanger = Math.min(nearestDanger, d);
        if (d < danger.dangerRadius) dangerPenalty += (danger.dangerRadius - d) * 4;
      }
      const edgePenalty =
        object.center.x < rect.x + 36 ||
        object.center.x > rect.x + rect.w - 36 ||
        object.center.y < rect.y + 96 ||
        object.center.y > rect.y + rect.h - 36
          ? 45
          : 0;
      const rolePenalty = object.role === "projectile_or_particle" ? 85 : 0;
      const nearPlayerWeaponPenalty = distToPlayer < 150 ? 90 : 0;
      const highLanePenalty = object.center.y < player.y - 125 ? 75 : 0;
      const clusterBonus = Math.min(110, clusterCount * 28);
      const persistenceBonus = Math.min(80, persistenceCount * 40);
      const freshnessPenalty = (now - object.receivedAtMs) * 0.03;
      const score =
        distToPlayer +
        dangerPenalty +
        edgePenalty +
        rolePenalty +
        nearPlayerWeaponPenalty +
        highLanePenalty +
        freshnessPenalty -
        clusterBonus -
        persistenceBonus;
      return { ...object, score, nearestDanger, clusterCount, persistenceCount };
    })
    .sort((a, b) => a.score - b.score);
}

function vectorToSafeTarget(player, target, dangers) {
  let vx = target ? target.center.x - player.x : 0;
  let vy = target ? target.center.y - player.y : 0;
  if (target) {
    const len = Math.hypot(vx, vy) || 1;
    vx /= len;
    vy /= len;
  }

  for (const danger of dangers) {
    const dx = player.x - danger.center.x;
    const dy = player.y - danger.center.y;
    const d = Math.hypot(dx, dy) || 1;
    const influence = Math.max(0, (danger.dangerRadius + 70 - d) / (danger.dangerRadius + 70));
    if (influence <= 0) continue;
    vx += (dx / d) * influence * (danger.role === "hazard" ? 2.8 : 2.1);
    vy += (dy / d) * influence * (danger.role === "hazard" ? 2.8 : 2.1);
  }

  const len = Math.hypot(vx, vy);
  if (len < 0.001) return null;
  return { x: vx / len, y: vy / len };
}

function chooseOrbitMotorCommand(sample, tickIndex, commandSeq) {
  const dpr = sample.dpr || 1;
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

function chooseVisualMotorCommand(sample, tickIndex, commandSeq, liveVisualState) {
  const dpr = sample.dpr || 1;
  const rect = canvasRect(sample);
  if (!rect.w || !rect.h) return null;

  const player = findPlayerAnchor(liveVisualState, sample);
  const drops = candidateDrops(liveVisualState, sample);
  const dangers = dangerObjects(liveVisualState, sample);
  const target = drops[0] || null;
  const dir = vectorToSafeTarget(player, target, dangers);
  if (!dir) return chooseOrbitMotorCommand(sample, tickIndex, commandSeq);

  const targetDistance = target ? distance(player, target.center) : Infinity;
  const step = target ? clamp(targetDistance + 60, 120, 270) : Math.min(rect.w, rect.h) * 0.22;
  const end = clampPointToRect(
    {
      x: player.x + dir.x * step,
      y: player.y + dir.y * step,
    },
    rect,
    18,
  );
  const start = clampPointToRect(player, rect, 18);

  return {
    id: `visual-${commandSeq}`,
    type: "drag",
    start: { x: start.x * dpr, y: start.y * dpr },
    end: { x: end.x * dpr, y: end.y * dpr },
    frames: 8,
    basis: "visual_facts_pickup_policy_v0",
    player,
    target: target
      ? {
          role: target.role,
          palette: target.palette,
          center: target.center,
          area: target.area,
          score: Math.round(target.score * 10) / 10,
          clusterCount: target.clusterCount,
          persistenceCount: target.persistenceCount,
          nearestDanger: Number.isFinite(target.nearestDanger)
            ? Math.round(target.nearestDanger * 10) / 10
            : null,
        }
      : null,
    dangers: dangers.slice(0, 4).map((danger) => ({
      role: danger.role,
      palette: danger.palette,
      center: danger.center,
      radius: Math.round(danger.dangerRadius * 10) / 10,
    })),
  };
}

function chooseMotorCommand(sample, tickIndex, commandSeq, options = {}) {
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

  if (options.policy === "visual" && options.liveVisualState) {
    return chooseVisualMotorCommand(sample, tickIndex, commandSeq, options.liveVisualState);
  }
  return chooseOrbitMotorCommand(sample, tickIndex, commandSeq);
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

async function drainAndRecordBrowserFacts({
  bridge,
  factsPath,
  replayPath,
  startedAtMs,
  allFacts,
  reason,
}) {
  const facts = await drainBrowserFacts(bridge, 1000);
  if (!facts.length) {
    return [];
  }
  allFacts.push(...facts);
  await writeFactsJsonl(factsPath, facts);
  await writeReplay(replayPath, {
    kind: "browser_facts_observed",
    t_ms: nowMs(startedAtMs),
    reason,
    summary: summarizeFacts(facts),
    samples: facts.slice(0, 5),
  });
  return facts;
}

async function main() {
  const args = parseArgs(process.argv);
  await fs.mkdir(args.outputDir, { recursive: true });
  const replayPath = path.join(args.outputDir, "replay.jsonl");
  const factsPath = path.join(args.outputDir, "facts.jsonl");
  const semanticFactsPath = path.join(args.outputDir, "semantic_facts.jsonl");
  const semanticReportPath = path.join(args.outputDir, "semantic_report.json");
  const reviewPath = path.join(args.outputDir, "review.html");
  await fs.writeFile(replayPath, "");
  await fs.writeFile(factsPath, "");
  await fs.writeFile(semanticFactsPath, "");

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
  const browserFacts = [];
  const liveVisualState = createLiveVisualState();
  let seenReceiptCount = 0;
  let commandSeq = 0;
  let commandCount = 0;
  let ok = false;
  let finalReason = "not_finished";
  let nextVisualFactAtMs = 0;

  await writeReplay(replayPath, {
    kind: "run_started",
    t_ms: 0,
    url: args.url,
    controller: args.policy === "visual" ? "local_game_visual_fact_policy_v0" : "local_game_debug_policy_v0",
    note:
      args.policy === "visual"
        ? "Uses live browser visual facts for movement policy; public canvas.dataset.debug is retained for scoring only."
        : "Uses public canvas.dataset.debug for v0 policy; browser input still enters via ServoShell bridge.",
  });

  try {
    await bridge.start();
    await bridge.open(args.url);
    await waitForLocalGameDebug(bridge.port, bridge.sessionId, 15000);
    if (args.browserFacts) {
      await installBrowserFactStream(bridge, {
        allowCanvasDebugValues: false,
        allowCanvasPixelRead: true,
        canvasMaxSamplePixels: 90000,
        textLimit: 120,
        visualColorDistanceThreshold: 64,
        visualMinAreaPx: 40,
        visualMaxObjectsPerCanvas: 32,
      });
      await sleep(100);
      const initialFacts = await drainAndRecordBrowserFacts({
        bridge,
        factsPath,
        replayPath,
        startedAtMs,
        allFacts: browserFacts,
        reason: "initial_browser_facts",
      });
      updateLiveVisualState(
        liveVisualState,
        classifyLocalGameFacts(initialFacts),
        nowMs(startedAtMs),
      );
    }

    const deadline = Date.now() + args.durationMs;
    let tickIndex = 0;
    while (Date.now() <= deadline) {
      const sampleAtMs = nowMs(startedAtMs);
      if (args.browserFacts && sampleAtMs >= nextVisualFactAtMs) {
        await sampleBrowserVisualObjects(bridge, "local_game_visual_sample");
        nextVisualFactAtMs = sampleAtMs + args.visualFactIntervalMs;
      }
      const sample = await sampleLocalGame(bridge.port, bridge.sessionId);
      samples.push({ wall_ms: sampleAtMs, ...sample });
      const observation = summarizeObservation(sample);
      await writeReplay(replayPath, {
        kind: "detector_observation",
        t_ms: sampleAtMs,
        tick: tickIndex,
        observation,
      });

      if (args.browserFacts) {
        const preActionFacts = await drainAndRecordBrowserFacts({
          bridge,
          factsPath,
          replayPath,
          startedAtMs,
          allFacts: browserFacts,
          reason: "pre_action_drain",
        });
        const semanticFacts = classifyLocalGameFacts(preActionFacts);
        updateLiveVisualState(liveVisualState, semanticFacts, nowMs(startedAtMs));
        if (semanticFacts.length) {
          await writeReplay(replayPath, {
            kind: "semantic_observation",
            t_ms: nowMs(startedAtMs),
            tick: tickIndex,
            summary: summarizeSemanticFacts(semanticFacts),
            live_visual_state: summarizeLiveVisualState(liveVisualState, sample),
          });
        }
      }

      const mode = sample.debug?.mode;
      if (mode === "gameover" || mode === "victory") {
        finalReason = mode;
        break;
      }

      const action = chooseMotorCommand(sample, tickIndex, commandSeq + 1, {
        policy: args.policy,
        liveVisualState,
      });
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
      if (args.browserFacts) {
        const postActionFacts = await drainAndRecordBrowserFacts({
          bridge,
          factsPath,
          replayPath,
          startedAtMs,
          allFacts: browserFacts,
          reason: "post_action_drain",
        });
        updateLiveVisualState(
          liveVisualState,
          classifyLocalGameFacts(postActionFacts),
          nowMs(startedAtMs),
        );
      }

      tickIndex += 1;
      await sleep(args.tickMs);
    }
    seenReceiptCount = await appendNewReceipts(bridge, replayPath, seenReceiptCount, startedAtMs);
    if (args.browserFacts) {
      await drainAndRecordBrowserFacts({
        bridge,
        factsPath,
        replayPath,
        startedAtMs,
        allFacts: browserFacts,
        reason: "final_drain",
      });
    }
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
  const browserFactSummary = summarizeFacts(browserFacts);
  const semanticFacts = args.browserFacts ? classifyLocalGameFacts(browserFacts) : [];
  const semanticFactSummary = summarizeSemanticFacts(semanticFacts);
  await writeFactsJsonl(semanticFactsPath, semanticFacts);
  await fs.writeFile(
    semanticReportPath,
    JSON.stringify(
      {
        ok: semanticFacts.length > 0,
        semantic_facts_path: semanticFactsPath,
        summary: semanticFactSummary,
        samples: semanticFacts.slice(0, 12),
      },
      null,
      2,
    ),
  );
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
      facts: factsPath,
      semantic_facts: semanticFactsPath,
      semantic_report: semanticReportPath,
      review: reviewPath,
    },
    started_at_ms: startedAtMs,
    ended_at_ms: endedAtMs,
    summary: {
      samples: sampleSummary,
      receipts: receiptSummary,
      frames: frameSummary,
      browser_facts: browserFactSummary,
      semantic_facts: semanticFactSummary,
      live_visual_state: summarizeLiveVisualState(liveVisualState, samples[samples.length - 1]),
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
      drop_delta:
        typeof firstDebug.drops === "number" && typeof lastDebug.drops === "number"
          ? lastDebug.drops - firstDebug.drops
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
  const generatedReviewPath = await buildReviewForRunDir(args.outputDir, reviewPath);
  console.log(
    JSON.stringify(
      {
        ok: pass,
        report: reportPath,
        review: generatedReviewPath,
        replay: replayPath,
        facts: factsPath,
        semantic_facts: semanticFactsPath,
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
