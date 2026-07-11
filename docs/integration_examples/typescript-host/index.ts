import { ChildProcessWithoutNullStreams, spawn } from "node:child_process";
import * as readline from "node:readline";

type Json = Record<string, unknown>;

class McpClient {
  private nextId = 0;
  private pending = new Map<number, { resolve: (value: Json) => void; reject: (error: Error) => void }>();

  constructor(private readonly child: ChildProcessWithoutNullStreams) {
    readline.createInterface({ input: child.stdout }).on("line", (line) => {
      const response = JSON.parse(line) as Json;
      const id = response.id as number | undefined;
      if (id === undefined) return;
      const pending = this.pending.get(id);
      if (!pending) return;
      this.pending.delete(id);
      if (response.error) {
        const data = (response.error as Json).data as Json | undefined;
        pending.reject(new Error(`${data?.saccade_code ?? "MCP_ERROR"}: ${data?.detail ?? "request failed"}`));
      } else pending.resolve(response.result as Json);
    });
  }

  request(method: string, params: Json = {}): Promise<Json> {
    const id = ++this.nextId;
    this.child.stdin.write(JSON.stringify({ jsonrpc: "2.0", id, method, params }) + "\n");
    return new Promise((resolve, reject) => this.pending.set(id, { resolve, reject }));
  }

  async tool(name: string, arguments_: Json = {}): Promise<Json> {
    const result = await this.request("tools/call", { name, arguments: arguments_ });
    return (result.structuredContent as Json) ?? {};
  }
}

const command = process.env.SACCADE_MCP_COMMAND ?? "cargo";
const args = process.env.SACCADE_MCP_COMMAND ? [] : ["run", "-q", "-p", "saccade-mcp", "--", "serve-stdio"];
const grantPath = process.env.SACCADE_GRANT_PATH;
const assignments = JSON.parse(process.env.SACCADE_ASSIGNMENTS_JSON ?? "{}") as Json;
if (!grantPath || Object.keys(assignments).length === 0) throw new Error("set SACCADE_GRANT_PATH and SACCADE_ASSIGNMENTS_JSON");

const child = spawn(command, args, { stdio: "pipe" });
const mcp = new McpClient(child);
let tabId: number | undefined;

try {
  await mcp.request("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "vendor-host-example", version: "0.1" } });
  const capabilities = await mcp.tool("saccade.system.capabilities");
  const contract = ((capabilities.saccade as Json).contract_version as string | undefined) ?? "";
  if (!contract.startsWith("1.")) throw new Error(`unsupported Saccade contract ${contract}`);
  const grant = await mcp.tool("saccade.tabs.grant_current", { grant_path: grantPath, reason: "user requested ordinary-field draft assistance", policy: { explicit_user_grant: true, local_dev_only: true } });
  tabId = grant.tab_id as number;
  const inventory = await mcp.tool("saccade.web.form_inventory", { tab_id: tabId, mode: "actionable" });
  const revision = inventory.page_revision as number;
  const policy = { block_sensitive: true, preserve_existing: true, no_submit: true };
  const plan = await mcp.tool("saccade.web.form_compile_plan", { tab_id: tabId, basis_page_revision: revision, assignments, policy });
  const executed = await mcp.tool("saccade.web.form_execute_plan", { tab_id: tabId, basis_page_revision: revision, expected_plan_id: plan.plan_id, assignments, policy });
  console.log(JSON.stringify({ status: executed.status, summary: executed.summary, replay: executed.replay_path }, null, 2));
  await mcp.tool("saccade.tabs.pause_agent", { tab_id: tabId });
} finally {
  if (tabId !== undefined) await mcp.tool("saccade.tabs.close", { tab_id: tabId }).catch(() => undefined);
  child.kill();
}
