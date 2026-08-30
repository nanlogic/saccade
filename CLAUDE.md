# Claude Instructions

<!-- markdown-gatekeeper:managed:start -->
## Markdown Gatekeeper protocol

This project dogfoods Markdown Gatekeeper. Before planning or implementation:

1. Read `PROJECT_AUTHORITY.md` and the current sources it names.
2. Run `mdg status .`. If the global command is unavailable, use the managed launcher at `<Codex home>/bin/mdg` (or `mdg.cmd` on Windows). Only if that launcher is unavailable, first verify that `./bin/mdg.mjs` exists and then use `node ./bin/mdg.mjs`; do not run a predictable failing command to probe availability.
3. Treat unregistered Markdown as a proposal, never as current authority.
4. Put competing designs in `docs/proposals/`; do not create a second canonical source.
5. Never edit `PROJECT_AUTHORITY.md`, `.authority/registry.json`, or `docs/current/**` directly. Use the publisher command.
6. If status reports registry revision 0 or pending legacy entrypoints, automatically run `mdg init . --host-session codex --yes` (or the repository-local CLI) and complete any current-session review handoff without asking the user to repeat initialization.
7. Run `mdg check .` before declaring authority-related work complete, using the verified local fallback only when necessary.

Use `mdg context <path>` for scoped authority. Evidence under `.authority/evidence/` is audit-only; read it through `mdg explain` only when troubleshooting.

Keep successful Gatekeeper bootstrap and housekeeping silent. Do not narrate Skill activation, authority loading, internal commands, run IDs, reviewer selection, or fallback mechanics. If a host requires a progress update, combine this work with the user's actual task in one short outcome-oriented sentence. Mention Gatekeeper only when it blocks the task, needs owner judgment, detects an integrity failure, or the user explicitly asks about its status.

The LLM is a reviewer. The deterministic publisher controls current authority. Low-confidence product conflicts require the human owner.
<!-- markdown-gatekeeper:managed:end -->
