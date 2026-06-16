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
- Side-effect actions require user confirmation or manual user action.
- The site policy and risk profile do not prohibit automation.

## Risk Levels

| Level | Default behavior | Good examples | Saccade may do | Saccade must not do |
| --- | --- | --- | --- | --- |
| Green | Run by default | Local dev apps, docs, blogs, public product pages, local fixtures, public search/research pages | Read truth, detect actions, click low-risk controls, fill non-sensitive test/forms, produce replay | None beyond normal rate/robots/terms respect |
| Yellow | Human-in-loop | Logged-in low-risk dashboards, GitHub/Gist drafts, internal tools, forum/comment drafts, ordinary forms without legal/financial impact | Assist after explicit grant, draft text, fill non-sensitive fields, verify UI, stop before submit | Read secrets, publish/delete/submit without user confirmation |
| Orange | Assisted fallback | App Store Connect, cloud consoles, app review portals, tax/benefit/government forms, healthcare/education portals, job/marketplace/social reputation flows | Summarize copied/redacted text, make checklists, draft responses, explain UI, prepare non-sensitive content | Login automation, high-impact submit/release/payment/security changes |
| Red | No agent automation | Login, password, 2FA/passkey, CAPTCHA, account recovery, banking transfer, tax payment, trading, legal signature, credential/API-key/security settings | Tell the user why it is blocked and what safe fallback to use | Circumvent anti-bot/fraud controls, collect credentials, enter OTPs, click final confirmation |

## Site Classes

| Site class | Initial classification | Notes |
| --- | --- | --- |
| Local development apps | Green | Primary dogfood lane. Use Saccade first, then Chrome/reference for parity. |
| Public documentation/news/blogs | Green | Good for research, summaries, action-map tests, and rendering checks. |
| MouseAccuracy / public demos | Green/Yellow | Safe for performance demos unless ads/iframes or anti-bot overlays change the page. |
| GitHub/Gist/forums | Yellow | Drafting and editor assistance are okay. Publishing, deleting, mass posting, or scraping is confirmation-gated. |
| App Store Connect | Orange | Browser access is for app management, review, agreements, and financial/in-app purchase work. Use Saccade for redacted analysis only; submit/reply/release stays human. |
| Google/Microsoft/Apple account login | Red | Authentication, recovery, account security, and 2FA are human-only. Saccade may resume after explicit handoff if the page and action are low-risk. |
| Login.gov / IRS / SSA / USCIS / DMV | Green for public info, Orange/Red for accounts/forms | Public pages are okay. Authenticated identity proofing, tax/benefit forms, payments, signatures, and submissions are human-only or confirmation-gated. |
| Banking, credit cards, brokerage, crypto, payroll | Orange/Red | Summaries/checklists are okay from user-provided redacted text. Transactions, payments, trades, withdrawals, and security changes are human-only. |
| Healthcare portals | Orange/Red | Appointment or instruction summaries may be okay with explicit user consent. Diagnosis, medical advice, prescription, insurance, billing, and PHI-heavy flows are not default automation. |
| Cloud consoles and production admin | Orange/Red | Read/status can be assisted. Destructive ops, billing, IAM, credentials, deploy/release, and security settings need human confirmation or manual action. |
| Shopping/travel | Green/Yellow until checkout, Red at payment | Research and comparison are fine. Checkout, payment, cancellation, refund, and booking confirmation are human-only. |
| Social networks / marketplaces | Yellow/Orange | Drafts are okay. Posting, messaging at scale, reputation-impacting actions, reviews, and moderation need confirmation. |
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
5. Saccade/agent drafts, checks, summarizes, or prepares the next non-sensitive
   step.
6. User performs final submit/release/payment/signature/security actions.

## Immediate Product Work

| ID | Priority | Work | Done when |
| --- | --- | --- | --- |
| SP-001 | P0 | DONE: Add a site-risk classifier to the bridge/MCP layer. | Current URL/action gets `green/yellow/orange/red` and a human-readable reason. Evidence: `runs/mcp/selftest_1781641440418/report.json`. |
| SP-002 | P0 | DONE: Add high-risk action gating. | Login, OTP, password, CAPTCHA, payment, submit, release, delete, sign, API key, and security actions return `requires_user`. Evidence: `saccade_core::site_policy` unit tests plus MCP selftest. |
| SP-003 | P1 | DONE: Add block evidence artifact. | Blocked bridge control runs write a redacted `control/block_report.json` with URL, class, error text, request id, and fallback recommendation. Evidence: `cargo test -p saccade-servoshell block_report`. |
| SP-004 | P1 | Add a user-facing fallback copy path. | User can paste redacted text into a local Saccade note and get analysis without exposing the live high-risk page. |
| SP-005 | P1 | Add allowlist lanes for owned/local apps. | Localhost, file fixtures, and explicitly owned domains can run Green gates without repeated prompts. |
| SP-006 | P2 | Add policy docs to the handoff prompt for other sessions. | Other Codex sessions know when to use Saccade and when to fall back. |

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
