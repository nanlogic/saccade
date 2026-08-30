# How Saccade works

The Chrome/Edge Extension continuously turns an authorized page into semantic
objects. It sends one full observation and then compact deltas to a shared
loopback Node Broker. Each Agent's MCP adapter receives a private session and
can read only tabs leased to that session.

```text
page → Extension → Node Broker → MCP → Agent
```

The Broker keeps canonical current Truth, bounded revision history, exclusive
tab leases, deadlines, and a reliable command queue. Agents choose `full` or
`delta` for one exact `tab_id`; they never receive unrelated tabs.

Actions address a current semantic object, never a selector or coordinate. The
Extension performs bounded local actionability waiting and the Broker waits for
the resulting pushed Truth transition within the same deadline. A lost response
after dispatch is an unknown outcome and is never retried automatically.

For Broker process failure, the local state journal contains only hashed
session proof, exact Tab lease metadata, and value-free command occurrence. A
still-running MCP adapter can use its in-memory proof once to resume the same
`agent_session_id`; the proof then rotates. Truth is rebuilt from a fresh
Extension full snapshot. Without valid proof the lease is not exposed or
transferred.

See [the Node Broker contract](current/saccade-0-2-0-runtime-contract.md) and
[product boundary](current/product-execution-boundary.md) for the full contract.
