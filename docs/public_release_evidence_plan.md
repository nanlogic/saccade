# Saccade Public Release Evidence Plan

Date: 2026-06-12

## Publishing Shape

Write one readable launch article and one evidence appendix.

The article should tell the story:

```text
Saccade is an AI-first browser automation layer: browser truth, verified actions, sensitive-field masking, and replay.
```

The appendix should carry the weight: commands, artifact paths, screenshots, manifests, validators, and known limits.

## Evidence Already In Hand

| Area | Status | Evidence |
| --- | --- | --- |
| MOUSEMAX real-site proof | Strong | `runs/real/run_1781193985`: Epic + Tiny, 15 seconds, 47 hits, 0 misses, `instrumentation=none`, replay + validator pass |
| MOUSEMAX public page references | Partial | Chrome and Safari URL-bar screenshots captured in `runs/real/run_1781193985/parity_review.html`; Firefox pending because this machine lacks Firefox |
| Local visual/action parity | Strong for current fixtures | `runs/demo_pack/demo_1781306995672/demo_review.html`: all seven visual fixtures produced no red action-map verdicts; Chrome hit-test total 35/35, with four blocked modal actions skipped |
| Native browser UI evidence | Partial | Chrome and Safari native window screenshots captured; Firefox capture path exists but reports unavailable on this machine |
| FORMMAX workflow | Strong local proof | 96 rows, two pages, 672 non-sensitive fields filled, three sensitive fields blocked, receipt verified |
| Safety truth | Strong local proof | Agent sees agent-filled values; human sees all values; sensitive human-owned values stay masked |
| DEVMAX/MCP | Useful local proof | Local fixture corpus, Servo worker, Chrome reference audit, MCP tool surface, redacted artifacts |

## Next Tests Before A Public Post

1. Capture Firefox URL-bar references on a machine with Firefox installed.
2. Add an optional Chrome result screenshot for the MOUSEMAX page after a manual or adapter-run result.
3. Bundle the launch appendix: latest demo pack, MOUSEMAX parity review, FORMMAX report, safety truth report, DEVMAX report, and known limitations.
4. Draft the article after the appendix exists.

## Article Structure

1. Problem: LLMs need browser facts and verifiable actions, not guesses.
2. Architecture: user-visible browser, mediated truth, action map, safety policy, replay.
3. Hard proof: Mouse Accuracy Epic + Tiny on the real site.
4. Practical proof: FORMMAX, DEVMAX, safety truth, Chrome/Saccade hit-test evidence.
5. Limits: Firefox not captured on this machine, Chrome automated click baseline deferred, Servo does not claim pixel parity.
6. Ask: feedback, adversarial test pages, and collaborators for Chrome/Firefox adapter work.

## Do Not Overclaim

- Do not claim Servo looks like Chrome.
- Do not claim full Chrome automated click parity yet.
- Do not claim arbitrary third-party forms are safe until product UI and redacted replay cover them.
- Do not publish screenshots containing real sensitive values.
