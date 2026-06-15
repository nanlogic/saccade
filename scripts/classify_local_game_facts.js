#!/usr/bin/env node

const fs = require("node:fs/promises");
const path = require("node:path");
const { readJsonl } = require("./lib/reflex_live_bridge");
const { writeFactsJsonl } = require("./lib/browser_fact_stream");
const {
  classifyLocalGameFacts,
  summarizeSemanticFacts,
} = require("./lib/local_game_fact_classifier");

function usage() {
  console.error(`usage:
  node scripts/classify_local_game_facts.js \\
    --facts runs/local_game_reflex/<run>/facts.jsonl \\
    [--output-dir runs/local_game_reflex/<run>]
`);
}

function parseArgs(argv) {
  const args = {};
  for (let i = 2; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--facts") {
      args.facts = argv[++i];
    } else if (arg === "--output-dir") {
      args.outputDir = argv[++i];
    } else if (arg === "--help" || arg === "-h") {
      usage();
      process.exit(0);
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }
  if (!args.facts) throw new Error("--facts is required");
  args.outputDir = args.outputDir || path.dirname(args.facts);
  return args;
}

async function main() {
  const args = parseArgs(process.argv);
  await fs.mkdir(args.outputDir, { recursive: true });
  const facts = await readJsonl(args.facts);
  const semanticFacts = classifyLocalGameFacts(facts);
  const semanticFactsPath = path.join(args.outputDir, "semantic_facts.jsonl");
  const reportPath = path.join(args.outputDir, "semantic_report.json");
  await fs.writeFile(semanticFactsPath, "");
  await writeFactsJsonl(semanticFactsPath, semanticFacts);

  const report = {
    ok: semanticFacts.length > 0,
    args,
    semantic_facts_path: semanticFactsPath,
    summary: summarizeSemanticFacts(semanticFacts),
    samples: semanticFacts.slice(0, 12),
  };
  await fs.writeFile(reportPath, JSON.stringify(report, null, 2));
  console.log(
    JSON.stringify(
      {
        ok: report.ok,
        report: reportPath,
        semantic_facts: semanticFactsPath,
        summary: report.summary,
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
