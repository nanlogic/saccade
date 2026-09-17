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

## Experimental observation in 0.2.3

Use with Saccade Extension 0.4.16 for screenshots, sampled video frames and
loaded captions, and short 3D sequences from applications using the Saccade
scene bridge. One observation approval covers this Agent's authorized tabs
for its live session. Other Agents and unshared tabs are excluded; Stop sharing
revokes access to a tab.

Video sampling may pause or seek playback. Sparse frames can miss brief events;
audio is not transcribed. Scene state is application-reported, not independent
proof of a correct animation. Screenshots and frames can contain personal
information and are not persisted in Saccade diagnostics.

Video and 3D observation are experimental. Comparative visual-accuracy
evaluation is incomplete; no accuracy, speed advantage or token savings are
claimed. Low-level WebGL rendering diagnostics remain a separate prototype.
