# Python host example

This standard-library example is the Python equivalent of the TypeScript host. It negotiates contract v1, consumes an explicit local user-grant artifact, and executes only a no-submit ordinary-field plan.

```bash
export SACCADE_GRANT_PATH=/absolute/path/to/grant.json
export SACCADE_ASSIGNMENTS_JSON='{"field-id":"ordinary value"}'
python3 docs/integration_examples/python-host/main.py
```

Set `SACCADE_MCP_COMMAND` to a packaged `saccade-mcp` binary when not running from this repository. The host pauses and closes the tab in `finally`, so user cancellation cannot leave an agent with an active grant.
