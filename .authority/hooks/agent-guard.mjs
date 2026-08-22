#!/usr/bin/env node

import process from "node:process";

let raw = "";
for await (const chunk of process.stdin) raw += chunk;

let input = {};
try {
  input = raw.trim() ? JSON.parse(raw) : {};
} catch {
  process.exit(0);
}

const toolName = String(input.tool_name || "");
const toolInput = input.tool_input || {};
const serialized = JSON.stringify(toolInput).replace(/\\\\/g, "/").toLowerCase();
const protectedPatterns = [
  "project_authority.md",
  ".authority/registry.json",
  ".authority/evidence/",
  "docs/current/"
];

const touchesProtected = protectedPatterns.some((pattern) => serialized.includes(pattern));
const directEditTool = /^(?:apply_patch|write|edit)$/i.test(toolName);
const command = String(toolInput.command || "");
const bashWriteIntent = /^bash$/i.test(toolName) && (
  /(?:^|[;&|\s])(?:set-content|add-content|out-file|remove-item|move-item|rename-item|copy-item|tee|rm|mv|cp)(?:\s|$)/i.test(command) ||
  /(?:sed|perl)\s+[^\r\n]*(?:-i|-pi)\b/i.test(command) ||
  /(?:^|[^>])>{1,2}(?!>)/.test(command)
);
const invokesPublisher = /(?:^|[\\/\s])(?:node\s+[^\s]*mdg\.mjs|mdg)(?:\s+)(?:publish|sync|init|adopt|owner|evidence)(?:\s|$)/i.test(
  command
);

if (touchesProtected && (directEditTool || bashWriteIntent) && !invokesPublisher) {
  const reason = "Markdown Gatekeeper blocked a direct authority edit. Write a proposal and use mdg publish/sync.";
  process.stdout.write(JSON.stringify({
    hookSpecificOutput: {
      hookEventName: "PreToolUse",
      permissionDecision: "deny",
      permissionDecisionReason: reason
    }
  }));
}
