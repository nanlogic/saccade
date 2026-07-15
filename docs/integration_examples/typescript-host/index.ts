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

async function main(): Promise<void> {
  const overrideCommand = process.env.SACCADE_MCP_COMMAND?.trim().split(/\s+/);
  const command = overrideCommand?.[0] ?? "cargo";
  const args = overrideCommand?.slice(1) ?? ["run", "-q", "-p", "saccade-mcp", "--", "serve-stdio"];
  const grantPath = process.env.SACCADE_GRANT_PATH;
  const assignments = JSON.parse(process.env.SACCADE_ASSIGNMENTS_JSON ?? "{}") as Json;
  const lifecycleOnly = process.env.SACCADE_LIFECYCLE_ONLY === "1";
  if (!grantPath || (!lifecycleOnly && Object.keys(assignments).length === 0)) {
    throw new Error("set SACCADE_GRANT_PATH and SACCADE_ASSIGNMENTS_JSON");
  }

  const child = spawn(command, args, { stdio: "pipe" });
  const mcp = new McpClient(child);
  let tabId: number | undefined;

  try {
    await mcp.request("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "vendor-host-example", version: "0.1" } });
    const capabilities = await mcp.tool("saccade.system.capabilities");
    const contract = ((capabilities.saccade as Json).contract_version as string | undefined) ?? "";
    if (!contract.startsWith("1.")) throw new Error(`unsupported Saccade contract ${contract}`);
    const grant = await mcp.tool("saccade.tabs.grant_current", { grant_path: grantPath, reason: "user requested ordinary-field draft assistance", policy: { explicit_user_grant: true, local_dev_only: true } });
    tabId = (grant.tab as Json).tab_id as number;
    if (lifecycleOnly) {
      const target = process.env.SACCADE_NAVIGATE_URL;
      if (!target) throw new Error("set SACCADE_NAVIGATE_URL");
      const policy = { same_webview_only: true, user_granted_tab_only: true };
      const navigation = await mcp.tool("saccade.browser.navigate", { tab_id: tabId, action: "navigate", url: target, policy });
      await new Promise((resolve) => setTimeout(resolve, 250));
      const status = await mcp.tool("saccade.browser.navigate", { tab_id: tabId, action: "status", policy });
      const paused = await mcp.tool("saccade.tabs.pause_agent", { tab_id: tabId });
      console.log(JSON.stringify({
        contract,
        attached: grant.same_webview_attached,
        capabilities: grant.same_webview_capabilities,
        navigate_status: navigation.status,
        current_url: status.url,
        pause_status: paused.status,
      }, null, 2));
    } else {
      const inventory = await mcp.tool("saccade.web.form_inventory", { tab_id: tabId, mode: "actionable" });
      const revision = inventory.page_revision as number;
      const policy = { block_sensitive: true, preserve_existing: true, no_submit: true };
      const plan = await mcp.tool("saccade.web.form_compile_plan", { tab_id: tabId, basis_page_revision: revision, assignments, policy });
      const executed = await mcp.tool("saccade.web.form_execute_plan", { tab_id: tabId, basis_page_revision: revision, expected_plan_id: plan.plan_id, assignments, policy });
      console.log(JSON.stringify({ status: executed.status, summary: executed.summary, replay: executed.replay_path }, null, 2));
      await mcp.tool("saccade.tabs.pause_agent", { tab_id: tabId });
    }
  } finally {
    if (tabId !== undefined) await mcp.tool("saccade.tabs.close", { tab_id: tabId }).catch(() => undefined);
    child.kill();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
