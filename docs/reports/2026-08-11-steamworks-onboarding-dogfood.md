# Steamworks authenticated-onboarding dogfood

Date: 2026-08-11
Status: engineering evidence; not release or compatibility certification

## Objective

Exercise a real, authenticated, multi-page business workflow in ordinary
Chrome while preserving the production boundary:

The required loop is **Saccade-observe / Agent-act / Saccade-verify**.

- Saccade opens the authorized tab and supplies current semantic Truth.
- The Codex client's own same-tab browser tool performs page operations.
- Saccade observes and verifies the resulting page transitions.
- No site-specific selector, execution route, or Steamworks integration is
  added to Saccade.

The retained report is deliberately sanitized. It contains no password, SSN,
EIN, mailing address, telephone number, personal email address, confirmation
token, cookie, or browser-storage value.

## Completed evidence

| Workflow phase | Truth evidence | Outcome |
| --- | --- | --- |
| Partner onboarding | Saccade projected ordinary company and agreement fields, their state, affordances, and current bounds across the authenticated flow. | pass |
| Mailing address | The Agent completed ordinary fields and Saccade observed the saved post-operation state. | pass |
| Account permissions | Saccade projected the current user, communication-permission control, explanatory text, and resulting status. | pass |
| Long dashboard and navigation | Saccade supplied bounded semantic objects without transferring DOM locators or editable values. | pass |
| Confirmation consent | Saccade exposed the consent controls and the site's response after the Agent acted. | pass for observation; site rejected the confirmation because the signed-in account did not match the invite |

## Truthful boundaries encountered

- CAPTCHA remained a human/website authorization boundary. Saccade did not
  invent a bypass or an alternate execution route.
- A cross-origin Google account selector was represented as a restricted
  frame. This was a truthful visibility limitation, not silently flattened or
  guessed.
- Steamworks returned an account-mismatch error after confirmation. Saccade
  reported that server response; the workflow was not falsely marked
  successful.
- The user stopped before Steam Direct payment and app registration. No fee
  was paid and no app was created.
- The one-time confirmation URL was supplied by the user to the Agent client.
  Its secret query material was not projected into Truth and is not retained
  in this artifact.

## What this proves

- An ordinary signed-in Chrome session can support a real Saccade-observe /
  Agent-act / Saccade-verify loop across multiple pages.
- Complex forms, saved state, dialogs, long pages, permissions, and explicit
  server errors can be handled without adding a Saccade execution surface.
- Restricted cross-origin content and external account state remain explicit
  rather than becoming fabricated semantic certainty.
- The default autonomous Profile can complete independent ordinary work and
  continue until a genuine human, secret, payment, or site-account boundary is
  reached.

## What this does not prove

- It does not certify Steamworks, Google, or any other site as supported.
- It does not promote any Control Catalog row to `publishable`.
- It does not replace the same-candidate Chrome and Edge release gates.
- It does not prove CAPTCHA, password, tax, banking, payment, publication, or
  contract-acceptance automation.
- It does not make account mismatch a Saccade defect; the observed site state
  is the evidence.

## Non-regression acceptance

Future authenticated dogfood is accepted only when all of the following hold:

1. Saccade is the observation route and the Agent client's same-tab tool owns
   execution.
2. Verification comes from a newer truthful observation, not from assuming an
   operation succeeded.
3. Password, SSN, EIN, one-time URL secrets, cookies, browser storage, and
   editable values are absent from retained Truth and evidence artifacts.
4. Restricted frames, CAPTCHA, login/account mismatch, payment, and other
   external boundaries are reported explicitly.
5. No site-specific selector, DOM path, coordinate actuator, browser fallback,
   or execution command is added to core Saccade.
6. Single-browser dogfood remains engineering evidence until the frozen
   candidate passes the independent Chrome and Edge release gates.

This evidence does not promote any Control Catalog row to `publishable`.
