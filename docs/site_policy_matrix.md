# Saccade Site Policy Matrix

Date: 2026-06-16
Status: draft product boundary

This is the practical list for deciding whether Saccade should run a site,
assist the user, or fall back to a normal human browser flow.

Saccade is not an anti-bot bypass tool. If a site blocks automated or unusual
browsers, the product response is to record the block, classify it, and route to
a safe fallback rather than hiding automation or fighting the site.

## Default Rule

Saccade may help when all of these are true:

- The user grants the current tab/session.
- Agent truth is redacted before it leaves the browser boundary.
- Sensitive user-owned values stay hidden from the agent.
- The user's task authorizes the ordinary actions needed to complete its goal.
- Only highest-risk actions require renewed user confirmation or manual user
  action.
- The site policy and risk profile do not prohibit automation.

## Evidence Gate

Do not add or promote a site-specific Green/Yellow/Orange/Red rule by
intuition. A concrete site classification belongs in docs or code only after at
least one of these exists:

- A real Saccade/ServoShell dogfood run on that site, with artifact paths,
  observed URL, date, result, and what the agent did or refused to do.
- A reference-browser comparison showing that the issue is site/upstream
  compatibility rather than a Saccade-specific bug.
- Primary-source product/terms evidence that the surface is inherently
  high-impact, such as authentication, app release, payment, security,
  government identity, healthcare, or account recovery.

Unknown third-party sites are `unmeasured_unknown` Yellow by default. Saccade may
assist after a Human grant and complete task-authorized ordinary actions;
screenshots are not default-allowed and highest-risk boundaries remain
Human-confirmed. Promote a site to Green only after
successful low-risk dogfood evidence. Move a site to Orange/Red only after
observed risk, an explicit provider block, or primary-source high-impact proof.

## Risk Levels

| Level | Default behavior | Good examples | Saccade may do | Saccade must not do |
| --- | --- | --- | --- | --- |
| Green | Run by default only after evidence or ownership | Local dev apps, known-safe docs, owned public pages, local fixtures | Read truth, detect actions, click low-risk controls, fill non-sensitive test/forms, produce replay | None beyond normal rate/robots/terms respect |
| Yellow | Task-autonomous, screenshot-conservative | Unmeasured third-party sites, logged-in low-risk dashboards, GitHub/Gist, internal tools, forum/comment flows, ordinary forms without legal/financial impact | Complete task-authorized ordinary actions, including submit/publish/send, after explicit grant | Read secrets or cross a highest-risk boundary without the user |
| Orange | Task-autonomous with precise high-risk gates | App Store Connect, cloud consoles, app review portals, tax/benefit/government forms, healthcare/education portals, job/marketplace/social reputation flows | Complete ordinary task-authorized reads, fields, saves, navigation, and submissions | Payment/financial transfer, legal attestation, authentication/account-security change, irreversible deletion, or production release without the user |
| Red | No agent automation | Login, password, 2FA/passkey, CAPTCHA, account recovery, banking transfer, tax payment, trading, legal signature, credential/API-key/security settings | Tell the user why it is blocked and what safe fallback to use | Circumvent anti-bot/fraud controls, collect credentials, enter OTPs, click final confirmation |

## Site Classes

| Site class | Initial classification | Notes |
| --- | --- | --- |
| Local development apps | Green | Primary dogfood lane. Use Saccade first, then Chrome/reference for parity. |
| Unmeasured third-party sites | Yellow | Default until dogfood evidence or primary-source risk evidence says otherwise. Do not promote by guesswork. |
| Public documentation/news/blogs | Green after evidence | Good for research, summaries, action-map tests, and rendering checks after a smoke run or known-safe classification. |
| MouseAccuracy / public demos | Green/Yellow | Safe for performance demos unless ads/iframes or anti-bot overlays change the page. |
| GitHub/Gist/forums | Yellow | Task-authorized publishing and messaging are ordinary actions. Irreversible account deletion, payment, and security changes stay confirmation-gated. |
| App Store Connect | Orange | Ordinary metadata, save, reply, and task-authorized submission may proceed. Agreements, financial changes, and production release stay human-confirmed. |
| Google/Microsoft/Apple account login | Red | Authentication, recovery, account security, and 2FA are human-only. Saccade may resume after explicit handoff if the page and action are low-risk. |
| Login.gov / IRS / SSA / USCIS / DMV | Green for public info, Orange/Red for accounts/forms | Public pages and ordinary task-authorized form steps are okay. Authentication, identity proofing, payments, and legal attestations are human-confirmed. |
| Banking, credit cards, brokerage, crypto, payroll | Orange/Red | Summaries/checklists are okay from user-provided redacted text. Transactions, payments, trades, withdrawals, and security changes are human-only. |
| Healthcare portals | Orange/Red | Appointment or instruction summaries may be okay with explicit user consent. Diagnosis, medical advice, prescription, insurance, billing, and PHI-heavy flows are not default automation. |
| Cloud consoles and production admin | Orange/Red | Read/status can be assisted. Destructive ops, billing, IAM, credentials, deploy/release, and security settings need human confirmation or manual action. |
| Shopping/travel | Green/Yellow until checkout, Red at payment | Research and comparison are fine. Checkout, payment, cancellation, refund, and booking confirmation are human-only. |
| Social networks / marketplaces | Yellow/Orange | Task-authorized posting and ordinary messaging may proceed. Payment, security, irreversible deletion, and genuinely high-impact moderation remain gated. |
| Sites with explicit anti-automation blocks | Orange/Red | Record the block and use fallback. Do not add stealth or bypass behavior. |

