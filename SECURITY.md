# Security policy

Saccade is pre-release software. Only the latest commit on `main` is currently
in scope for security fixes; there are no supported stable versions yet.

## Report a vulnerability

Please use GitHub's **Report a vulnerability** / private security advisory flow
for this repository. Do not open a public issue for a suspected vulnerability,
credential exposure, sandbox escape, protected-value leak, or signing-key
problem.

Include the affected commit and platform, a minimal reproduction, the expected
security boundary, and the observed result. Remove cookies, tokens, passwords,
one-time codes, personal form values, profile data, and crash dumps containing
private browsing state.

## Important boundaries

Security reports are especially useful when they demonstrate one of these
failures:

- an Agent-Off or ungranted tab exposes structured truth or accepts input;
- a password, OTP, CVV, token, cookie, or protected identifier reaches agent
  output, replay, logs, or artifacts;
- input is accepted for a stale page revision or a different WebView;
- a failed or unverified native action is reported as successful;
- a package update replaces or deletes the user's browser profile; or
- an official artifact has an invalid signature, manifest, or checksum.

Saccade does not promise compatibility with every site and does not bypass
CAPTCHA, bot verification, DRM, or proprietary codec restrictions. A site
compatibility failure without a security-boundary violation can be reported as
a normal issue after the repository opens public issue tracking.
