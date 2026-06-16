#!/usr/bin/env node

const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const repoRoot = path.resolve(__dirname, "..");

function usage() {
  console.error(`Usage:
  node scripts/create_redacted_note_packet.js \\
    --source-url https://appstoreconnect.apple.com/apps \\
    --title "App Store Connect review note" \\
    --task evaluate_edit \\
    --text-file /path/to/redacted.txt

Options:
  --text <text>          Redacted text to package.
  --text-file <path>     Read redacted text from a local file.
  --source-url <url>     Original page URL. Query/fragment will be stripped.
  --title <title>        Human-readable packet title.
  --task <task>          evaluate_edit | draft_reply | summarize | checklist.
  --audience <audience>  Intended reader for the AI review.

If neither --text nor --text-file is supplied, text is read from stdin.`);
}

function parseArgs(argv) {
  const options = {
    sourceUrl: "",
    title: "Redacted fallback note",
    task: "evaluate_edit",
    audience: "human operator and AI reviewer",
    text: "",
    textFile: "",
  };

  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    const next = () => {
      index += 1;
      if (index >= argv.length) {
        throw new Error(`${arg} requires a value`);
      }
      return argv[index];
    };

    if (arg === "--help" || arg === "-h") {
      usage();
      process.exit(0);
    } else if (arg === "--source-url") {
      options.sourceUrl = next();
    } else if (arg === "--title") {
      options.title = next();
    } else if (arg === "--task") {
      options.task = next();
    } else if (arg === "--audience") {
      options.audience = next();
    } else if (arg === "--text") {
      options.text = next();
    } else if (arg === "--text-file") {
      options.textFile = next();
    } else {
      throw new Error(`unknown argument: ${arg}`);
    }
  }

  return options;
}

function readText(options) {
  if (options.textFile) {
    return fs.readFileSync(path.resolve(options.textFile), "utf8");
  }
  if (options.text) {
    return options.text;
  }
  if (!process.stdin.isTTY) {
    return fs.readFileSync(0, "utf8");
  }
  return "";
}

function main() {
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    usage();
    process.exit(2);
  }

  const redactedText = readText(options).trim();
  if (!redactedText) {
    console.error("redacted text is required via --text, --text-file, or stdin");
    usage();
    process.exit(2);
  }

  const request = {
    jsonrpc: "2.0",
    id: 1,
    method: "tools/call",
    params: {
      name: "saccade.report.redacted_note",
      arguments: {
        source_url: options.sourceUrl,
        title: options.title,
        task: options.task,
        audience: options.audience,
        redacted_text: redactedText,
        policy: {
          redacted_user_supplied: true,
          no_live_site_access: true,
        },
      },
    },
  };

  const child = spawnSync("cargo", ["run", "-q", "-p", "saccade-mcp", "--", "serve-stdio"], {
    cwd: repoRoot,
    input: `${JSON.stringify(request)}\n`,
    encoding: "utf8",
  });

  if (child.error) {
    console.error(child.error.message);
    process.exit(1);
  }
  if (child.status !== 0) {
    process.stderr.write(child.stderr);
    process.exit(child.status ?? 1);
  }

  const lines = child.stdout.split(/\r?\n/).filter((line) => line.trim());
  const response = lines.length ? JSON.parse(lines[lines.length - 1]) : {};
  if (response.error) {
    console.error(response.error.data || response.error.message || "MCP call failed");
    process.exit(1);
  }

  const packet = response.result?.structuredContent;
  if (!packet) {
    console.error("MCP response did not include structuredContent");
    process.exit(1);
  }

  const artifacts = packet.artifacts || {};
  console.log(`status=${packet.status}`);
  console.log(`task=${packet.task}`);
  console.log(`site_policy=${JSON.stringify(packet.site_policy)}`);
  console.log(`warnings=${(packet.redaction?.warnings || []).join(",") || "none"}`);
  console.log(`run_dir=${artifacts.run_dir}`);
  console.log(`redacted_note=${artifacts.redacted_note}`);
  console.log(`ai_review_prompt=${artifacts.ai_review_prompt}`);
  console.log(`report=${artifacts.report}`);
}

main();
