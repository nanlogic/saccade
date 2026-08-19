#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const rootResult = spawnSync("git", ["rev-parse", "--show-toplevel"], { encoding: "utf8" });
if (rootResult.status !== 0) process.exit(0);
const root = rootResult.stdout.trim();
const command = process.platform === "win32" ? "mdg.cmd" : "mdg";
let result = spawnSync(command, ["check", root], { stdio: "inherit" });
if (result.error?.code === "ENOENT") {
  const cli = path.join(root, "bin", "mdg.mjs");
  if (!existsSync(cli)) {
    console.error("Markdown Gatekeeper is unavailable. Install mdg or restore this project's bin/mdg.mjs fallback.");
    process.exit(1);
  }
  result = spawnSync(process.execPath, [cli, "check", root], { stdio: "inherit" });
}
process.exit(result.status ?? 1);
