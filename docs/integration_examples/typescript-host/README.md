# TypeScript host example

This dependency-free Node stdio MCP client negotiates the Saccade v1 contract, attaches only to a user-granted tab, and executes a no-submit form plan. It never reads or prints a bridge capability token.

Prerequisites:

1. A user has opened the visible Saccade browser and explicitly granted the current tab.
2. Set `SACCADE_GRANT_PATH` to the resulting local grant artifact.
3. Set `SACCADE_ASSIGNMENTS_JSON` to a JSON object of ordinary field IDs and values that your own policy permits.

Run with a TypeScript runner, for example:

```bash
npx tsx docs/integration_examples/typescript-host/index.ts
```

The example invokes `cargo run -q -p saccade-mcp -- serve-stdio` by default; set `SACCADE_MCP_COMMAND` to a packaged `saccade-mcp` binary for distribution. On cancellation or error, it pauses then closes the MCP tab.

Set `SACCADE_LIFECYCLE_ONLY=1` and `SACCADE_NAVIGATE_URL` to run only the
engine-neutral attach, navigate, pause, and close flow. A packaged command may
include arguments, for example
`SACCADE_MCP_COMMAND='/path/to/saccade-mcp serve-stdio'`.
