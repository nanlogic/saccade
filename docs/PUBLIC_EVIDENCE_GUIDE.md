# Public Evidence Guide

Saccade public demonstrations separate machine-verifiable proof from visual
illustration. A video can make a feature understandable, but it cannot make an
unverified browser action pass.

## Reflex evidence boundary

The canonical reflex route is one installed MCP call:

```text
saccade.web.reflex_run
```

The call keeps `next_fact -> native act -> next_receipt` inside the local MCP
process with zero LLM calls in the hot loop. A MouseAccuracy PASS requires:

- same-WebView results truth;
- 100% target efficiency;
- 100% click accuracy;
- page-reported hits equal to verified target receipts;
- no screenshot, external-input, Playwright, CDP, or browser fallback.

The public packager refuses a PASS pack when those fields are absent or do not
match.

## Record one canonical run

1. Use an installed signed Saccade build and a new MCP host task.
2. Open MouseAccuracy in a Saccade Agent-owned tab, or grant the visible Human
   tab explicitly.
3. Configure the page for the intended public gate, such as Epic/Insane, Tiny,
   and 15 seconds. Keep real-site runs below the repository's daily limit.
4. Start a window-scoped macOS recording. The recorder observes the demo; it
   must not provide browser truth or input. For a repeatable command-line run,
   compile the repository recorder once:

   ```bash
   xcrun swiftc -parse-as-library scripts/record_macos_window.swift \
     -o /private/tmp/record_macos_window
   /private/tmp/record_macos_window \
     --application Saccade \
     --output /private/tmp/reflex-master.mp4 \
     --duration 24
   ```

   Start the recorder as a background process, wait for
   `recording_started`, and then make the MCP call in step 5. On first use,
   macOS may require **Privacy & Security → Screen & System Audio Recording**
   permission for Codex or the terminal host followed by an app restart. The
   script selects only the visible Saccade window and refuses to overwrite an
   existing output file. Shift-Command-5 window recording is an acceptable
   manual fallback.
5. Call `saccade.web.reflex_run` once. Example arguments:

   ```json
   {
     "tab_id": 1,
     "auto_start": true,
     "max_hits": 1000,
     "timeout_ms": 15000,
     "results_settlement_timeout_ms": 5000
   }
   ```

6. Keep the recording continuous through the settled result screen. Save the
   structured tool result as `report.json`. The result's `artifacts.replay`
   field identifies the same session's value-free replay; copy that file as
   `replay.jsonl` into one private run directory. The MCP never exposes the
   owner-only grant or its capability.
7. Review both files for the expected PASS and then package them. Do not edit a
   failed result into a pass.

For repeatable iteration, prefer a deterministic local fixture. Use the public
site for the canonical headline run, not bulk development.

## Build the public pack

`ffmpeg` and `ffprobe` are required. The output directory must be new or empty:

```bash
python3 scripts/build_reflex_evidence_pack.py \
  --run-dir runs/private/reflex_build85 \
  --master-video /path/to/reflex-master.mov \
  --output-dir evidence/reflex-15s/2026-07-23-macos-build85 \
  --build 85 \
  --preview-start-sec 2 \
  --preview-duration-sec 6
```

The pack contains:

```text
README.md              public result summary
report.json            sanitized same-WebView result truth
replay.jsonl           sanitized execution replay
environment.json       build, commit, platform, and media provenance
manifest.json          file roles, sizes, and SHA-256 hashes
SHA256SUMS              independent checksum list
reflex-master.*         uncut source recording
reflex-full.mp4         full web-compatible recording
reflex-loop.webm        preferred website loop
reflex-loop.mp4         website fallback
reflex-poster.jpg       initial poster
reflex-readme.gif       optional GitHub animation
embed.html              WebM-first website markup
```

The packager strips URL queries/fragments, replaces the home path with `$HOME`,
redacts capabilities and common protected-value keys, and rejects an empty
replay. It preserves the uncut master and hashes every published artifact.

## Website and repository use

Use the WebM-first `<video>` markup from `embed.html` on a website. Use the GIF
only as a small README preview; link it to the full MP4 and evidence report.
The first five to six seconds should show the complete hook: a target appears,
native input lands, and the score changes.

Every public feature entry should state:

1. exact task and stopping point;
2. build, commit, date, and platform;
3. independent pass/fail oracle;
4. sanitized report and replay;
5. full recording and short preview;
6. explicit limitations and non-claims.

## Validation

```bash
python3 -m unittest scripts/test_build_reflex_evidence_pack.py
python3 -m py_compile scripts/build_reflex_evidence_pack.py
```
