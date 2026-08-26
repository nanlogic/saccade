# @nanlogic/saccade

Node.js-only Saccade Broker, MCP adapter, and setup CLI.

```sh
npx -y @nanlogic/saccade install
```

Setup configures supported local Agent clients to run:

```sh
npx -y @nanlogic/saccade mcp
```

The MCP adapter starts or joins the loopback Broker automatically. The browser
Extension connects to `127.0.0.1:32177`; no binary download, Native Messaging
registration, platform driver, administrator access, signing, or install hook
is used.

Broker crash recovery writes only hashed session proofs, exact Tab lease
metadata, and value-free command occurrence to
`~/.saccade/broker-state.json`. The MCP adapter keeps the usable proof only in
memory and rotates it after a successful resume. Page Truth, form values,
action payloads, tokens, cookies, and credentials are not persisted. A command
that may have been dispatched before transport loss returns `outcome_unknown`
and is never replayed.

Commands:

```text
saccade mcp
saccade broker
saccade install
saccade update
saccade doctor
saccade uninstall [--purge]
```

Requires Node.js 18 or newer. Chrome and Edge use the same package and Extension
candidate.
