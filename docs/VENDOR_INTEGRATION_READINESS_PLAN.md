# Saccade Vendor Integration Readiness Plan

Date: 2026-07-11
Status: execution plan

## Objective

Convince an AI product team that it can add Saccade to an existing agent without
giving the model unrestricted browser access or asking users to surrender their
normal browser session. The first integration target is a local, current-tab
tool for an existing agent product. An embedded browser runtime is a later
option.

Saccade must prove four things:

1. It completes a valuable class of work better than screenshot-and-click
   automation.
2. It keeps page instructions, secrets, and side effects inside explicit
   boundaries.
3. It gives the vendor enough evidence to investigate any action or failure.
4. It can ship as a small, versioned integration rather than a bespoke fork of
   the vendor's browser product.

## Integration Paths

| Path | What the vendor integrates | Value | Status |
| --- | --- | --- | --- |
| A. Local tool | Saccade MCP server plus a user-granted current-tab bridge | Fastest trial. The agent receives redacted truth, field inventory, verified safe actions, and replay artifacts. | Primary path |
| B. Computer-use companion | Vendor computer-use agent calls Saccade for form inspection, deterministic fill, and action verification while it keeps its own model/browser loop. | Reduces repeated screenshots and isolates sensitive fields. | Design after A is proven |
| C. Embedded runtime | Vendor embeds the Saccade/ServoShell control protocol behind its own browser product. | Lowest latency and strongest engine-level control. | Long-term; no commitment before A/B demand |

We do not ask a vendor to replace Chrome, Atlas, Chromium, or its existing
computer-use model. Saccade earns adoption as the browser control and safety
layer for workflows where its proof is stronger: long forms, PDF forms, mixed
human/agent completion, and verified action replay.

## What Exists Today

| Capability | Evidence | Adoption value |
| --- | --- | --- |
| Same visible-tab handoff | Current-tab grant plus ServoShell and Chrome compatibility bridges | The person and agent operate on one page state. |
| Redacted truth and field ownership | Form inventory hides sensitive values and preserves user-owned fields | A model need not receive passwords, SSNs, payment data, or user-entered values. |
| Deterministic ordinary-field fill | FORMMAX fixture fills 672 ordinary fields across 96 rows and two pages with verified receipts | Bulk work does not require one model turn per input. |
| Side-effect boundary | Submit, publish, payment, delete, login, OTP, signing, and similar actions stay user-owned | The agent can prepare work without silently committing it. |
| Local capability boundary | AI-033 gives each bridge session a short local bearer capability; grants are owner-only and reports omit the token | A random local process cannot issue browser actions by knowing a port. |
| Prompt-injection provenance | Page text and labels are marked untrusted; page content cannot authorize side effects | The browser distinguishes web content from Saccade policy. |
| Replay and block reports | Control reports and replay stay value-free | Security and reliability teams can audit an action without collecting secrets. |
| Measured compatibility fallback | A user-granted Chrome compatibility route exists for measured Servo blockers | Saccade can preserve its contract when an upstream engine limitation blocks a real site. |

These are internal dogfood claims. They become vendor claims only after the
evaluation and integration gates below pass.

## Required Work

### V1. Finish the useful wedge: forms and PDFs

Owner: current product line

- Complete AI-031 with human-reviewed live drafts on at least two ordinary
  forms. Measure task completion, correction count, elapsed time, token use,
  field rejection reasons, and value leaks.
- Complete AI-032 for AcroForm inspect, fill-to-copy, render verification, and
  redacted diff. Classify flat, scanned, encrypted, signed, and XFA documents
  instead of pretending to fill them.
- Include custom controls, virtualized rows, date/select/radio controls, and
  partial-failure repair in the fixture suite.

Done when: a vendor can run one command that produces a draft, a concise
user-review list, and a value-free replay for web and AcroForm tasks.

### V2. Turn AI-033 into a vendor-grade security case

Owner: agent safety

