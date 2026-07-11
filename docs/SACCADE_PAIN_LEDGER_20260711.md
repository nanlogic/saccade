# Saccade Pain Ledger

Date: 2026-07-11
Status: product-priority research

## Decision

Saccade should attack work that is painful for both the user and the agent. The
best initial target is not general browsing. It is completing long, consequential
web and PDF forms while the user keeps sensitive values and final authority.

The next product sequence is:

1. generic FORMMAX planning, fast fill, verification, and recovery;
2. sensitive-field and untrusted-content policy enforced outside the LLM;
3. DOCMAX classification and AcroForm inspect/fill/verify;
4. robust grounding for unlabeled, custom, dynamic, and paginated controls;
5. persistent human login and agent handoff with explicit tab grants.

CAPTCHA bypass, stealth browsing, and universal website compatibility are not
the first product. They consume large effort, have weak safety boundaries, and
do not use Saccade's strongest advantages.

## What the evidence says

### Human pain

| Pain | Evidence | What it means |
| --- | --- | --- |
| Long and repetitive forms | U.S. federal information collections accounted for an estimated 10.34 billion paperwork-burden hours in FY2022. This includes recordkeeping and research, not only typing into forms. Baymard found that 17% of U.S. shoppers had abandoned an order because checkout felt too long or complicated; its measured checkout averaged 23.48 visible form elements. | Repeated collection and form completion consume meaningful time. A narrow fast-fill product has a large task pool. |
| Broken or confusing controls | WebAIM's 2025 scan of one million home pages found detectable WCAG failures on 94.8% of pages. It found missing input labels on 48.2% of home pages, and 34.2% of all form inputs lacked a proper programmatic label. | Users struggle with unclear forms. Agents lose the label-to-control mapping they need for reliable action. |
| Dynamic interfaces and CAPTCHA | In WebAIM's 2024 survey of 1,539 screen-reader users, CAPTCHA ranked as the most problematic item. Menus, tabs, and dialogs that behave unexpectedly ranked second; unexpected screen changes ranked fourth; complex forms ranked seventh. The sample was voluntary and does not represent every web user. | A stable fact/action layer can help with dynamic controls. CAPTCHA still requires an explicit human handoff. |
| Inaccessible PDFs | A study of 11,397 scientific PDFs published from 2010-2019 found that only 2.4% met all five tested Adobe accessibility criteria. This corpus does not measure fillable administrative PDFs, but it quantifies how often PDF structure fails readers. | DOCMAX needs classification, structure recovery, and honest routing in addition to form filling. |
| Sensitive data and loss of control | Pew's 2023 U.S. survey found 81% concerned about how companies use collected data, 73% feeling little or no control over company collection, and 79% feeling little or no control over government collection. | Sending complete page state, credentials, IDs, or payment details to an agent conflicts with a common user concern. |
| Login and password burden | Pew found about seven in ten Americans overwhelmed by the number of passwords they must remember, and 45% anxious about password strength. A 2026 university study covering 2,559 Duo users found 4.35% of authentication attempts failed because the Duo task was incomplete; 43.86% of 57 surveyed participants reported at least one Duo login failure. | The user should authenticate in the visible browser once. The agent should inherit authorized session state without receiving credentials or cookies. |
| High-stress government and benefit workflows | The U.S. Web Design System warns that people often apply during displacement, unemployment, illness, bereavement, discrimination, or violence, and recommends save/resume for long forms. | Error recovery, preserved progress, plain handoff, and review matter as much as raw fill speed. |

### Agent pain

