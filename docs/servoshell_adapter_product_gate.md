# ServoShell Adapter Product Gate

Date: 2026-06-14

## What "Product Gate" Means

A product gate is the minimum evidence required before a runtime becomes the
default Saccade dogfood/product path.

Saccade already has many gates. The ServoShell adapter does not invent a new
product from scratch; it must rerun the important existing gates on the new
official ServoShell runtime.

## Current Status

| Gate | Existing status on old Saccade path | Needed on official ServoShell adapter |
| --- | --- | --- |
| Browser session smoke | pass | pass via `saccade-servoshell selftest` |
| Redacted truth/action map | pass | first adapter bundle pass via `saccade-servoshell` |
| Safe field policy | pass | rerun with WebDriver truth/action extraction |
| Safety redaction | pass | 9-kind safety matrix pass via `saccade-servoshell selftest` |
| Login handoff | pass | rerun or decide it requires fork |
| FORMMAX live fill | pass | pass via official ServoShell adapter |
| Focused typing | pass | rerun via WebDriver keys/actions |
| Native dropdown/input | pass on embedded Servo | rerun; WebDriver may differ from native path |
| Replay artifacts | pass | same schema through adapter |
| Local game | old path problematic; official app manually ok | basic redacted truth pass on `http://127.0.0.1:4173/` |
| Screenshot policy | partial | pass for forbidden default + guarded sensitive preflight |
| Isolation | partial | first pass: random loopback port, temporary storage, clean teardown |
| Upgradeability | not tested | adapter must work on pinned official app and one newer build/nightly |

## Minimal Pass Set For Option A

The external ServoShell WebDriver adapter can remain the main path only if it
passes:

1. **Browser Smoke**
   - create session,
   - execute redacted truth JS,
   - dispatch one action,
   - verify post-truth changed.

2. **Safety Redaction**
   - password, token, email, hidden input, autofill-like, and contenteditable
     fixtures leak no raw values into truth, actions, logs, replay, or reports.

3. **Screenshot Policy**
   - default mode blocks screenshots,
   - guarded diagnostic mode runs sensitive-surface preflight,
   - sensitive visible surfaces block screenshot before capture.

4. **FORMMAX**
   - capacity fixture fills normal/agent fields,
   - sensitive fields are skipped,
   - replay logs no table values.

5. **Focused Typing**
   - non-sensitive focused field receives text,
   - sensitive focused field is blocked,
   - contenteditable path is handled or explicitly routed.

6. **Login Handoff**
   - user can log in,
   - agent session can continue,
   - screenshots and truth extraction do not expose credentials or OTP.

7. **Replay Integrity**
   - every action records pre-truth, safety decision, action dispatch,
     post-truth, verification, and screenshot policy decision.

8. **Local Game Evidence**
   - official ServoShell adapter opens `http://127.0.0.1:4173/`,
   - collects title/basic truth,
   - captures screenshot only under low-risk allowlist or guarded mode.

9. **Isolation**
   - fresh profile/session per run,
   - random `127.0.0.1` WebDriver port,
   - no generic WebDriver exposure,
   - clean teardown.

10. **Upgradeability**
    - same adapter works against pinned installed ServoShell,
    - same adapter works against one newer official build/nightly or records a
      clear compatibility failure.

## Fork Trigger

If any of the following fail, Option B becomes justified:

- screenshot safety needs in-browser/pre-compositor masking,
- login handoff cannot be made safe externally,
- trusted UI can be spoofed by page content,
- manual/agent input provenance must be enforced inside ServoShell,
- WebDriver click/key semantics are not close enough to required native input.

## Immediate Implementation Queue

1. DONE: Build the first Rust WebDriver adapter smoke around official
   ServoShell: `cargo run -q -p saccade-servoshell -- selftest`.
2. DONE: Port a first redacted browser-session truth/action-map JS into a versioned JS
   bundle.
3. DONE: Add screenshot policy modes before using WebDriver screenshot in
   normal runs.
4. DONE: Re-run and expand local safety fixtures through the adapter.
5. DONE: Re-run FORMMAX fixtures through the adapter.
6. Re-run focused typing and dropdown/input fixtures through the adapter.
7. Decide whether login handoff is externally safe or needs the thin fork.

## Latest Evidence

- Adapter selftest:
  `runs/servoshell_adapter/adapter_1781483074229/summary.json`
- Normal page: action dispatch changed `data-session-revision` from `0` to
  `1`, guarded diagnostic screenshot captured after preflight.
- Safety matrix page:
  `runs/servoshell_adapter/adapter_1781483074229/safety_matrix/report.json`
- Safety matrix detected 9 redaction kinds: `api_token`, `credit_card`,
  `email`, `government_id`, `hidden`, `otp`, `password`,
  `recovery_token`, and `ssn`.
- Safety matrix screenshot blocked before capture, and raw value grep passed
  for all 9 fixture secrets.
- FORMMAX adapter:
  `runs/servoshell_adapter/formmax_1781484157780/result.json`
- FORMMAX filled 96 rows across 2 lazy-rendered pages, 672 ordinary fields,
  blocked 3 sensitive fields, verified the receipt, wrote 2715 replay events,
  and kept table values out of result/replay artifacts.
- The FORMMAX fixture now builds table rows via DOM APIs instead of
  `tr.innerHTML`, avoiding an official ServoShell table-parser slow path that
  produced repeated `foster parenting not implemented` warnings during lazy
  rendering.
- Local game probe:
  `runs/servoshell_adapter/probe_1781482435257/report.json`
- Local game screenshot default stayed blocked with decision
  `blocked_forbidden_default`.
