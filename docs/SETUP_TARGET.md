# Setup target

Saccade 0.2.0 ships as one npm package plus one Chrome/Edge Extension candidate.

```sh
npx -y @nanlogic/saccade install
```

The command creates a user Profile when absent and configures supported local
Agent clients to launch `npx -y @nanlogic/saccade mcp`. It downloads no binary,
writes no browser or OS registration, requests no administrator access, and
runs no npm `postinstall` hook.

Node.js 18+ is the only local runtime prerequisite. The same package runs on
macOS, Windows, and Linux wherever the Agent and browser can both reach the
loopback Broker.

`uninstall` removes managed client configuration and preserves the Profile.
`uninstall --purge` also removes the dedicated `.saccade` data directory.
