#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");

function usage() {
  console.error(`usage:
  node scripts/build_local_game_reflex_review.js \\
    --run-dir runs/local_game_reflex/<run_id> [--output review.html]
`);
}

function parseArgs(argv) {
  const args = {
    output: null,
  };
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--run-dir") {
      args.runDir = argv[++i];
    } else if (arg === "--output") {
      args.output = argv[++i];
    } else if (arg === "--help" || arg === "-h") {
      usage();
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!args.runDir) throw new Error("--run-dir is required");
  return args;
}

async function readJson(file) {
  return JSON.parse(await fs.readFile(file, "utf8"));
}

async function readJsonl(file, limit = Infinity) {
  let text;
  try {
    text = await fs.readFile(file, "utf8");
  } catch (error) {
    if (error.code === "ENOENT") return [];
    throw error;
  }
  const rows = [];
  for (const line of text.split(/\r?\n/)) {
    const trimmed = line.trim();
    if (!trimmed) continue;
    rows.push(JSON.parse(trimmed));
    if (rows.length >= limit) break;
  }
  return rows;
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function fmt(value, digits = 3) {
  if (value === null || value === undefined) return "n/a";
  if (typeof value === "number") {
    if (!Number.isFinite(value)) return "n/a";
    return Number.isInteger(value) ? String(value) : value.toFixed(digits).replace(/\.?0+$/, "");
  }
  return String(value);
}

function pct(value) {
  if (value === null || value === undefined || !Number.isFinite(value)) return "n/a";
  return `${fmt(value * 100, 1)}%`;
}

function artifactHref(fromDir, target) {
  if (!target) return "#";
  const abs = path.resolve(target);
  const rel = path.relative(fromDir, abs);
  return rel || path.basename(target);
}

function tableRows(object = {}, options = {}) {
  const entries = Object.entries(object);
  if (!entries.length) return `<tr><td colspan="2" class="muted">none</td></tr>`;
  const sorted = options.sortValue
    ? entries.sort((a, b) => Number(b[1] || 0) - Number(a[1] || 0))
    : entries;
  return sorted
    .map(([key, value]) => `<tr><td>${escapeHtml(key)}</td><td>${escapeHtml(fmt(value))}</td></tr>`)
    .join("\n");
}

function metricCard(label, value, sub = "") {
  return `
    <article class="metric">
      <div class="metric-label">${escapeHtml(label)}</div>
      <div class="metric-value">${escapeHtml(value)}</div>
      ${sub ? `<div class="metric-sub">${escapeHtml(sub)}</div>` : ""}
    </article>
  `;
}

function pickTimeline(replay, maxItems = 18) {
  const important = replay.filter((entry) =>
    [
      "run_started",
      "browser_facts_observed",
      "semantic_observation",
      "motor_action",
      "run_finished",
    ].includes(entry.kind),
  );
  if (important.length <= maxItems) return important;
  const first = important.slice(0, Math.ceil(maxItems / 2));
  const last = important.slice(-Math.floor(maxItems / 2));
  return [...first, { kind: "timeline_gap", skipped: important.length - first.length - last.length }, ...last];
}

function summarizeTimelineEntry(entry) {
  if (entry.kind === "timeline_gap") return `${entry.skipped} entries omitted`;
  if (entry.kind === "browser_facts_observed") {
    return `${entry.reason || "facts"}: ${entry.summary?.count || 0} facts, ${entry.summary?.visual_objects || 0} visual`;
  }
  if (entry.kind === "semantic_observation") {
    const roles = entry.summary?.by_role || {};
    const roleText = Object.entries(roles)
      .map(([name, count]) => `${name}:${count}`)
      .join(", ");
    return `semantic: ${entry.summary?.count || 0} objects${roleText ? ` (${roleText})` : ""}`;
  }
  if (entry.kind === "motor_action") {
    const action = entry.action || {};
    const end = action.end ? ` -> ${fmt(action.end.x, 1)},${fmt(action.end.y, 1)}` : "";
    return `${action.id || "command"} ${action.basis || ""}${end}`;
  }
  if (entry.kind === "run_started") return `started ${entry.url || ""}`;
  if (entry.kind === "run_finished") return entry.reason || "finished";
  return entry.kind || "event";
}

function roleColor(role) {
  return {
    player: "#e84855",
    drop: "#2fc76f",
    enemy: "#f59f00",
    hazard: "#8a2be2",
    projectile_or_particle: "#3b82f6",
    ui: "#8b95a7",
    unknown: "#64748b",
  }[role] || "#64748b";
}

function commandMapSvg(report, commands, semanticFacts) {
  const width = 1280;
  const height = 900;
  const sampledCommands = commands.slice(-32);
  const sampledFacts = semanticFacts.slice(-90);
  const live = report.summary?.live_visual_state || {};
  const player = live.player;

  const factsSvg = sampledFacts
    .map((fact) => {
      const center = fact.visual_object?.center_css;
      if (!center) return "";
      const role = fact.semantic?.role?.name || "unknown";
      const r = role === "player" ? 7 : role === "drop" ? 4 : 5;
      return `<circle cx="${fmt(center.x, 2)}" cy="${fmt(center.y, 2)}" r="${r}" fill="${roleColor(role)}" opacity="0.38"><title>${escapeHtml(role)} ${escapeHtml(fact.semantic?.label || "")}</title></circle>`;
    })
    .join("\n");

  const commandSvg = sampledCommands
    .map((command, index) => {
      if (!command.start || !command.end) return "";
      const opacity = 0.18 + (index / Math.max(1, sampledCommands.length - 1)) * 0.65;
      return `<line x1="${fmt(command.start.x, 2)}" y1="${fmt(command.start.y, 2)}" x2="${fmt(command.end.x, 2)}" y2="${fmt(command.end.y, 2)}" stroke="#111827" stroke-width="3" opacity="${fmt(opacity, 2)}" stroke-linecap="round"><title>${escapeHtml(command.id || "drag")}</title></line>`;
    })
    .join("\n");

  const playerSvg = player
    ? `<circle cx="${fmt(player.x, 2)}" cy="${fmt(player.y, 2)}" r="12" fill="none" stroke="#e84855" stroke-width="4"><title>latest player anchor</title></circle>`
    : "";

  return `
    <svg class="map" viewBox="0 0 ${width} ${height}" role="img" aria-label="Local game fact and motor map">
      <rect x="0" y="0" width="${width}" height="${height}" fill="#f8fafc"/>
      <g opacity="0.32">
        ${Array.from({ length: 8 }, (_, i) => `<line x1="${i * 160}" y1="0" x2="${i * 160}" y2="${height}" stroke="#cbd5e1"/>`).join("")}
        ${Array.from({ length: 6 }, (_, i) => `<line x1="0" y1="${i * 150}" x2="${width}" y2="${i * 150}" stroke="#cbd5e1"/>`).join("")}
      </g>
      <g>${factsSvg}</g>
      <g>${commandSvg}</g>
      ${playerSvg}
    </svg>
  `;
}

function buildHtml({ runDir, report, replay, commands, semanticFacts }) {
  const summary = report.summary || {};
  const samples = summary.samples || {};
  const receipts = summary.receipts || {};
  const frames = summary.frames || {};
  const browserFacts = summary.browser_facts || {};
  const semantic = summary.semantic_facts || {};
  const dispatch = receipts.dispatch_ms || {};
  const readback = frames.readback_ms || {};
  const started = report.started_at_ms ? new Date(report.started_at_ms).toISOString() : "unknown";
  const verdict = report.ok ? "PASS" : "FAIL";
  const controller = report.command_args?.includes("--policy")
    ? report.command_args[report.command_args.indexOf("--policy") + 1]
    : report.args?.policy || "unknown";
  const timeline = pickTimeline(replay);
  const outputDir = path.resolve(runDir);

  const links = report.paths || {};
  const linkList = Object.entries(links)
    .map(([name, target]) => `<a href="${escapeHtml(artifactHref(outputDir, target))}">${escapeHtml(name)}</a>`)
    .join("");

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Saccade Local Game Reflex Review</title>
  <style>
    :root {
      color-scheme: light;
      --bg: #f4f7fb;
      --ink: #111827;
      --muted: #667085;
      --line: #d9e1ec;
      --panel: #ffffff;
      --good: #0f8f5f;
      --bad: #bf2f3c;
      --accent: #3457d5;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      background: var(--bg);
      color: var(--ink);
      font: 15px/1.45 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }
    header {
      padding: 28px 32px 18px;
      background: var(--panel);
      border-bottom: 1px solid var(--line);
    }
    main { padding: 24px 32px 36px; max-width: 1280px; }
    h1 { margin: 0 0 8px; font-size: 30px; letter-spacing: 0; }
    h2 { margin: 0 0 14px; font-size: 18px; letter-spacing: 0; }
    section {
      background: var(--panel);
      border: 1px solid var(--line);
      border-radius: 8px;
      padding: 18px;
      margin: 0 0 18px;
    }
    .status {
      display: inline-flex;
      align-items: center;
      min-height: 30px;
      padding: 0 12px;
      border-radius: 999px;
      color: #fff;
      background: ${report.ok ? "var(--good)" : "var(--bad)"};
      font-weight: 700;
      letter-spacing: 0.04em;
    }
    .muted { color: var(--muted); }
    .grid {
      display: grid;
      grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
      gap: 12px;
    }
    .metric {
      min-height: 94px;
      padding: 14px;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fbfdff;
    }
    .metric-label { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; }
    .metric-value { margin-top: 8px; font-size: 26px; font-weight: 750; }
    .metric-sub { margin-top: 4px; color: var(--muted); font-size: 13px; }
    .two {
      display: grid;
      grid-template-columns: minmax(0, 1.15fr) minmax(300px, 0.85fr);
      gap: 18px;
    }
    table { width: 100%; border-collapse: collapse; }
    td, th { border-bottom: 1px solid var(--line); padding: 9px 6px; text-align: left; vertical-align: top; }
    th { color: var(--muted); font-size: 12px; text-transform: uppercase; letter-spacing: 0.08em; }
    code {
      background: #eef3fb;
      border: 1px solid #d9e3f4;
      border-radius: 5px;
      padding: 2px 5px;
      font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
      font-size: 0.93em;
    }
    .map {
      width: 100%;
      height: auto;
      border: 1px solid var(--line);
      border-radius: 8px;
      background: #fff;
    }
    .timeline { margin: 0; padding: 0; list-style: none; }
    .timeline li {
      display: grid;
      grid-template-columns: 80px 160px minmax(0, 1fr);
      gap: 10px;
      border-bottom: 1px solid var(--line);
      padding: 8px 0;
    }
    .timeline li:last-child { border-bottom: 0; }
    .tag {
      display: inline-flex;
      align-items: center;
      min-height: 24px;
      padding: 0 8px;
      border-radius: 999px;
      background: #edf2ff;
      color: #243b8f;
      font-size: 12px;
      font-weight: 700;
      white-space: nowrap;
    }
    .links { display: flex; flex-wrap: wrap; gap: 8px; margin-top: 12px; }
    .links a {
      color: var(--accent);
      text-decoration: none;
      border: 1px solid var(--line);
      border-radius: 999px;
      padding: 5px 9px;
      background: #fff;
    }
    .legend { display: flex; flex-wrap: wrap; gap: 10px; margin-top: 10px; color: var(--muted); }
    .swatch { display: inline-block; width: 10px; height: 10px; border-radius: 50%; margin-right: 5px; vertical-align: -1px; }
    @media (max-width: 860px) {
      main, header { padding-left: 18px; padding-right: 18px; }
      .two { grid-template-columns: 1fr; }
      .timeline li { grid-template-columns: 64px minmax(0, 1fr); }
      .timeline .kind { display: none; }
    }
  </style>
</head>
<body>
  <header>
    <div class="status">${verdict}</div>
    <h1>Saccade Local Game Reflex Review</h1>
    <div class="muted">
      Run <code>${escapeHtml(path.basename(runDir))}</code> started ${escapeHtml(started)}.
      Controller <code>${escapeHtml(controller)}</code>.
    </div>
    <div class="links">${linkList}</div>
  </header>
  <main>
    <section>
      <h2>Outcome</h2>
      <div class="grid">
        ${metricCard("Fill Delta", fmt(summary.fill_delta), `last fill ${fmt(samples.last_debug?.fill)} / ${fmt(samples.last_debug?.fillCap)}`)}
        ${metricCard("HP Delta", fmt(summary.hp_delta), `last hp ${fmt(samples.last_debug?.hp)}`)}
        ${metricCard("Drop Delta", fmt(summary.drop_delta), `last drops ${fmt(samples.last_debug?.drops)}`)}
        ${metricCard("Game Time Scale", fmt(samples.time_scale), `${fmt(samples.game_time_delta_sec)}s game / ${fmt(samples.sample_wall_time_delta_sec)}s sampled`)}
        ${metricCard("Commands", fmt(summary.command_count), `${fmt(summary.command_receipts)} command receipts`)}
        ${metricCard("Dispatch p95", `${fmt(dispatch.p95)} ms`, `p50 ${fmt(dispatch.p50)} ms, max ${fmt(dispatch.max)} ms`)}
        ${metricCard("Readback p95", `${fmt(readback.p95)} ms`, `${fmt(frames.readback_ok)} / ${fmt(frames.count)} readback ok`)}
        ${metricCard("Semantic Facts", fmt(semantic.count), `${fmt(browserFacts.visual_objects)} visual objects`)}
      </div>
    </section>

    <section class="two">
      <div>
        <h2>Fact And Motor Map</h2>
        ${commandMapSvg(report, commands, semanticFacts)}
        <div class="legend">
          <span><span class="swatch" style="background:${roleColor("player")}"></span>player</span>
          <span><span class="swatch" style="background:${roleColor("drop")}"></span>drop</span>
          <span><span class="swatch" style="background:${roleColor("enemy")}"></span>enemy</span>
          <span><span class="swatch" style="background:${roleColor("projectile_or_particle")}"></span>projectile</span>
          <span>dark lines: recent drag commands</span>
        </div>
      </div>
      <div>
        <h2>Role Counts</h2>
        <table>
          <thead><tr><th>Role</th><th>Count</th></tr></thead>
          <tbody>${tableRows(semantic.by_role, { sortValue: true })}</tbody>
        </table>
        <h2 style="margin-top:20px">Palette Counts</h2>
        <table>
          <thead><tr><th>Palette</th><th>Count</th></tr></thead>
          <tbody>${tableRows(semantic.by_palette, { sortValue: true })}</tbody>
        </table>
      </div>
    </section>

    <section>
      <h2>Timeline</h2>
      <ol class="timeline">
        ${timeline
          .map((entry) => `
            <li>
              <span class="muted">${entry.kind === "timeline_gap" ? "" : `${fmt(entry.t_ms, 0)} ms`}</span>
              <span class="kind"><span class="tag">${escapeHtml(entry.kind || "event")}</span></span>
              <span>${escapeHtml(summarizeTimelineEntry(entry))}</span>
            </li>
          `)
          .join("")}
      </ol>
    </section>

    <section class="two">
      <div>
        <h2>What This Proves</h2>
        <table>
          <tbody>
            <tr><td>Release ServoShell ran the local game</td><td>${escapeHtml(report.ok ? "yes" : "no")}</td></tr>
            <tr><td>Browser facts were observed</td><td>${fmt(browserFacts.count)} facts</td></tr>
            <tr><td>Semantic facts drove the accepted path</td><td>${fmt(semantic.count)} semantic objects</td></tr>
            <tr><td>Motor commands reached the bridge</td><td>${fmt(summary.command_receipts)} receipts</td></tr>
            <tr><td>Game state improved without HP loss</td><td>fill ${fmt(summary.fill_delta)}, hp ${fmt(summary.hp_delta)}</td></tr>
            <tr><td>Sensitive page surface</td><td>${browserFacts.sensitive ? "present" : "none observed"}</td></tr>
          </tbody>
        </table>
      </div>
      <div>
        <h2>Known Limits</h2>
        <table>
          <tbody>
            <tr><td>Classifier</td><td>Local-game heuristic, not a universal object model.</td></tr>
            <tr><td>Policy</td><td>First visual pickup policy, not a strong player.</td></tr>
            <tr><td>Screenshot truth</td><td>Not used as the general safety model; this is a non-sensitive reflex run.</td></tr>
            <tr><td>stderr tail</td><td>${escapeHtml((report.stderr_tail || "").split(/\r?\n/).filter(Boolean).slice(-1)[0] || "none")}</td></tr>
          </tbody>
        </table>
      </div>
    </section>
  </main>
</body>
</html>
`;
}

async function main() {
  const args = parseArgs(process.argv);
  const runDir = path.resolve(args.runDir);
  const reportPath = path.join(runDir, "report.json");
  const report = await readJson(reportPath);
  const replay = await readJsonl(path.join(runDir, "replay.jsonl"));
  const commands = await readJsonl(path.join(runDir, "commands.jsonl"));
  const semanticFacts = await readJsonl(path.join(runDir, "semantic_facts.jsonl"));
  const output = path.resolve(args.output || path.join(runDir, "review.html"));
  const html = buildHtml({ runDir, report, replay, commands, semanticFacts });
  await fs.writeFile(output, html);
  console.log(`LOCAL_GAME_REFLEX_REVIEW_READY review=${output}`);
}

main().catch((error) => {
  console.error(error.stack || String(error));
  process.exit(1);
});
