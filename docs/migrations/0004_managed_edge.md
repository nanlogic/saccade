# Migration 0004: managed Edge development route

- Source baseline: the Edge Native Messaging path in the private
  `nanlogic/saccade-legacy` macOS packaging files at commit `8c4defb3f8b0`.
- Reviewed source paths: `packaging/macos/install-dev.sh`,
  `packaging/macos/health-check.sh`, and `packaging/macos/SaccadeApp.swift`.
- Destination: `scripts/dev.sh`, `scripts/dev_probe.py`, the Extension tests,
  CI, and Batch 0 documentation.
- Retained: Edge's user Native Messaging directory at
  `~/Library/Application Support/Microsoft Edge/NativeMessagingHosts` and the
  fixed development Extension origin.
- Shared route: Edge uses the same synchronized development Extension, Native
  Host name, Runtime binary, owner-only IPC, MCP tools, native input, fixtures,
  and probe as Chrome. Only executable discovery, browser profile, PID, log,
  and evidence directories differ. The Extension and fixtures are copied to
  the fixed Saccade Dev directory before launch so launchd does not require
  repository-folder TCC access.
- Process rule: Chrome and Edge run in sequence. Starting one stops only the
  recorded PID for the other, which prevents two browser instances from
  competing for one development Host session.
- Evidence rule: `test chrome` and `test edge` write a browser field and store
  files below `<timestamp>/<browser>/`. `test all` uses one timestamp for both.
- Intentionally excluded: Edge Add-ons packaging, a second protocol, a second
  control Registry, alternate input, browser automation, and Catalog status
  promotion from local development evidence.
- Checks: shell syntax, Extension Node tests, Rust tests and Clippy, Catalog
  generation, the single-architecture gate, and native four-control runs in
  both managed browser profiles.
- Native Edge evidence: Microsoft Edge Stable `150.0.4078.105` passed the
  four-control and Profile routes. Paired run `20260728T224742Z` produced
  Chrome and Edge receipts under one timestamp; every click, type, click, and
  select dispatch was `accepted_by_os`, every postcondition was `verified`, and
  both browsers rejected the consumed stale token. The probe found no textfield
  sentinel in saved evidence.
- Public status: Catalog rows remain `implementation`; release Chrome and Edge
  evidence remains `pending`.
