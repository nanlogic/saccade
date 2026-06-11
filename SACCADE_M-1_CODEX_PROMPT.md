# Saccade M-1 Codex Prompt — Browser Viability Chat / Kill Gate

Paste this as the first message to Codex after opening the repo directory. This is intentionally a thinking/decision step, not coding.

```text
$global-codex-supervisor Read SACCADE_BUILD_SPEC.md. Do M-1 only.

We are not coding yet. First we need to decide whether the Saccade browser route is viable enough to attempt.

Rules:
- Do not create Rust files.
- Do not create Cargo.toml.
- Do not pin Servo.
- Do not scaffold the workspace.
- You may write only docs/viability_review.md.
- Keep tokens low: use one bounded cheap researcher only if current docs are needed.

Answer in docs/viability_review.md:
1. Does stock Servo plausibly provide rendered frame readback, frame readiness, browser-level input, and recon probes?
2. What are the top five kill risks? Mark each as existential or annoying.
3. What exactly will M0 and M1 prove or disprove?
4. If Servo fails, what is the backup: arena-only, CEF/Chromium, or kill?
5. End with exactly one line:
   SACCADE_BROWSER_VERDICT: <GO_SERVO|GO_SERVO_WITH_BACKUP|ARENA_ONLY|PIVOT_ENGINE|KILL>

After writing that file, stop and ask Wayne for approval.
```