| Pain | Quantitative evidence | What it means |
| --- | --- | --- |
| End-to-end reliability | OpenAI reported CUA success of 58.1% on WebArena versus 78.2% for humans, and 38.1% on OSWorld versus 72.4% for humans. The live 300-task, 136-site Online Mind2Web leaderboard currently shows a best verified result of 42.33%. Results improve with models, but the task gap remains. | A product cannot treat a plausible action trace as task completion. It needs receipts and postcondition checks. |
| GUI grounding and motor errors | OSWorld's failure analysis found mouse-click inaccuracies in more than 75% of 550 failed examples. It also identified repetitive clicks, pop-ups, ads, selecting, and scrolling as common problems. | Stable action IDs, hit-test evidence, page revisions, and deterministic control executors can remove work from the model loop. |
| Open-web interruptions | BrowserArena identified CAPTCHA resolution, pop-up removal, and direct URL navigation as three consistent live-web failure modes. Open CaptchaWorld reported at most 40.0% success for the tested agent, versus 93.3% for humans. | Human handoff and honest blockers are required. Saccade should not claim that an agent can or should bypass CAPTCHA. |
| Large noisy observations | FocusAgent matched strong baselines on WorkArena and WebArena while reducing observation size by more than 50%. | Compact field inventories and state diffs can reduce context cost without hiding task-relevant facts. Saccade still needs a controlled A/B before making a token claim. |
| Prompt injection and excessive authority | AgentDojo contains 97 realistic tasks and 629 security cases. InjecAgent contains 1,054 cases across 17 user tools and 62 attacker tools; its ReAct-prompted GPT-4 was vulnerable 24% of the time. OWASP lists prompt injection, data exposure, tool abuse, and excessive autonomy as agent risks. | Prompt rules alone are not a security boundary. Field ownership, source labels, capability scope, and side-effect checks belong in code. |
| Authentication and delegation | OpenAI's computer-use product asks the user to handle sensitive actions such as login details and CAPTCHA. Research on authenticated delegation argues for agent-specific credentials and scoped permissions. | Human login plus a bounded tab capability is a normal product requirement, not an edge case. |

Sources:

