#!/usr/bin/env node

const { spawn } = require("node:child_process");
const fs = require("node:fs/promises");
const path = require("node:path");

function parseArgs(argv) {
  const args = {
    mcp: path.join("target", "debug", "saccade-mcp"),
    outputDir: path.join("runs", "chrome_compat_mcp", `probe_${Date.now()}`),
  };
  for (let index = 2; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--mcp") args.mcp = argv[++index];
    else if (arg === "--grant-path") args.grantPath = argv[++index];
    else if (arg === "--output-dir") args.outputDir = argv[++index];
    else if (arg === "--action-label") args.actionLabel = argv[++index];
    else if (arg === "--fill-field") {
      const pair = argv[++index];
      const separator = pair.indexOf("=");
      if (separator <= 0) throw new Error("--fill-field requires field=value");
      args.fillFields ||= {};
      args.fillFields[pair.slice(0, separator)] = pair.slice(separator + 1);
    } else if (arg === "--inspect-fields") {
      args.inspectFields = argv[++index].split(",").filter(Boolean);
    }
    else throw new Error(`unknown argument: ${arg}`);
  }
  if (!args.grantPath) throw new Error("--grant-path is required");
  return args;
}

function startMcp(binary) {
  const child = spawn(binary, ["serve-stdio"], { stdio: ["pipe", "pipe", "pipe"] });
  const pending = [];
  let buffer = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => {
    buffer += chunk.toString();
    let newline;
    while ((newline = buffer.indexOf("\n")) >= 0) {
      const line = buffer.slice(0, newline).trim();
      buffer = buffer.slice(newline + 1);
      if (!line) continue;
      const resolve = pending.shift();
      if (resolve) resolve(JSON.parse(line));
    }
  });
  child.stderr.on("data", (chunk) => (stderr += chunk.toString()));
  return {
    child,
    call(id, method, params) {
      return new Promise((resolve, reject) => {
        if (child.exitCode !== null) {
          reject(new Error(`saccade-mcp exited before ${method}: ${stderr.trim()}`));
          return;
        }
        pending.push(resolve);
        child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
      });
    },
  };
}

async function main() {
  const args = parseArgs(process.argv);
  const outputDir = path.resolve(args.outputDir);
  await fs.mkdir(outputDir, { recursive: true });
  const mcp = startMcp(args.mcp);
  try {
    await mcp.call(1, "initialize", {});
    const grant = await mcp.call(2, "tools/call", {
      name: "saccade.tabs.grant_current",
      arguments: {
        grant_path: args.grantPath,
        reason: "AI-030B compatibility bridge probe",
      },
    });
    const grantContent = grant.result?.structuredContent;
    if (!grantContent || grantContent.status !== "ok") {
      throw new Error(`grant_current failed: ${JSON.stringify(grant)}`);
    }
    const tabId = grantContent.tab?.tab_id;
    const actions = await mcp.call(3, "tools/call", {
      name: "saccade.web.actions",
      arguments: { tab_id: tabId },
    });
    const actionsContent = actions.result?.structuredContent;
    if (!actionsContent || actionsContent.status !== "ok") {
      throw new Error(`web.actions failed: ${JSON.stringify(actions)}`);
    }

    let action = null;
    let actContent = null;
    if (args.actionLabel) {
      action = actionsContent.actions.find((item) => item.label === args.actionLabel);
      if (!action) throw new Error(`action label not found: ${args.actionLabel}`);
      const act = await mcp.call(4, "tools/call", {
        name: "saccade.web.act",
        arguments: {
          tab_id: tabId,
          action_id: action.action_id,
          basis_page_revision: actionsContent.page_revision,
        },
      });
      actContent = act.result?.structuredContent;
      if (!actContent || actContent.status !== "ok") {
        throw new Error(`web.act failed: ${JSON.stringify(act)}`);
      }
    }

    let fillContent = null;
    if (args.fillFields) {
      const fill = await mcp.call(5, "tools/call", {
        name: "saccade.web.fill_agent_fields",
        arguments: {
          tab_id: tabId,
          basis_page_revision: actionsContent.page_revision,
          fields: args.fillFields,
          agent_owned_only: true,
          block_sensitive: true,
          live_worker_only: true,
        },
      });
      fillContent = fill.result?.structuredContent;
      if (!fillContent || fillContent.status !== "ok") {
        throw new Error(`fill_agent_fields failed: ${JSON.stringify(fill)}`);
      }
    }

    let inspectContent = null;
    if (args.inspectFields) {
      const inspect = await mcp.call(6, "tools/call", {
        name: "saccade.web.inspect_fields",
        arguments: {
          tab_id: tabId,
          basis_page_revision: fillContent?.page_revision || actionsContent.page_revision,
          fields: args.inspectFields,
          redact_sensitive: true,
          explicit_fields_only: true,
          live_worker_only: true,
        },
      });
      inspectContent = inspect.result?.structuredContent;
      if (!inspectContent || inspectContent.status !== "ok") {
        throw new Error(`inspect_fields failed: ${JSON.stringify(inspect)}`);
      }
    }

    const report = {
      ok: true,
      runtime: grantContent.runtime,
      same_webview_attached: grantContent.same_webview_attached === true,
      tab_id: tabId,
      action_count: actionsContent.actions.length,
      action_label: action?.label || null,
      verification: actContent?.verification || null,
      fill: fillContent ? {
        requested: fillContent.requested,
        filled: fillContent.filled,
        rejected: fillContent.rejected,
        sensitive_fields_seen: fillContent.sensitive_fields_seen,
      } : null,
      inspected_fields: inspectContent?.fields || null,
      artifacts: actContent?.artifacts || actionsContent.artifacts || null,
    };
    await fs.writeFile(path.join(outputDir, "report.json"), `${JSON.stringify(report, null, 2)}\n`);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
  } finally {
    mcp.child.kill("SIGTERM");
  }
}

main().catch((error) => {
  console.error(error.stack || error.message || String(error));
  process.exitCode = 1;
});