## Fallback Protocol

When Saccade is blocked or the site is high-risk:

1. Record the URL, classification, visible error text, request ID if present,
   and whether sensitive content was visible.
2. Do not take screenshots if sensitive data is visible.
3. Ask the user to complete login, 2FA, CAPTCHA, or high-risk steps in Safari,
   Chrome, or the official app.
4. User provides redacted non-sensitive text or a local redacted note when they
   want agent help.
5. Saccade/agent completes all task-authorized ordinary steps.
6. The user performs only the precise highest-risk payment, legal attestation,
   authentication/account-security, irreversible-deletion, or production-release
   action.

## Immediate Product Work

| ID | Priority | Work | Done when |
| --- | --- | --- | --- |
| SP-001 | P0 | DONE: Add a site-risk classifier to the bridge/MCP layer. | Current URL/action gets `green/yellow/orange/red` and a human-readable reason. Evidence: `runs/mcp/selftest_1781641440418/report.json`. |
| SP-002 | P0 | DONE: Add precise highest-risk action gating. | Authentication secrets/account-security ownership changes, payment/financial transfer, legal attestation, irreversible deletion/account closure, and production release return `requires_user`; generic submit/save/send/publish do not. Evidence: `saccade_core::site_policy` unit tests plus MCP initialization tests. |
| SP-003 | P1 | DONE: Add block evidence artifact. | Blocked bridge control runs write a redacted `control/block_report.json` with URL, class, error text, request id, and fallback recommendation. Evidence: `cargo test -p saccade-servoshell block_report`. |
| SP-004 | P1 | DONE: Add a user-facing fallback copy path. | `saccade.report.redacted_note` creates a local `runs/redacted_notes/note_*/` AI review packet from user-supplied redacted text without live-site access. Evidence: `runs/mcp/selftest_1781645696687/report.json`. |
| SP-005 | P1 | DONE: Add allowlist lanes for owned/local apps. | Localhost/file are Green by default; `SACCADE_OWNED_DOMAINS=nanmesh.ai,mythcastera.com` marks owned non-high-risk domains as `owned_domain` Green without overriding auth/financial/government/high-risk classes. Evidence: `cargo test -p saccade_core owned_domains`. |
| SP-006 | P2 | DONE: Add policy docs to the handoff prompt for other sessions. | `docs/SACCADE_DOGFOOD_HANDOFF.md` includes a paste-ready prompt for other sessions covering Saccade use, risk levels, fallback, and `saccade.report.redacted_note`. |
| SP-007 | P0 | DONE: Add evidence-first policy gate. | Unknown third-party sites classify as `unmeasured_unknown` Yellow; site-specific policy changes require dogfood artifacts, reference comparison, provider block evidence, or primary-source high-impact proof. Evidence: `cargo test -p saccade_core site_policy`. |

## Sources Checked

- W3C WebDriver defines browser remote control and introspection:
  https://www.w3.org/TR/webdriver2/
- Login.gov Rules of Use explicitly prohibit automated access:
  https://www.login.gov/policy/rules-of-use/
- IRS online accounts contain tax records, balances, payments, forms, and
  identity verification:
  https://www.irs.gov/payments/online-account-for-individuals
- Apple describes App Store Connect as app management, submission, review,
  agreements, and financial/in-app purchase tooling:
  https://developer.apple.com/support/app-store/
- GitHub Terms warn against abuse/excessive API usage and token/rate-limit
  misuse:
  https://docs.github.com/en/site-policy/github-terms/github-terms-of-service
- FTC consumer guidance treats email, banking, tax, payment, and social accounts
  as sensitive accounts where MFA matters:
  https://consumer.ftc.gov/articles/use-two-factor-authentication-protect-your-accounts