- Run an AgentDojo-relevant subset and publish the exact adapter, tasks,
  model settings, and raw aggregate results. AgentDojo provides 97 realistic
  tasks and 629 security test cases, but Saccade must not claim its score until
  its browser-control adapter runs them. [AgentDojo](https://github.com/ethz-spylab/agentdojo)
- Add Saccade adversarial fixtures for shadow DOM/custom controls, visual and
  semantic sensitive-field evasion, cross-tab capability misuse, stale
  confirmation after navigation, malicious page labels, and benign controls
  that must not be falsely blocked.
- Report four measures for each release: task utility, attack success rate,
  false-block rate, and protected-value leaks. A protected-value leak blocks
  release.
- Add an explicit trusted confirmation object that a host product can render
  without trusting page prose. It must bind the action to origin, tab, page
  revision, scope, expiry, and a user gesture.
- Map the local capability protocol to standard remote deployment: OAuth 2.1,
  resource audience binding, short-lived tokens, and no token in a URL. MCP's
  current authorization specification requires bearer validation for every
  request and prohibits URI token transport. [MCP authorization](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)

Done when: an independent reviewer can reproduce the suite, inspect every
block, and confirm zero protected-value leaks in the published artifacts.

### V3. Stabilize the integration contract

Owner: bridge/MCP

- Publish a versioned capability schema for grants, truth, field inventory,
  fill plans, action receipts, confirmation requests, block reports, and
  replay. Keep raw values, cookies, storage, and screenshot data out of the
  default contract.
- Publish a host SDK example for TypeScript and Python. Each example must
  attach to one visible tab, grant the agent bounded scopes, fill an ordinary
  form, pause for human completion, and read the replay.
- Define compatibility rules: protocol version negotiation, feature discovery,
  structured error codes, timeouts, cancellation, and graceful bridge
  shutdown.
- Add cross-platform release work: signed/notarized macOS app or installer,
  checksum, SBOM, dependency/license inventory, and reproducible release
  commands.

Done when: an engineer at another company can integrate a local proof of
concept in one day without modifying Saccade source or Servo source.

### V4. Establish reliability on relevant workloads

Owner: evaluation

- Freeze MOUSEMAX as a low-latency substrate proof, including the exact engine,
  machine, input path, and limits. It is supporting evidence, not the product
  benchmark.
- Run the UI torture suite, public read matrix, FORMMAX, DOCMAX, and a
  WorkArena/WebArena-like self-hosted suite. Use Chrome compatibility only when
  a recorded engine blocker requires it. Do not bury routing in aggregate
  scores.
- Compare Saccade with a screenshot/browser baseline on the same tasks:
  success, verified actions, wall time, model tokens, retries, user
  corrections, blocks, and leaks. A browser-use benchmark alone does not make
  this comparison because its agents, browsers, models, and task setup differ.
- Keep site-policy results honest. CAPTCHA, anti-bot blocks, authentication,
  payments, legal filings, signing, security changes, and final publish remain
  explicit human or provider boundaries.

Done when: the report gives per-task evidence and explains every unsupported,
blocked, or fallback result.

### V5. Prove the human product, not only the API

Owner: dogfood

- Ship one polished dogfood release with persistent normal/incognito profiles,
  a visible browser-mode indicator, stable current-tab grant, and recovery
  after tab close, reload, navigation, and login expiry.
- Test a small pilot with real builders and operators. Each person brings one
  active form or document workflow. Capture consented task telemetry without
  values, then interview them about corrections and trust.
- Keep a separate measured compatibility ledger for upstream Servo gaps. A
  known limitation with a clear fallback is safer than a claim of universal
  compatibility.

Done when: ten users complete real, non-sensitive draft workflows and the team
can show where Saccade saved time, where the human intervened, and where it
failed.

### V6. Prepare the vendor decision package

Owner: product and founder

- One-page architecture: host product, Saccade bridge, browser, user, model,
  trust boundaries, and data flows.
- Security paper: threat model, residual risks, source of authority, redaction
  rules, capability lifecycle, confirmation protocol, test coverage, and
  incident/revocation path.
- Evaluation report: task corpus, baseline configuration, raw artifacts,
  summary tables, limitations, and rerun instructions.
- Integration guide: SDK/API examples, scopes, supported engines, deployment
  choices, compatibility fallback behavior, support expectations, and license.
- Commercial brief: hosted support versus on-device deployment, expected
  integration effort, pricing hypothesis, and a design-partner proposal.

Done when: a vendor can answer product, security, procurement, and platform
engineering questions from the package without a bespoke presentation.

## Order of Work

| Order | Work | Reason |
| --- | --- | --- |
| 1 | AI-033 adversarial suite and confirmation protocol | Safety claims must survive hostile web content before wider dogfood. |
| 2 | AI-031 live form drafts | Forms give Saccade a concrete, repeated user benefit. |
| 3 | AI-032 AcroForm product path | PDFs widen the same ownership model without changing the core promise. |
| 4 | V3 protocol/SDK/release contract | A vendor cannot evaluate an undocumented local bridge. |
| 5 | V4 benchmark report | The comparison must measure value, tokens, reliability, and limits. |
| 6 | V5 ten-user pilot | Real correction and trust data determines whether the wedge is real. |
| 7 | V6 vendor package and design partners | Outreach begins after the evidence can survive scrutiny. |

## Work We Will Not Prioritize First

- building a full Chrome replacement;
- defeating CAPTCHAs, anti-bot challenges, or provider access controls;
- automatic payment, signing, publish, delete, release, credential, OTP, or
  identity actions;
- broad claims that every website works in Servo;
- a permanent Servo fork before the local-tool adoption path proves demand.

## Why This Fits the Market

Google's Computer Use documentation says the capability should receive close
supervision for sensitive or consequential work. Anthropic documents that the
host application owns safety checks around tool execution. Saccade provides the
missing host-side layer: bounded browser authority, redacted state, deterministic
ordinary-field execution, reviewable receipts, and a trusted confirmation object.

This is a complement to frontier browser models, not a claim that a browser
model alone is inadequate. The integration pitch is simple: use the vendor's
model for reasoning, use Saccade for the tab's authority, sensitive-field
boundary, deterministic batch work, and evidence.

Sources:

- [MCP authorization specification](https://modelcontextprotocol.io/specification/2025-06-18/basic/authorization)
- [AgentDojo benchmark](https://github.com/ethz-spylab/agentdojo)
- [Gemini Computer Use safety guidance](https://ai.google.dev/gemini-api/docs/computer-use)
- [Anthropic tool host responsibility](https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/bash-tool)
