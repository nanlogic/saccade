#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const {
  ReflexLiveBridge,
  sleep,
  summarizeReceipts,
  summarizeReflexEvents,
} = require("./lib/reflex_live_bridge");

function usage() {
  console.error(`usage:
  node scripts/probe_reflex_live_click_fixture.js \\
    --servoshell /path/to/servoshell \\
    [--headed] [--window-size 1024x740] \\
    [--output-dir runs/reflex_live_click/<name>]
`);
}

function defaultFixtureUrl() {
  return `file://${path.resolve("test_pages/browser_session/index.html")}`;
}

function parseArgs(argv) {
  const args = {
    url: defaultFixtureUrl(),
    headless: true,
    windowSize: "1024x740",
    outputDir: path.join("runs", "reflex_live_click", `click_${Date.now()}`),
  };

  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--servoshell") {
      args.servoshell = argv[++i];
    } else if (arg === "--url") {
      args.url = argv[++i];
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

  return args;
}

async function waitForButton(bridge, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await bridge.execute(`
      const button = document.querySelector("#verify-action");
      if (!button) return null;
      const rect = button.getBoundingClientRect();
      return {
        title: document.title,
        readyState: document.readyState,
        dpr: window.devicePixelRatio || 1,
        rect: { x: rect.x, y: rect.y, w: rect.width, h: rect.height },
        revision: document.body.dataset.sessionRevision || null,
        buttonText: button.textContent,
        statusText: document.querySelector("#status")?.textContent || null,
      };
    `);
    if (last?.rect?.w > 0 && last?.rect?.h > 0) {
      return last;
    }
    await sleep(100);
  }
  throw new Error(`fixture button did not become visible: ${JSON.stringify(last)}`);
}

async function waitForRevision(bridge, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  while (Date.now() < deadline) {
    last = await bridge.execute(`
      return {
        revision: document.body.dataset.sessionRevision || null,
        buttonText: document.querySelector("#verify-action")?.textContent || null,
        statusText: document.querySelector("#status")?.textContent || null,
      };
    `);
    if (last?.revision === "1") {
      return last;
    }
    await sleep(50);
  }
  return last;
}

async function main() {
  const args = parseArgs(process.argv);
  const bridge = new ReflexLiveBridge({
    servoshell: args.servoshell,
    url: args.url,
    headless: args.headless,
    windowSize: args.windowSize,
    outputDir: args.outputDir,
    observeMaxFrames: 80,
  });

  let ok = false;
  let pre = null;
  let post = null;
  let command = null;
  const startedAtMs = Date.now();

  try {
    await bridge.start();
    await bridge.open(args.url);
    pre = await waitForButton(bridge, 10000);
    const x = (pre.rect.x + pre.rect.w / 2) * pre.dpr;
    const y = (pre.rect.y + pre.rect.h / 2) * pre.dpr;
    command = {
      id: "live-click-fixture-1",
      type: "click",
      x,
      y,
    };
    await bridge.appendCommand(command);
    post = await waitForRevision(bridge, 5000);
    ok = post?.revision === "1" && post?.buttonText === "Verified";
  } finally {
    await bridge.stop();
  }

  const endedAtMs = Date.now();
  const receipts = await bridge.receipts();
  const frames = await bridge.frames();
  const report = {
    ok,
    args,
    command: args.servoshell,
    command_args: bridge.commandArgs,
    webdriver_port: bridge.port,
    paths: bridge.paths,
    started_at_ms: startedAtMs,
    ended_at_ms: endedAtMs,
    click_command: command,
    pre,
    post,
    summary: {
      receipts: summarizeReceipts(receipts),
      frames: summarizeReflexEvents(frames),
    },
    receipts,
    frames,
    process: bridge.processInfo(),
  };

  const reportPath = path.join(args.outputDir, "report.json");
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(
    JSON.stringify(
      {
        ok,
        report: reportPath,
        click_command: command,
        post,
        summary: report.summary,
        stderr_tail: report.process.stderr_tail.split("\n").slice(-8).join("\n"),
      },
      null,
      2,
    ),
  );

  if (!ok) {
    process.exitCode = 1;
  }
}

main().catch((error) => {
  console.error(error.stack || error.message);
  usage();
  process.exit(1);
});
