# Agent MCP compatibility

## What is already portable

`saccade-runtime mcp` is the small, vendor-neutral Saccade product adapter. It
uses standard MCP over stdio and exposes only capabilities, tab list/open/close,
and Truth read/resource updates. Codex, Claude, and any MCP client that supports
stdio tools can use the same executable and schemas.

Example Claude Desktop entry (replace the executable with the installed
absolute path):

```json
{
  "mcpServers": {
    "saccade": {
      "command": "/absolute/path/to/saccade-runtime",
      "args": ["mcp"]
    }
  }
}
```

The browser Extension and Native Host must already be installed and authorized.
The public target is the store Extension plus `npx -y @saccade/setup`, which
installs the headless Runtime, user-level Native Messaging manifests, and local
MCP entries. No Accessibility permission is required for this Truth-only route.
`tabs.close` is limited to Agent-owned tabs; user-shared tabs are rejected.

The repository's `Saccade Dev Runtime.app` wrapper is internal development
tooling. It is not installed for users. Although the repository also contains
an explicitly launched historical actuator subcommand, normal Runtime startup
never launches it and never receives browser-control authority.

Cloud-only clients cannot start the local STDIO MCP or reach the user's local
Extension and Native Host. The first release reports this as incompatible and
does not add a remote relay.

## Why a server cannot borrow the client's browser tool

MCP tool calls run from client to server. Current Codex MCP host documentation
supports server tools, resources, prompts/instructions, stdio and streamable
HTTP transports. It does not define a portable server callback that invokes a
client-owned Browser or Computer Use tool. Claude and Codex also expose
different native execution surfaces.

Therefore a so-called universal `web-act` MCP must own a real execution backend;
it cannot be a thin relay to an unspecified client tool. Pretending otherwise
would make same-tab behavior client-specific and untestable.

## Optional executor contract

If a separate executor is built, keep it outside default Saccade and require:

- standard MCP transport and identical tools for Codex and Claude;
- an explicit same-browser-instance and same-tab readiness proof;
- finite semantic operations: `click`, `type`, `select`, `scroll`, and `upload`;
- no selectors, arbitrary coordinates, page scripts, Playwright, or CDP;
- current Truth identity/revision binding and explicit stale rejection;
- explicit permissions and provenance on every result;
- no claim that its receipts prove the page changed—the next Saccade delta does.

The existing `saccade-runtime reference-actuator-mcp` is the development-only
prototype of an executor-owned backend. It is standard MCP and works with
Codex or Claude, but it depends on native input permission and remains excluded
from core product and fair Playwright evidence. It must not be silently renamed
or promoted as the Agent client's own web-act tool.

Angular Material showed why `scroll` is part of the minimum executor contract:
Edge compiled only the initial shell until the example viewport was reached;
after native anchor navigation Saccade received a pushed delta containing four
selects and four options. A general executor should scroll and let Truth report
the newly rendered objects, not encode an Angular URL or selector workaround.
