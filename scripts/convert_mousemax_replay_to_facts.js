#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const {
  browserFactFromMousemaxTarget,
  summarizeFacts,
  writeFactsJsonl,
} = require("./lib/browser_fact_stream");
const { readJsonl } = require("./lib/reflex_live_bridge");

function usage() {
  console.error(`usage:
  node scripts/convert_mousemax_replay_to_facts.js \\
    --replay runs/arena/run_<id>/replay.jsonl \\
    [--mode appeared|frames|both] \\
    [--output-dir runs/browser_fact_stream/mousemax_<name>]
`);
}

function parseArgs(argv) {
  const args = {
    mode: "appeared",
    includeOutsideGameArea: false,
    outputDir: path.join("runs", "browser_fact_stream", `mousemax_${Date.now()}`),
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--replay") {
      args.replay = argv[++i];
    } else if (arg === "--mode") {
      args.mode = argv[++i];
    } else if (arg === "--include-outside-game-area") {
      args.includeOutsideGameArea = true;
    } else if (arg === "--output-dir") {
      args.outputDir = argv[++i];
    } else if (arg === "--help" || arg === "-h") {
      usage();
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!args.replay) {
    throw new Error("--replay is required");
  }
  if (!["appeared", "frames", "both"].includes(args.mode)) {
    throw new Error("--mode must be appeared, frames, or both");
  }
  return args;
}

function runContext(events, replayPath) {
  const started = events.find((event) => event.kind === "run_started") || {};
  const finished = [...events].reverse().find((event) => event.kind === "run_finished") || {};
  const config = started.config || {};
  const result = finished.result || null;
  return {
    replay_path: replayPath,
    run_id: started.run_id || path.basename(path.dirname(replayPath)),
    url: config.url || null,
    title: "MOUSEMAX replay",
    config,
    result,
  };
}

function contains(rect, point) {
  if (!rect || !point) return true;
  return (
    point.x >= rect.x &&
    point.y >= rect.y &&
    point.x < rect.x + rect.w &&
    point.y < rect.y + rect.h
  );
}

function gameAreasByFrame(events) {
  const areas = new Map();
  for (const event of events) {
    if (event.kind === "frame_report" && event.report?.game_area_css) {
      areas.set(event.report.frame_id, event.report.game_area_css);
    }
  }
  return areas;
}

function appearedTargets(events, args) {
  const areas = gameAreasByFrame(events);
  const out = [];
  const skipped = [];
  for (const event of events) {
    const target = event.event?.Appeared?.target;
    if (event.kind === "tracker_event" && target) {
      const gameArea = areas.get(target.frame_id) || null;
      if (!args.includeOutsideGameArea && gameArea && !contains(gameArea, target.center_css)) {
        skipped.push({ target, reason: "outside_game_area", game_area_css: gameArea });
        continue;
      }
      out.push({ target, reason: "tracker_appeared", game_area_css: gameArea });
    }
  }
  return { out, skipped };
}

function frameTargets(events) {
  const out = [];
  for (const event of events) {
    if (event.kind !== "frame_report" || !event.report?.targets?.length) continue;
    for (const target of event.report.targets) {
      out.push({
        target,
        reason: "frame_report_target",
        game_area_css: event.report.game_area_css || null,
      });
    }
  }
  return out;
}

function convertEventsToFacts(events, args) {
  const context = runContext(events, args.replay);
  const records = [];
  let skipped = [];
  if (args.mode === "appeared" || args.mode === "both") {
    const appeared = appearedTargets(events, args);
    records.push(...appeared.out);
    skipped.push(...appeared.skipped);
  }
  if (args.mode === "frames" || args.mode === "both") records.push(...frameTargets(events));

  return {
    context,
    skipped,
    facts: records.map((record, index) =>
      browserFactFromMousemaxTarget(record.target, {
        seq: index + 1,
        url: context.url,
        title: context.title,
        reason: record.reason,
        game_area_css: record.game_area_css,
      }),
    ),
  };
}

function sourceCounts(facts) {
  const byDetectorSource = {};
  for (const fact of facts) {
    const source = fact.visual_object?.detector_source || "unknown";
    byDetectorSource[source] = (byDetectorSource[source] || 0) + 1;
  }
  return byDetectorSource;
}

async function main() {
  const args = parseArgs(process.argv);
  await fs.mkdir(args.outputDir, { recursive: true });
  const factsPath = path.join(args.outputDir, "facts.jsonl");
  const reportPath = path.join(args.outputDir, "report.json");
  await fs.writeFile(factsPath, "");

  const events = await readJsonl(args.replay);
  const { context, facts, skipped } = convertEventsToFacts(events, args);
  await writeFactsJsonl(factsPath, facts);

  const summary = summarizeFacts(facts);
  const report = {
    ok: facts.length > 0,
    args,
    context,
    facts_path: factsPath,
    summary,
    detector_sources: sourceCounts(facts),
    checks: {
      has_visual_object: facts.some((fact) => fact.fact_type === "visual_object_seen"),
      facts_match_result_targets_seen:
        typeof context.result?.result?.targets_seen === "number"
          ? facts.length === context.result.result.targets_seen
          : null,
      skipped_outside_game_area: skipped.length,
    },
    skipped_samples: skipped.slice(0, 8),
    samples: facts.slice(0, 8),
  };
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(
    JSON.stringify(
      {
        ok: report.ok,
        report: reportPath,
        facts: factsPath,
        summary,
        detector_sources: report.detector_sources,
        checks: report.checks,
      },
      null,
      2,
    ),
  );
  if (!report.ok) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error.stack || error.message);
  usage();
  process.exit(1);
});
