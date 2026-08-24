#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const rootResult = spawnSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" });
if (rootResult.status !== 0) process.exit(0);
const root = rootResult.stdout.trim();
const command = process.platform === "win32" ? "mdg.cmd" : "mdg";
let result = spawnSync(command, ["check", root], {
  stdio: "inherit",
  shell: process.platform === "win32",
});
if (result.error?.code === "ENOENT" || result.status === 9009) {
  const codexHome = process.env.CODEX_HOME
    || path.join(process.env.USERPROFILE || process.env.HOME || "", ".codex");
  const managed = path.join(codexHome, "bin", process.platform === "win32" ? "mdg.cmd" : "mdg");
  if (existsSync(managed)) {
    result = spawnSync(managed, ["check", root], {
      stdio: "inherit",
      shell: process.platform === "win32",
    });
  } else {
    const cli = path.join(root, "bin", "mdg.mjs");
    if (!existsSync(cli)) {
      console.error("Markdown Gatekeeper is unavailable. Install mdg or restore a managed launcher.");
      process.exit(1);
    }
    result = spawnSync(process.execPath, [cli, "check", root], { stdio: "inherit" });
  }
}
process.exit(result.status ?? 1);
