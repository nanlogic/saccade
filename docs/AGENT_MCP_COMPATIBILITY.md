# Agent MCP compatibility

Configure a local MCP client with:

```json
{
  "command": "npx",
  "args": ["-y", "@nanlogic/saccade", "mcp"]
}
```

The adapter speaks stdio MCP and starts or joins the Node Broker automatically.
Each connection gets a distinct session. The client must call
`saccade.system.capabilities`, then use exact tab IDs returned by
`saccade.tabs.open` or `saccade.tabs.list`.

If the shared Broker restarts while the MCP adapter remains alive, the adapter
resumes the same session with an in-memory, single-use proof and receives a
rotated proof. It may retry rejected-before-dispatch calls and idempotent reads.
It never retries `tabs.open`, `tabs.close`, or `saccade.act` after an ambiguous
transport failure.

Truth reads always specify `mode: "full"` or `mode: "delta"`; delta requires
`after_revision`. Agents must not substitute browser selectors, page scripts,
coordinates, Playwright, or another action route.

Cloud-only MCP clients cannot reach a browser on a user's loopback interface and
are therefore incompatible with this local product route.