- [Congressional Research Service: Paperwork Reduction Act](https://www.congress.gov/crs_external_products/IF/HTML/IF11837.web.html)
- [Baymard checkout abandonment research](https://baymard.com/blog/ecommerce-checkout-usability-report-and-benchmark)
- [WebAIM Million 2025](https://webaim.org/projects/million/2025)
- [WebAIM Screen Reader User Survey 2024](https://webaim.org/projects/screenreadersurvey10/)
- [PDF accessibility study](https://papertohtml.org/paper?id=5269622e79a98df2fbb6f788f16f1c06aa692708)
- [Pew Research: data privacy](https://www.pewresearch.org/internet/2023/10/18/views-of-data-privacy-risks-personal-data-and-digital-privacy-laws/)
- [Pew Research: privacy and password findings](https://www.pewresearch.org/short-reads/2023/10/18/key-findings-about-americans-and-data-privacy/)
- [DuoLungo MFA usability study](https://arxiv.org/abs/2602.01489)
- [USWDS: complete a complex form](https://designsystem.digital.gov/patterns/complete-a-complex-form/progress-easily/)
- [OpenAI computer-using agent evaluation](https://openai.com/index/computer-using-agent/)
- [OSWorld paper](https://proceedings.neurips.cc/paper_files/paper/2024/file/5d413e48f84dc61244b6be550f1cd8f5-Paper-Datasets_and_Benchmarks_Track.pdf)
- [BrowserArena](https://arxiv.org/abs/2510.02418)
- [Open CaptchaWorld](https://arxiv.org/abs/2505.24878)
- [FocusAgent](https://openreview.net/forum?id=mINaJKSy7A)
- [AgentDojo](https://proceedings.neurips.cc/paper_files/paper/2024/hash/97091a5177d8dc64b1da8bf3e1f6fb54-Abstract-Datasets_and_Benchmarks_Track.html)
- [InjecAgent](https://aclanthology.org/2024.findings-acl.624/)
- [Online Mind2Web leaderboard](https://hal.cs.princeton.edu/online_mind2web)
- [OWASP AI Agent Security Cheat Sheet](https://cheatsheetseries.owasp.org/cheatsheets/AI_Agent_Security_Cheat_Sheet.html)
- [Authenticated delegation for AI agents](https://openreview.net/forum?id=9skHxuHyM4)

## Ranked overlap

This score orders development. It is not a prevalence estimate. Each dimension
uses a 1-5 judgment based on the evidence above and current Saccade artifacts.

`priority = 30% user cost + 25% agent block + 30% Saccade leverage + 15% evidence confidence`

| Rank | Shared pain | User | Agent | Leverage | Evidence | Priority | Current Saccade state |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | Long repeated multi-page forms | 5 | 5 | 5 | 5 | 100 | Local 96-row fixture passes; generic and real-form planning remain. |
| 2 | Sensitive values mixed with ordinary fields | 5 | 5 | 5 | 5 | 100 | Field masking and human ownership pass local and bounded live gates; custom-control coverage remains. |
| 3 | False success, lost progress, and poor recovery | 5 | 5 | 5 | 4 | 97 | Receipts and replay exist; generic field postconditions, save/resume, and repair plans remain. |
| 4 | Prompt injection plus overpowered actions | 5 | 5 | 4 | 5 | 94 | Side-effect gates exist; source-taint labels, per-session control secret, and AgentDojo runs remain. |
| 5 | Unlabeled, custom, dynamic, and off-screen controls | 4 | 5 | 4 | 5 | 88 | Action maps/native input exist; real dropdown, overlay, shadow DOM, virtualization, and resize coverage remain. |
| 6 | PDF types that look alike but behave differently | 4 | 4 | 5 | 4 | 86 | AcroForm feasibility passes; product CLI, rendering verification, public blank form, XFA/scanned routing remain. |
| 7 | Login, MFA, session continuity, and delegation | 4 | 5 | 3 | 5 | 82 | Same-session handoff and persistent profiles exist; recovery and broader real-site evidence remain. |
| 8 | Noisy pages and oversized model context | 3 | 4 | 5 | 4 | 80 | Article extraction and compact truth exist; form-state delta and token A/B remain. |
| 9 | Browser/rendering incompatibility and anti-bot blocks | 3 | 5 | 2 | 5 | 70 | Explicit Chrome route exists. Engine routing should stay honest and measured. |
| 10 | CAPTCHA | 3 | 5 | 1 | 5 | 64 | Human-only handoff. Do not build bypass or stealth claims. |

## Attack plan

### P0: finish the form product

1. **Compile once.** Inspect the page once and emit stable field IDs, labels,
   types, options, constraints, ownership, sensitivity, and page revision.
2. **Execute without repeated LLM turns.** Fill eligible controls through a
   deterministic plan covering scroll, virtual rows, pagination, selects,
   dates, radio groups, and validation messages.
3. **Verify each result.** Return `filled`, `preserved`, `blocked`, `rejected`,
   or `needs_review` from observed postconditions. Never infer success from a
   dispatched click.
4. **Repair safely.** Keep user-entered values, rerun only failed ordinary
   fields, and show the user the remaining steps.

Acceptance gate: one local adversarial fixture and two real forms, with field
success, wall time, actions, retries, user corrections, tokens, and zero raw
sensitive values in truth/log/replay/screenshots.

### P0: make the safety claim testable

- attach provenance to page instructions and retrieved document text;
- issue a per-session capability secret for the loopback control channel;
- enforce field ownership and side-effect policy outside the model;
- run a relevant AgentDojo subset and record utility, attack success, false
  blocks, and sensitive-value leaks;
- render confirmation text from trusted action metadata, not page-controlled
  prose.

### P0: ship DOCMAX AcroForm first

- classify AcroForm, flat, scanned, XFA, encrypted, and signed PDFs;
- inspect field metadata without protected values;
- fill a copy, preserve source and appearances, render affected pages, and
  verify internal field state;
- hand identity numbers, payment, signature, consent, and legal attestation to
  the user;
- return an explicit unsupported route instead of a damaged or misleading PDF.

### P1: improve the human-agent handoff

- keep normal profiles signed in across restarts without exposing cookies;
- let the user grant the current visible tab and revoke the grant;
- distinguish `Normal`, `Private`, and `Agent active` in visible browser UI;
- recover cleanly after a window closes, a login expires, or the page revision
  changes;
- preserve keyboard focus, cursor, dropdown, and resize behavior expected from
  a normal browser.

### Parked

- CAPTCHA solving or anti-bot evasion;
- universal Servo compatibility;
- replacing Acrobat as a general PDF editor;
- automatic signatures, legal attestations, payments, publishing, or submit;
- a broad browser assistant before the narrow form workflow passes externally.

## First-party statistics we still need

External evidence identifies the pain. It does not prove that Saccade solves it.
Run 30 observed tasks with at least 10 people before changing the product claim:

| Measurement | Record |
| --- | --- |
| Task mix | 10 long web forms, 10 sensitive handoff forms, 5 AcroForms, 5 unsupported/flat/scanned PDFs |
| Baseline | Manual browser/PDF workflow; Playwright or Chrome MCP where appropriate; plain pypdf for AcroForm |
| Outcome | Task completion and field-level completion |
| Cost | Wall time, LLM tokens, browser actions, retries, and user active time |
| Friction | Confusing controls, login interruptions, lost progress, corrections, and abandonments |
| Safety | Sensitive values exposed, false-sensitive blocks, side-effect attempts, and prompt-injection outcomes |
| Trust | Whether the user understood what the agent filled, what remained, and why it stopped |

Ship criteria for the first wedge:

- at least 90% ordinary-field completion on supported forms;
- zero protected-value leaks across all artifacts;
- at least 50% lower user active time than manual entry on long forms;
- no silent success when a field, page, or output rendering fails;
- at least 8 of 10 participants choose Saccade for a second comparable task.

These thresholds are product hypotheses. The study may reject them. Record the
result without lowering the gate after seeing the data.
