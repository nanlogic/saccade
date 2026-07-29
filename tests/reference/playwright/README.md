# Playwright reference oracle

This directory is an out-of-band comparison harness explicitly requested for
public-page dogfood. It is not a Saccade action route, fallback, verifier, or
source of receipts. Saccade must pass independently before these results are
compared. The oracle cannot create or upgrade a Saccade receipt.

The harness uses locator access and screenshots because its only purpose is to
provide an external reference result. No Playwright package or code is loaded
by the Extension, Native Host, Runtime, MCP adapter, or shipped product.
