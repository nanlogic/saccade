#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const {
  drainBrowserFacts,
  factTextCorpus,
  installBrowserFactStream,
  summarizeFacts,
  writeFactsJsonl,
} = require("./lib/browser_fact_stream");
const { ReflexLiveBridge, sleep } = require("./lib/reflex_live_bridge");

function usage() {
  console.error(`usage:
  node scripts/probe_browser_fact_stream.js \\
    --servoshell /path/to/servoshell \\
    [--url file:///.../test_pages/browser_fact_stream/index.html] \\
    [--headed] [--output-dir runs/browser_fact_stream/<name>]
`);
}

function defaultUrl() {
  return `file://${path.resolve("test_pages/browser_fact_stream/index.html")}`;
}

function parseArgs(argv) {
  const args = {
    url: defaultUrl(),
    headless: true,
    windowSize: "1024x740",
    outputDir: path.join("runs", "browser_fact_stream", `facts_${Date.now()}`),
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

async function triggerFixtureMutations(bridge) {
  await bridge.execute(`
    document.querySelector("#add-task").click();
    document.querySelector("#add-sensitive").click();
    document.querySelector("#update-canvas").click();
    return true;
  `);
}

async function main() {
  const args = parseArgs(process.argv);
  await fs.mkdir(args.outputDir, { recursive: true });
  const factsPath = path.join(args.outputDir, "facts.jsonl");
  await fs.writeFile(factsPath, "");

  const bridge = new ReflexLiveBridge({
    servoshell: args.servoshell,
    url: args.url,
    headless: args.headless,
    windowSize: args.windowSize,
    outputDir: args.outputDir,
    observeMaxFrames: 10,
  });

  let installResult = null;
  let initialFacts = [];
  let mutationFacts = [];
  let ok = false;

  try {
    await bridge.start();
    await bridge.open(args.url);
    installResult = await installBrowserFactStream(bridge, {
      allowCanvasDebugValues: false,
      textLimit: 140,
    });
    await sleep(150);
    initialFacts = await drainBrowserFacts(bridge, 500);
    await writeFactsJsonl(factsPath, initialFacts);

    await triggerFixtureMutations(bridge);
    await sleep(250);
    mutationFacts = await drainBrowserFacts(bridge, 500);
    await writeFactsJsonl(factsPath, mutationFacts);
  } finally {
    await bridge.stop();
  }

  const facts = [...initialFacts, ...mutationFacts];
  const summary = summarizeFacts(facts);
  const corpus = factTextCorpus(facts);
  const forbidden = [
    "123-45-6789",
    "4111111111111111",
    "correct-horse-battery",
  ].filter((needle) => corpus.includes(needle));
  const hasChildAdded = facts.some((fact) => fact.reason === "child_added");
  const hasSensitive = facts.some((fact) => fact.fact_type === "sensitive_field_seen");
  const hasCanvas = facts.some((fact) => fact.fact_type === "canvas_seen");
  const hasActionable = facts.some((fact) => fact.fact_type === "actionable_seen");
  ok = hasChildAdded && hasSensitive && hasCanvas && hasActionable && forbidden.length === 0;

  const report = {
    ok,
    args,
    facts_path: factsPath,
    install_result: installResult,
    summary,
    checks: {
      has_child_added: hasChildAdded,
      has_sensitive: hasSensitive,
      has_canvas: hasCanvas,
      has_actionable: hasActionable,
      forbidden_value_leaks: forbidden,
    },
    samples: {
      initial: initialFacts.slice(0, 12),
      mutation: mutationFacts.slice(0, 16),
    },
    process: bridge.processInfo(),
  };
  const reportPath = path.join(args.outputDir, "report.json");
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(
    JSON.stringify(
      {
        ok,
        report: reportPath,
        facts: factsPath,
        summary,
        checks: report.checks,
        stderr_tail: report.process.stderr_tail.split("\\n").slice(-8).join("\\n"),
      },
      null,
      2,
    ),
  );

  if (!ok) process.exitCode = 1;
}

main().catch((error) => {
  console.error(error.stack || error.message);
  usage();
  process.exit(1);
});
