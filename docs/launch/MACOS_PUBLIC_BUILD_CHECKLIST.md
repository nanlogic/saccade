# macOS Public Build And Notarization Checklist

Status: Build 86 notarized; clean-install and installed-product regression remain

Observed on 2026-07-23:

- Apple reports the earlier Build 65 App submission
  `580a69d6-7da7-40c2-b2bd-312d92c3b39c` as `Accepted`;
- the same notary profile's history contains no DMG submission, and the local
  rehearsal stopped before a saved staple/DMG/Gatekeeper result;
- previous local package: Build 85, commit
  `3dc0ad7c97e43d96a262d400ec6dcddc53ffa478`, had Hardened Runtime but no
  secure timestamp or notarization and failed preflight with
  `CSSMERR_TP_NOT_TRUSTED`;
- the original local check reported no valid Developer ID identity;
- a new CSR generated on this Mac produced a matching certificate and private
  key; `security find-identity -v -p codesigning` now lists
  `Developer ID Application: NaN Logic LLC (W5D59P54A2)`.
- Build 86 was frozen at commit
  `9338ab06703dc57ccffffd5d588acdcaf34c7f16`, signed with Hardened Runtime and
  an Apple secure timestamp, and passed the no-upload preflight;
- Apple accepted and stapled both the Build 86 App and DMG. The final DMG
  passed Gatekeeper with SHA-256
  `303149e1113785dbea608cc47795325b38ec2cabf630ba262e49730a07953f66`.

Build 85 cannot become the public package. Build the next candidate from the
commit that contains the updated MCP and use a valid Developer ID Application
identity with an Apple secure timestamp.

Apple notarization is specific to the submitted binary. Build 65's Accepted
status cannot notarize a changed MCP or a later App bundle.

## 1. Human Keychain prerequisite — complete

Install a valid `Developer ID Application` certificate and its matching private
key in the login Keychain. Use one of Apple's supported paths:

- import the certificate and private key from the authorized release Mac using
  a password-protected `.p12`; or
- create a new Developer ID Application certificate through the Apple
  Developer account and install it with the private key created by the local
  certificate signing request.

The release owner enters any import password. Do not copy certificate passwords
or private keys into Codex, MCP output, scripts, logs, or repository files.

Verify the identity:

```bash
security find-identity -v -p codesigning
```

The command must list a valid `Developer ID Application` identity. The existing
notarytool profile `saccade-notary-nanlogic` can handle notarization submission
after code signing passes. It does not replace the signing certificate.

## 2. Freeze a clean candidate

- [x] Commit the updated MCP, public evidence tooling, and launch documents.
- [x] Run the narrow Rust and Python tests.
- [x] Push the exact candidate commit.
- [x] Confirm `git status --porcelain` returns no output.
- [x] Choose the next monotonically increasing build number: 86.

Do not publish a package whose `VERSION.json` says `source_dirty=true`.

## 3. Build with Developer ID and timestamp

Replace `[BUILD]` with the selected build number:

```bash
SACCADE_BUILD_NUMBER=[BUILD] \
SACCADE_CODESIGN_IDENTITY=auto \
SACCADE_CODESIGN_TIMESTAMP=apple \
SACCADE_RELEASE_STAMP=build[BUILD] \
engines/cef/scripts/build_dogfood_release_macos.sh
```

Confirm the candidate metadata records the frozen commit, selected build,
Hardened Runtime, and secure timestamp.

## 4. Run the no-upload preflight

```bash
engines/cef/scripts/notarize_macos.sh preflight \
  dist/saccade-cef-dogfood-build[BUILD]/Saccade.app
```

The preflight checks the main app and executable children for Developer ID,
Hardened Runtime, secure timestamps, and forbidden debug entitlements.

Build 86 result: PASS.

## 5. Submit the App and DMG

This step uploads the frozen candidate to Apple. Run it only after the release
owner approves that exact commit and build:

```bash
SACCADE_NOTARY_KEYCHAIN_PROFILE=saccade-notary-nanlogic \
SACCADE_NOTARY_OUT=dist/notarization-build[BUILD] \
engines/cef/scripts/notarize_macos.sh submit \
  dist/saccade-cef-dogfood-build[BUILD]/Saccade.app
```

The script submits the App archive, checks for `Accepted`, staples the App,
builds and signs the DMG, submits the DMG, staples it, and runs Gatekeeper
assessment on both artifacts.

Build 86 result: PASS. App submission
`44e1a5e9-526f-422e-83d4-01a7e841eb77` and DMG submission
`49f5d93e-fc65-40e0-b4a7-80387a0ff614` were both `Accepted`.

## 6. Public artifact verification

- [x] `xcrun stapler validate` passes on the App and DMG.
- [x] Gatekeeper accepts the local stapled DMG and App without a bypass.
- [x] Generate a SHA-256 checksum for the final DMG.
- [x] Record build, commit, signing team, Apple submission IDs, acceptance
      status, and test platform in a release report.
- [ ] Install on a second clean Mac without the development repository.
- [ ] Verify profile preservation and uninstall behavior.
- [ ] Open a new MCP host task and confirm it runs the MCP embedded in the new
      installed App.
- [ ] Run the remaining protected-value and reflex release gates against
      the installed package.

Same-Mac Build 85 removal and Build 86 DMG reinstall passed installed launch,
signature, staple, Gatekeeper, embedded-MCP cleanroom, profile-preservation,
single-iframe, and nested-iframe checks. This does not close the independent
clean-Mac checkbox above.

The next public candidate contains more than the MCP replay field when compared
with the only Apple-accepted submission, Build 65. It also contains the later
Agent-control, nested-iframe, native-receipt, and task-scoped action changes.
Use this minimum regression set:

- [x] `cargo test -p saccade-mcp`;
- [x] installed App opens and its embedded MCP passes the repo-free cleanroom
  probe; a newly restarted Codex host session remains to be checked;
- [ ] Agent Off/On and same-tab bounded article reading pass;
- [x] the nested-iframe page exposes all three visible fields, compiles one
      complete plan, fills them, and returns verified native receipts;
- [ ] the local task-scoped action fixture passes and protected values remain
      absent from MCP output and replay;
- [ ] one `saccade.web.reflex_run` returns matching receipts and a non-null
      `artifacts.replay`;
- [ ] the returned report, replay, and uncut recording build a valid public
      evidence pack;
- [ ] signing, notarization, staple, Gatekeeper, and clean-install checks pass.

DOCMAX/PDF does not need the full public matrix because this candidate does not
change its code or make a new PDF claim. For a downloadable App, run one
packaged local DOCMAX smoke to prove the binary, scripts, fixtures, ordinary
field writes, and protected-field blocks survived packaging:

```bash
dist/saccade-cef-dogfood-build[BUILD]/bin/run-docmax-gate release_pdf_smoke
```

Repeat the public blank-PDF matrix when PDF code, packaged resources,
protected-value behavior, or a public PDF claim changes.

Build 86 packaged DOCMAX/PDF smoke result: PASS.

## 7. Publication boundary

The technical article can publish before this checklist closes if it states
that no supported public binary exists. A download page and Show HN require the
notarized DMG, checksum, clean-machine result, and matching source commit.
