'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const childProcess = require('node:child_process');

// Chrome and Edge expose Web Crypto globally. Node 18 exposes the same API
// through node:crypto, but not through globalThis in every supported build.
if (!globalThis.crypto) globalThis.crypto = crypto.webcrypto;

const { BROKER_PROTOCOL, OBSERVATION_SCHEMA, randomToken } = require('../src/protocol.js');
const { normalizeOrigin, isProtectedFieldType, redactProtectedText } = require('../src/consent.js');
const { compileChanges, compactTransport } = require('../src/truth_delta.js');
const registry = require('../src/controls/registry.js');

test('Extension protocol names the Node Broker and observation schema', () => {
  assert.equal(BROKER_PROTOCOL, 'saccade.node-broker/1');
  assert.equal(OBSERVATION_SCHEMA, 'saccade.observation/1');
  assert.match(randomToken('action', 16), /^action\.[0-9a-f]{32}$/);
  assert.throws(() => randomToken('action', 15));
});

test('consent helpers enforce only password, SSN, and EIN redaction', () => {
  assert.equal(normalizeOrigin('https://Example.test/path'), 'https://example.test');
  assert.equal(normalizeOrigin('not a URL'), null);
  assert.equal(isProtectedFieldType('password'), true);
  assert.equal(isProtectedFieldType('text', 'current-password'), true);
  assert.equal(isProtectedFieldType('text', '', 'Social Security Number'), true);
  assert.equal(isProtectedFieldType('text', '', 'Employer Identification Number'), true);
  assert.equal(isProtectedFieldType('text', 'one-time-code'), false);
  assert.equal(isProtectedFieldType('text', 'cc-number'), false);
  assert.equal(isProtectedFieldType('text', 'name'), false);
  assert.equal(redactProtectedText('SSN 123-45-6789; EIN 12-3456789'), 'SSN [REDACTED SSN]; EIN [REDACTED EIN]');
});

test('production manifest preserves identity and excludes out-of-scope capabilities', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '../manifest.json'), 'utf8'));
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.equal(manifest.manifest_version, 3);
  assert.equal(manifest.name, 'Saccade');
  assert.equal(manifest.version, '0.4.1');
  const digest = crypto.createHash('sha256').update(Buffer.from(manifest.key, 'base64')).digest('hex').slice(0, 32);
  const extensionId = [...digest].map((digit) => String.fromCharCode(97 + Number.parseInt(digit, 16))).join('');
  assert.equal(extensionId, 'bobfbgjplflcigednmccmbhlgclomgod');
  assert.deepEqual(manifest.permissions, ['tabs', 'storage', 'alarms']);
  assert.ok(manifest.host_permissions.includes('http://127.0.0.1:32177/*'));
  assert.deepEqual(manifest.icons, {
    16: 'icons/icon-16.png',
    32: 'icons/icon-32.png',
    48: 'icons/icon-48.png',
    128: 'icons/icon-128.png',
  });
  assert.deepEqual(manifest.action.default_icon, {
    16: 'icons/icon-16.png',
    32: 'icons/icon-32.png',
  });
  assert.equal(manifest.content_scripts.length, 1);
  assert.equal(manifest.content_scripts[0].run_at, 'document_start');
  assert.equal(manifest.content_scripts[0].world, 'ISOLATED');
  assert.equal(manifest.content_scripts[0].js[0], 'src/candidate_identity.js');
  assert.equal(manifest.content_scripts[0].js.at(-1), 'src/collector.js');
  assert.ok(manifest.content_scripts[0].js.indexOf('src/truth_delta.js') < manifest.content_scripts[0].js.indexOf('src/collector.js'));
  assert.equal(manifest.action.default_popup, 'popup.html');
  assert.match(worker, /BROKER_ORIGIN = 'http:\/\/127\.0\.0\.1:32177'/);
  assert.doesNotMatch(worker, /connectNative|nativeMessaging|NATIVE_HOST/);
  assert.match(worker, /extension_candidate: LOADED_CANDIDATE/);
  assert.match(worker, /reloadIfCandidateChanged/);
  assert.match(worker, /sameCandidate\(ping\.extension_candidate\)/);
  assert.match(collector, /extension_candidate: globalThis\.SaccadeCandidate/);
  assert.match(collector, /identities\.set\(element, `object\.\$\{\+\+objectSerial\}`\)/);
  assert.doesNotMatch(collector, /`object\.\$\{documentId\}/);
  assert.doesNotMatch(worker, /chrome\.(downloads|debugger|scripting)/);
  assert.doesNotMatch(worker, /Playwright|CDP|protected_fill|loop\.start/);
});

test('popup uses the brand icon and states the exact protected-value boundary', () => {
  const html = fs.readFileSync(path.join(__dirname, '../popup.html'), 'utf8');
  const css = fs.readFileSync(path.join(__dirname, '../popup.css'), 'utf8');
  const popup = fs.readFileSync(path.join(__dirname, '../popup.js'), 'utf8');
  assert.match(html, /icons\/icon-48\.png/);
  assert.match(html, /id="dev-badge"/);
  assert.match(css, /color-scheme: light/);
  assert.match(css, /--bg: #ffffff/);
  assert.match(popup, /Password, SSN, and EIN values stay protected/);
  assert.doesNotMatch(popup, /Passwords, OTPs, and editable values/);
  assert.match(popup, /name\.includes\('\(Development\)'\)/);
  assert.match(popup, /status\.broker_connected && status\.observation_ready/);
  assert.doesNotMatch(popup, /status\.host_connected/);
});

test('tab sharing UI revokes any authorized tab without closing it', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const popup = fs.readFileSync(path.join(__dirname, '../popup.js'), 'utf8');
  assert.match(worker, /sender\.url !== chrome\.runtime\.getURL\('popup\.html'\)/);
  assert.match(worker, /userSharedTabs\.add\(tabId\)/);
  assert.match(worker, /agentOwnedTabs\.delete\(tabId\)/);
  assert.match(worker, /userSharedTabs\.delete\(tabId\)/);
  assert.match(worker, /collector\.deauthorize/);
  assert.match(worker, /authorizeTab\(tabId, \{ recoverStale: true \}\)/);
  assert.match(worker, /if \(!ready && recoverStale\)/);
  assert.match(worker, /chrome\.tabs\.reload\(tabId\)/);
  assert.match(worker, /waitForCurrentCollector\(tabId, 200\)/);
  assert.doesNotMatch(worker, /Agent-owned tabs are revoked by closing the tab/);
  assert.match(collector, /function deauthorize/);
  assert.match(collector, /tokenTargets\.clear\(\)/);
  assert.match(popup, /ui\.tab\.share/);
  assert.match(popup, /ui\.tab\.revoke/);
  assert.match(popup, /current\?\.authorized \? 'ui\.tab\.revoke'/);
  assert.match(popup, /Saccade access is on for this tab/);
  assert.doesNotMatch(popup, /Agent-owned tab/);
  assert.doesNotMatch(popup, /storage\.(local|session)|executeScript|connectNative/);
});

test('collector routes editable-family controls through the Registry', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /registry\.observe\(role/);
  assert.match(collector, /registry\.option\(/);
  assert.match(collector, /option_object_id/);
  assert.match(collector, /\[role="listbox"\]/);
  assert.match(collector, /\[role="combobox"\]/);
  assert.match(collector, /comboboxForListbox/);
  assert.match(collector, /optionsForChoice/);
  assert.match(collector, /rememberedChoiceOwner/);
  assert.match(collector, /rememberedChoicePopup/);
  assert.match(collector, /optionEnabled/);
  assert.match(collector, /function composedQuery/);
  assert.match(collector, /\[contenteditable\]/);
  assert.match(collector, /type === 'search'/);
  assert.match(collector, /type === 'number'/);
  assert.match(collector, /spin_button/);
  assert.match(collector, /content_editable/);
  assert.match(collector, /visibleFileTrigger/);
  assert.match(collector, /visibleRadioTrigger/);
  assert.match(collector, /OBSERVED_SELECTOR = `\$\{CONTROL_SELECTOR\},label,/);
  assert.match(collector, /function navigationTargetFor/);
  assert.match(collector, /\['http:', 'https:'\]/);
  assert.match(collector, /object\.navigation_target = navigationTarget/);
  assert.match(collector, /aria-controls/);
  assert.match(collector, /aria-activedescendant/);
  assert.match(collector, /input\[type="file"\]/);
  assert.match(collector, /fileTriggerHasValue/);
  assert.match(collector, /activeFileTrigger/);
  assert.match(collector, /changed\.files\?\.length/);
  assert.match(collector, /replace\|add/);
  assert.match(collector, /images\?\|covers\?\|screenshots\?/);
  assert.match(collector, /seenFileTriggers/);
  assert.match(collector, /repeatedActionKeys/);
  assert.match(collector, /copy\.querySelectorAll\('button,input,select,textarea,\[contenteditable\]'\)/);
  assert.match(collector, /isContentEditable/);
  assert.match(collector, /data-saccade-image-identity/);
  assert.match(collector, /function imageObject/);
  assert.match(collector, /Semantic identity:/);
  assert.match(collector, /accessibleFallbackText/);
  assert.match(collector, /aria-hidden/);
  assert.doesNotMatch(collector, /element\.value[^\n]*name|XPath/i);
  assert.match(collector, /SURFACE_SELECTOR/);
  assert.match(collector, /opaque_canvas/);
  assert.match(collector, /opaque_webgl/);
  assert.match(collector, /opaque_video/);
  assert.match(collector, /browser_restricted_page/);
});

test('object-addressed upload captures dynamic choosers and supplies one bounded FileList', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const fixture = fs.readFileSync(
    path.join(__dirname, '../../fixtures/controls/file_input.html'), 'utf8');
  assert.match(collector, /function isFileUploadTrigger/);
  assert.match(collector, /data-saccade-file-upload/);
  assert.match(collector, /authoredUploadLike/);
  assert.match(collector, /authoredUploadButton/);
  assert.match(collector, /unnamedButton/);
  assert.match(collector, /class\*="upload" i/);
  assert.match(collector, /Drop|drop/);
  assert.match(collector, /function captureDynamicFileInput/);
  const uploadClick = collector.slice(
    collector.indexOf('function dispatchUploadTriggerClick('),
    collector.indexOf('async function captureDynamicFileInput('),
  );
  assert.match(uploadClick, /trigger\.addEventListener\('click', preventNativeDefault, \{ once: true \}\)/);
  assert.match(uploadClick, /trigger\.dispatchEvent\(event\)/);
  assert.doesNotMatch(uploadClick, /if \(type === 'click'\) event\.preventDefault\(\)/);
  assert.match(collector, /document\.addEventListener\('click', onClick\)/);
  assert.doesNotMatch(collector, /document\.addEventListener\('click', onClick, true\)/);
  assert.match(collector, /event\.preventDefault\(\)/);
  assert.match(collector, /function uploadFileFromPayload/);
  assert.match(collector, /function uploadDropTarget/);
  assert.match(collector, /function dispatchFileDrop/);
  assert.match(collector, /nativeChooserButton/);
  const linkedInput = collector.slice(
    collector.indexOf('function linkedFileInput('),
    collector.indexOf('function dispatchUploadTriggerClick('),
  );
  assert.match(linkedInput, /for \(let depth = 0;[\s\S]*depth < 5/);
  assert.match(linkedInput, /querySelectorAll\('input\[type="file"\]'\)/);
  assert.match(linkedInput, /if \(candidates\.length === 1\) return candidates\[0\]/);
  assert.match(collector, /new view\.DragEvent/);
  assert.match(collector, /function waitForUploadResponse/);
  assert.match(collector, /new view\.MutationObserver/);
  assert.match(collector, /file_drop_response_observed/);
  assert.match(collector, /file_selection_observed/);
  assert.match(collector, /verified: responseObserved/);
  assert.match(collector, /verified: selectionObserved/);
  assert.match(collector, /MAX_UPLOAD_BYTES/);
  assert.match(collector, /new view\.File/);
  assert.match(collector, /new view\.DataTransfer\(\)/);
  assert.match(collector, /transfer\.items\.add\(file\)/);
  assert.match(collector, /input\.files = transfer\.files/);
  assert.match(collector, /new view\.Event\('input'/);
  assert.match(collector, /new view\.Event\('change'/);
  assert.match(collector, /if \(request\.operation === 'upload'\) return softUpload/);
  assert.doesNotMatch(collector, /showOpenFilePicker|webkitRelativePath/);
  assert.match(worker, /function scrubUploadPayload/);
  assert.match(worker, /delete payload\.payload\.file\.content_base64/);
  assert.match(fixture, /id="dynamic-upload"/);
  assert.match(fixture, /id="context-upload"/);
  assert.match(fixture, /id="unnamed-upload"/);
  assert.match(fixture, /id="drop-upload"/);
  assert.match(fixture, />Manage media</);
  assert.match(fixture, /document\.createElement\('input'\)/);
  assert.match(fixture, /input\.click\(\)/);
});

test('visually hidden native radios keep native identity and state while using their visible label for actionability', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const fixture = fs.readFileSync(
    path.join(__dirname, '../../fixtures/controls/segmented_radio.html'), 'utf8');
  const trigger = collector.slice(
    collector.indexOf('function visibleRadioTrigger('),
    collector.indexOf('function ariaBoolean('),
  );
  assert.match(trigger, /inputVisibility === 'visible'/);
  assert.match(trigger, /Array\.from\(input\.labels \|\| \[\]\)\.find\(visible\) \|\| input/);
  assert.match(collector, /role === 'radio' \? visibleRadioTrigger\(element\) : element/);
  assert.match(collector, /element: interactionElement, controlElement: element, role/);
  assert.match(collector, /const control = target\.controlElement \|\| target\.element/);
  assert.match(collector, /tokenTargets\.values\(\)\]\.map\(\(target\) => target\.element\)/);
  assert.match(fixture, /clip-path: inset\(50%\)/);
  assert.match(fixture, /for="speed-epic">Epic/);
  assert.match(fixture, /for="size-tiny">Tiny/);
});

test('collector projects bounded structural text without actions or editable descendants', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /MAX_STRUCTURAL_TEXT_BYTES = 256 \* 1024/);
  assert.match(collector, /STRUCTURAL_SELECTOR/);
  assert.match(collector, /function structuralText/);
  assert.match(collector, /function structuralObject/);
  assert.ok(collector.indexOf("role === 'status'") < collector.indexOf("tag === 'P'"));
  assert.match(collector, /DIALOG_SELECTOR/);
  assert.match(collector, /function dialogTitleCandidates/);
  assert.match(collector, /function dialogTextCandidates/);
  assert.match(collector, /GENERIC_TEXT_SELECTOR/);
  assert.match(collector, /function genericTextCandidates/);
  assert.match(collector, /\.\.\.genericTexts/);
  assert.match(collector, /visibilityFor\(element, boxFor\(element\)\) === 'hidden'/);
  assert.match(collector, /element\.hasAttribute\('aria-live'\)\) return 'status'/);
  assert.match(collector, /tag === 'OUTPUT'\) return 'status'/);
  assert.match(collector, /\(dialogText \|\| genericText\) \? 'text'/);
  assert.match(collector, /state\.modal = String\(element\.getAttribute\('aria-modal'\) === 'true'\)/);
  assert.match(collector, /deferred_content_possible/);
  assert.match(collector, /transitionend/);
  assert.match(collector, /animationend/);
  assert.match(collector, /kind: 'text', role, text, state, affordances: \[\], protected: false/);
  assert.match(collector, /element\.closest\(CONTROL_SELECTOR\)/);
  assert.match(collector, /TextEncoder/);
  assert.match(collector, /document\.readyState === 'loading'\) \{\s*schedule\(\);\s*document\.addEventListener\('DOMContentLoaded', collect/s);
  assert.doesNotMatch(collector, /function collect\(\) \{\s*if \(!config\) return null;\s*if \(document\.readyState === 'loading'\) return null;/s);
  assert.doesNotMatch(collector, /object\.affordances = \[\].*delete object\.action_token.*tokenTargets\.clear\(\)/s);
  assert.match(collector, /Per-action local preflight owns/);
  assert.match(collector, /DOMContentLoaded.*collect/s);
  assert.match(collector, /schedule\(\);\s*return null;/);
});

test('unrelated page mutations do not churn current control tokens', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /mutationCanChangeObservation/);
  assert.match(collector, /records\.some\(mutationCanChangeObservation\)/);
  assert.match(collector, /element\.matches\(OBSERVED_SELECTOR\)/);
  assert.match(collector, /compileChanges\(compiledObjects, objects\)/);
});

test('semantic page churn preserves authority only for the same live object contract', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const fingerprint = collector.slice(
    collector.indexOf('function authorityFingerprint('),
    collector.indexOf('function reuseStableAuthorities('),
  );
  const reuse = collector.slice(
    collector.indexOf('function reuseStableAuthorities('),
    collector.indexOf('function collect('),
  );
  assert.match(reuse, /previous\.target\.element !== current\.element/);
  assert.match(reuse, /previous\.target\.role !== current\.role/);
  assert.match(reuse, /previous\.target\.affordances\.join/);
  assert.match(reuse, /object\.action_token = previous\.token/);
  assert.match(fingerprint, /delete contract\.visibility/);
  assert.match(fingerprint, /delete contract\.transition/);
  assert.ok(collector.indexOf('reuseStableAuthorities(objects, previousTokenTargets)') < collector.indexOf('const changes = compileChanges'));
});

test('semantic mutations are not gated by rendering frames', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /if \(!records\.some\(mutationCanChangeObservation\)\) return/);
  assert.match(collector, /if \(isMouseAccuracyGame\(document\)\) scheduleVisual\(\);\s*else schedule\(\)/);
  assert.match(collector, /function schedule\(\).*queueMicrotask/s);
  assert.match(collector, /addEventListener\('scroll', scheduleVisual/);
  assert.match(collector, /addEventListener\('resize', scheduleVisual/);
  assert.match(collector, /function scheduleVisual\(\).*requestAnimationFrame/s);
  assert.match(collector, /ResizeObserver\(scheduleVisual\)/);
  assert.match(collector, /function currentGeometryIsAnimating/);
  assert.match(collector, /getAnimations\?\.\(\).*playState === 'running'/s);
  assert.match(collector, /transitionrun.*animationstart/s);
});

test('the bounded MouseAccuracy reflex bridge covers both current and Classic game lifecycles', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const bridge = collector.slice(
    collector.indexOf('function isMouseAccuracyGame('),
    collector.indexOf('function observeCurrentGeometry('),
  );
  assert.match(bridge, /pathname\.startsWith\('\/game'\)/);
  assert.match(bridge, /pathname\.startsWith\('\/classic'\)/);
  assert.match(bridge, /function updateMouseAccuracyHitOccurrence/);
  assert.match(bridge, /querySelectorAll\('\.target\.hit'\)/);
  assert.match(bridge, /function recordMouseAccuracyOccurrence/);
  assert.match(bridge, /!softwareDispatchCompleted && element\.isConnected && !element\.classList\.contains\('hit'\)/);
  assert.match(collector, /isMouseAccuracyClassic\(page\)[\s\S]*String\(mouseAccuracyHitOccurrence\)/);
  assert.ok(collector.indexOf('updateMouseAccuracyHitOccurrence(document)') < collector.indexOf('const hadCompiledObjects'));
  const softClick = collector.slice(collector.indexOf('async function softClick('), collector.indexOf('async function softType('));
  assert.match(softClick, /if \(type === 'click'\) clickDispatchCompleted = true/);
  assert.match(softClick, /target\.role === 'reflex_target'[\s\S]*recordMouseAccuracyOccurrence\(target\.element, clickDispatchCompleted\)/);
  assert.match(softClick, /if \(recordedReflexOccurrence\) collect\(\)/);
});

test('editable placeholders are explicitly distinguished from current values', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /return `Placeholder: \$\{placeholder\}`/);
  assert.doesNotMatch(collector, /if \(placeholder && placeholder !== name\) return placeholder;/);
});

test('Extension compiler emits semantic and geometry Truth Layer deltas while ignoring authority churn', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /const unchanged = hadCompiledObjects && changes\.length === 0/);
  const base = {
    object_id: 'o.internal', object_revision: 1, role: 'button', kind: 'control',
    name: 'Save', state: { pressed: 'false' }, affordances: ['click'],
    action_token: 'action.old', document_bounds: { x: 1, y: 1, width: 10, height: 10 },
  };
  const authorityOnly = { ...base, object_revision: 2, action_token: 'action.new' };
  assert.deepEqual(compileChanges([base], [authorityOnly]), []);
  const changed = { ...authorityOnly, state: { pressed: 'true' } };
  assert.deepEqual(compileChanges([base], [changed]), [
    { kind: 'updated', object_id: 'o.internal', object_revision: 2 },
  ]);
  assert.equal(compileChanges([], [base])[0].kind, 'appeared');
  assert.equal(compileChanges([base], [])[0].kind, 'disappeared');
});

test('Extension transports one full snapshot then only changed objects and compact authorities', () => {
  const unchanged = {
    object_id: 'o.keep', object_revision: 2, role: 'button', kind: 'control',
    name: 'Keep', state: { pressed: 'false' }, affordances: ['click'],
    action_token: 'action.keep', document_bounds: { x: 1, y: 1, width: 10, height: 10 },
  };
  const updated = {
    object_id: 'o.change', object_revision: 2, role: 'checkbox', kind: 'control',
    name: 'Change', state: { checked: 'true' }, affordances: ['click'],
    action_token: 'action.change', document_bounds: { x: 1, y: 20, width: 10, height: 10 },
  };
  const changes = [
    { kind: 'updated', object_id: 'o.change', object_revision: 2 },
    { kind: 'disappeared', object_id: 'o.gone', object_revision: 1 },
  ];
  assert.deepEqual(compactTransport([unchanged, updated], changes), {
    objects: [updated],
    authorities: [{ object_id: 'o.keep', action_token: 'action.keep' }],
  });
  assert.throws(() => compactTransport([updated], [changes[0], changes[0]]));

  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(collector, /collector\.observation_delta/);
  assert.match(collector, /base_revision: revision - 1/);
  assert.match(collector, /collector\.snapshot/);
  assert.match(worker, /observation\.delta/);
  assert.match(worker, /command\.kind === 'observation\.resync'/);
  assert.match(worker, /requestCollectorSnapshot\(tabId\)/);
  assert.doesNotMatch(worker, /chrome\.storage\.(local|session)\.(set|get)\([^\n]*observation/i);
});

test('control movement emits Truth updates when geometry changes', () => {
  const staticGeometry = { x: 1, y: 2, width: 10, height: 10 };
  const nextGeometry = { x: 30, y: 40, width: 10, height: 10 };
  const before = {
    object_id: 'o.move', object_revision: 1, role: 'button', kind: 'control', name: 'Move', state: { pressed: 'false' },
    affordances: ['click'], protected: false, action_token: 'action.001', visibility: 'visible',
    document_bounds: staticGeometry, viewport_bounds: staticGeometry,
  };
  const after = { ...before, object_revision: 2, document_bounds: nextGeometry, viewport_bounds: nextGeometry };
  assert.deepEqual(compileChanges([before], [after]), [
    { kind: 'updated', object_id: 'o.move', object_revision: 2 },
  ]);
});

test('hidden-to-removed transition stays truthful: update first, disappear on omission', () => {
  const base = {
    object_id: 'o.visible', object_revision: 1, role: 'button', kind: 'control', name: 'Submit', state: { pressed: 'false' },
    affordances: ['click'], protected: false, action_token: 'action.002', visibility: 'visible',
    document_bounds: { x: 4, y: 8, width: 25, height: 8 }, viewport_bounds: { x: 4, y: 8, width: 25, height: 8 },
  };
  const hidden = { ...base, object_revision: 2, visibility: 'hidden' };
  assert.deepEqual(compileChanges([base], [hidden]), [
    { kind: 'updated', object_id: 'o.visible', object_revision: 2 },
  ]);
  assert.deepEqual(compileChanges([hidden], []), [
    { kind: 'disappeared', object_id: 'o.visible', object_revision: 2 },
  ]);
});

test('protected controls still expose geometry fields and stay value-free', () => {
  const observed = registry.observe('text_field', { hasValue: true, protected: true, value: 'SENSITIVE' });
  const projected = {
    object_id: 'o.protected-field', object_revision: 1, kind: 'control', role: 'text_field',
    name: 'Protected field', state: observed.state, affordances: observed.affordances,
    protected: true, visibility: 'visible',
    document_bounds: { x: 12, y: 34, width: 160, height: 20 },
    viewport_bounds: { x: 12, y: 34, width: 160, height: 20 },
  };
  const wire = JSON.stringify(projected);
  assert.equal(projected.protected, true);
  assert.equal(projected.document_bounds.x, 12);
  assert.equal(projected.viewport_bounds.width, 160);
  assert.equal(wire.includes('SENSITIVE'), false);
  assert.equal(wire.includes('redacted'), false);
});

test('duplicate actionable controls receive value-free semantic context across control families', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /registry\.observe\(role, signalsFor\(element, role\)\)\.affordances\.length/);
  assert.doesNotMatch(collector, /!\['button', 'link'\]\.includes\(role\)/);
  assert.match(collector, /querySelectorAll\('button,input,select,textarea,\[contenteditable\]'\)/);
});

test('same-origin frames and open shadow roots compose without changing the root message route', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(collector, /function collectFrameContexts/);
  assert.match(collector, /element\.contentDocument/);
  assert.match(collector, /status: 'restricted_permission'/);
  assert.match(collector, /kind: 'restricted_frame'/);
  assert.match(collector, /element\.shadowRoot/);
  assert.match(collector, /function topViewportBox/);
  assert.match(collector, /frame\.clientLeft/);
  assert.match(worker, /port\.name !== 'saccade\.collector'/);
  assert.match(worker, /acceptCollectorObservation/);
  assert.match(collector, /chrome\.runtime\.connect\(\{ name: 'saccade\.collector' \}\)/);
  assert.doesNotMatch(worker, /webNavigation|getAllFrames|frame_observation/);
});

test('open ownership precedes response and loading tabs start collection without waiting for complete', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  // The Saccade-created branch is the last tabs.open branch; the claim arm and
  // confirm branches are matched ahead of it and create no tab.
  const open = worker.slice(worker.lastIndexOf("command.kind === 'tabs.open'"), worker.indexOf("command.kind === 'prepare_action'"));
  const openAgentTab = worker.slice(worker.indexOf('async function openAgentTab'), worker.indexOf('function armTabClaim'));
  assert.match(openAgentTab, /chrome\.windows\.getAll\(\{ windowTypes: \['normal'\] \}\)/);
  assert.match(openAgentTab, /normalWindows\.find\(\(window\) => window\.focused\) \|\| normalWindows\.at\(-1\)/);
  assert.match(openAgentTab, /chrome\.tabs\.create\(\{\s*windowId: targetWindow\.id, url, active/s);
  assert.match(openAgentTab, /chrome\.windows\.create\(\{\s*url, type: 'normal', focused: active/s);
  assert.match(openAgentTab, /chrome\.tabs\.query\(\{ windowId: createdWindow\.id, active: true \}\)/);
  assert.doesNotMatch(open, /chrome\.tabs\.create\(\{ url:/);
  assert.ok(open.indexOf('agentOwnedTabs.add(tab.id)') < open.indexOf('reply(command'));
  assert.match(open, /isSupportedUrl\(current\.url\).*authorizeTab\(tab\.id\)/s);
  assert.match(worker, /change\.status === 'loading'.*sessions\.delete\(tabId\)/s);
  assert.match(worker, /change\.url \|\| change\.status === 'loading' \|\| change\.status === 'complete'/);
  assert.match(worker, /waitForCurrentCollector\(tabId, 40\)/);
  assert.doesNotMatch(worker, /executeScript/);
  assert.match(worker, /authorizationPromises\.get\(tabId\)/);
  assert.match(worker, /existing\?\.url === tab\.url/);
  assert.match(worker, /existing\.promise\.catch\(\(\) => \{\}\)\.then/);
  assert.match(worker, /session\?\.url === tab\.url && \(session\.configuring \|\| session\.configured\)/);
  assert.match(worker, /tab URL changed during collector authorization/);
});

test('tab cleanup exposes ownership and closes Agent-owned tabs only', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const close = worker.slice(worker.indexOf("command.kind === 'tabs.close'"), worker.indexOf("command.kind === 'prepare_action'"));
  assert.match(worker, /ownership: agentOwnedTabs\.has\(tabId\) \? 'agent' : 'user_shared'/);
  assert.match(close, /if \(!agentOwnedTabs\.has\(tabId\)\) throw new Error/);
  assert.match(close, /chrome\.tabs\.remove\(tabId\)/);
  assert.match(close, /only Agent-owned tabs may be closed through Saccade/);
  assert.match(close, /const closesLastWindowTab = windowTabs\.length === 1/);
  assert.ok(close.indexOf('reply(command, { tab_id: String(tabId), closed: true });') < close.indexOf('try { await chrome.tabs.remove(tabId); }'));
  assert.match(worker, /chrome\.windows\.onRemoved\.addListener\(\(\) => \{ reconnectAfterWindowRemoval\(\); \}\)/);
  assert.doesNotMatch(close, /userSharedTabs\.has\(tabId\).*chrome\.tabs\.remove/s);
});

test('tab ownership survives Extension reload but resets at browser startup', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(worker, /chrome\.storage\.local\.set\(\{ \[TAB_ACL_KEY\]/);
  assert.match(worker, /chrome\.storage\.local\.get\(TAB_ACL_KEY\)/);
  assert.match(worker, /chrome\.storage\.session\.get\(\[BROWSER_SESSION_KEY, CONNECTION_SESSION_KEY\]\)/);
  assert.match(worker, /freshBrowserSession \? \{\} : \(storedAcl\[TAB_ACL_KEY\] \|\| \{\}\)/);
  assert.match(worker, /chrome\.storage\.local\.remove\(TAB_ACL_KEY\)/);
  assert.match(worker, /chrome\.runtime\.onStartup\.addListener\(\(\) => \{\s*connectBroker\(\)\.catch\(scheduleReconnect\)/s);
  assert.match(worker, /connectBroker\(\)\.catch\(scheduleReconnect\);\s*$/);
  assert.doesNotMatch(worker, /bootTimer|setTimeout\(\(\) => \{ connectBroker/);
  assert.doesNotMatch(worker, /chrome\.storage\.session\.(get|set)\(TAB_ACL_KEY/);
});

test('prepare checks the revision basis after tab activation and focus', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const publicAct = worker.slice(worker.indexOf("command.kind === 'act'"), worker.indexOf("command.kind === 'act.batch'"));
  assert.match(worker, /if \(!browserWindow\.focused\).*chrome\.windows\.update/s);
  assert.match(worker, /if \(!tab\.active\).*chrome\.tabs\.update/s);
  assert.ok(worker.indexOf('chrome.windows.update') < worker.indexOf("kind: 'collector.prepare_action'"));
  assert.match(publicAct, /await activateTabForAction\(tabId, command\.deadline_at\)/);
  assert.ok(publicAct.indexOf('activateTabForAction') < publicAct.indexOf("kind: 'collector.soft_action'"));
  assert.match(worker, /ACTION_RESPONSE_RESERVE_MS = 250/);
  assert.match(worker, /return remainingMs - ACTION_RESPONSE_RESERVE_MS/);
  assert.match(collector, /request\.basis_revision !== revision/);
  assert.match(collector, /previous\.target\.authorityFingerprint !== current\.authorityFingerprint/);
  assert.match(collector, /authorityFingerprint: authorityFingerprint\(object\)/);
  assert.match(collector, /request\.document_id !== documentId/);
  assert.match(collector, /detail === 'stale action basis'\) collect\(\)/);
  assert.doesNotMatch(worker, /sessions\.get\(tabId\)\?\.last\?\.document_id !== payload\.document_id/);
});

test('software action bridge is token-bound and limited to Registry roles', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /data-saccade-reflex-target/);
  assert.match(collector, /!element\.classList\.contains\('hit'\)/);
  assert.match(collector, /element === page\.body/);
  assert.match(collector, /location\.hostname === 'mouseaccuracy\.com'/);
  assert.match(collector, /location\.pathname\.startsWith\('\/game'\)/);
  assert.match(collector, /'Decrease' : 'Increase'/);
  assert.match(collector, /loop_class_token = reflexLoopClassToken/);
  assert.match(collector, /SOFTWARE_CLICK_ROLES/);
  assert.match(collector, /'select', 'option', 'tab'/);
  assert.match(collector, /const interactionElement = clickable \? option : owner/);
  assert.match(collector, /role: 'option'.*affordances: descriptor\.affordances/s);
  assert.match(collector, /software click is not registered for the current control/);
  assert.ok(collector.indexOf('prepare(request);') < collector.indexOf('target.element.dispatchEvent'));
  assert.match(collector, /reflexOccurrence/);
  assert.match(collector, /isMouseAccuracyGame\(document\).*scheduleVisual/s);
  assert.match(collector, /target\.element\.dispatchEvent/);
  assert.match(worker, /command\.kind === 'soft_action'/);
  assert.match(collector, /choiceOwner\(option\) !== target/);
  assert.match(collector, /option\.selected = true/);
  assert.match(collector, /function waitForSelectOption/);
  assert.match(collector, /option !== original/);
  assert.match(collector, /select_option_stale/);
  assert.match(collector, /select_option_actionability_timeout_/);
  assert.match(collector, /current\.option\.dispatchEvent/);
  assert.doesNotMatch(collector, /Array\(prepared\.selection_index\)\.fill\('ArrowDown'\)/);
  assert.match(collector, /requestAnimationFrame\(collect\)/);
  assert.match(worker, /command\.kind === 'soft_click'/);
  assert.match(worker, /collector\.soft_click/);
});

test('prepare-stage stale authority is explicitly retry safe because nothing dispatched', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /actionFailure\('prepare', 'stale_action_basis', true/);
  assert.match(collector, /actionFailure\('prepare', 'stale_action_token', true/);
});

test('Chrome and Edge share one loopback Broker protocol', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(worker, /BROWSER_FAMILY = navigator\.userAgent\.includes\('Edg\/'\) \? 'edge' : 'chrome'/);
  assert.match(worker, /browser_family: BROWSER_FAMILY/);
  assert.match(worker, /broker_epoch/);
  assert.doesNotMatch(worker, /darwin|win32|platform_input|connectNative/);
});

test('Node Broker reconnect uses bounded backoff without a popup wake-up', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const reconnect = worker.slice(worker.indexOf('function scheduleReconnect'), worker.indexOf('async function brokerRequest'));
  assert.match(reconnect, /Math\.min\(250 \* \(2 \*\* reconnectAttempts\+\+\), 4000\)/);
  assert.doesNotMatch(reconnect, /reconnectAttempts\s*>=/);
  assert.match(reconnect, /armReconnectAlarm\(\)/);
  assert.match(worker, /function armReconnectAlarm\(\) \{\s*chrome\.alarms\.create\(RECONNECT_ALARM/s);
  assert.match(worker, /RECONNECT_ALARM_PERIOD_MINUTES = 0\.5/);
  assert.match(worker, /periodInMinutes: RECONNECT_ALARM_PERIOD_MINUTES/);
  assert.match(worker, /async function settleReconnect\(connectionId\)/);
  assert.match(worker, /armReconnectAlarm\(\);\s*connectBroker\(\)\.catch\(scheduleReconnect\);\s*$/);
  assert.match(worker, /chrome\.alarms\.onAlarm\.addListener/);
  assert.match(worker, /function brokerRuntimeReady\(\)/);
  assert.match(worker, /broker_connected: brokerRuntimeReady\(\)/);
  assert.match(worker, /if \(!brokerRuntimePresent\(\)\) \{\s*try \{ await ensureBrokerConnection\(\); \}/s);
  assert.doesNotMatch(reconnect, /if \(brokerConnectionId \|\| connectPromise\) return/);
  assert.match(reconnect, /if \(connectPromise\) \{[\s\S]*pending\.finally/);
  assert.match(worker, /function startCommandLoop\(connectionId, generation\)/);
  assert.match(worker, /function startTabRecovery\(connectionId, requireFullTruth\)/);
  const connect = worker.slice(worker.indexOf('async function connectBroker'), worker.indexOf('function numericTabId'));
  assert.ok(connect.indexOf('startCommandLoop(') < connect.indexOf('startTabRecovery('));
  assert.doesNotMatch(connect, /await authorizeTab/);
  assert.match(worker, /commandLoopState !== state[\s\S]*brokerLoopGeneration !== generation/);
  assert.match(worker, /if \(commandLoopState === state\) commandLoopState = undefined/);
  assert.match(worker, /brokerRequest\('\/v1\/extension\/commands', \{\s*method: 'POST'/s);
  assert.doesNotMatch(worker, /extension\/commands\?connection_id/);
  assert.match(worker, /KEEPALIVE_INTERVAL_MS = 20_000/);
  assert.match(worker, /new WebSocket\(/);
  assert.match(worker, /kind: 'heartbeat'/);
  assert.match(worker, /kind !== 'heartbeat\.ack'/);
  assert.match(worker, /AbortSignal\.timeout\(timeoutMs\)/);
  assert.match(worker, /Collector response did not arrive before the command deadline/);
  assert.match(worker, /error\.saccadeCode = 'OUTCOME_UNKNOWN'/);
  assert.match(worker, /error\.saccadeOutcome = 'outcome_unknown'/);
  assert.match(worker, /catch \(error\) \{\s*brokerConnectionId = undefined;\s*brokerEpoch = undefined;\s*throw error;/s);
});

test('the Extension Service Worker is valid JavaScript', () => {
  const workerPath = path.join(__dirname, '../src/service_worker.js');
  const checked = childProcess.spawnSync(process.execPath, ['--check', workerPath], { encoding: 'utf8' });
  assert.equal(checked.status, 0, checked.stderr || checked.stdout);
});

test('Broker connect declares browser family and candidate', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(worker, /BROWSER_FAMILY = navigator\.userAgent\.includes\('Edg\/'\) \? 'edge' : 'chrome'/);
  assert.match(worker, /browser_family: BROWSER_FAMILY/);
  assert.match(worker, /extension_candidate: LOADED_CANDIDATE/);
  assert.match(worker, /browser_session_id: connectionSessionId/);
  assert.match(worker, /worker_instance_id: WORKER_INSTANCE_ID/);
  assert.match(worker, /\/v1\/extension\/connect/);
});

test('Agent tab lifecycle recovers from no current window and survives last-tab close', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const opener = worker.slice(worker.indexOf('async function openAgentTab'), worker.indexOf('async function authorizeTab'));
  assert.match(opener, /chrome\.windows\.getAll\(\{ windowTypes: \['normal'\] \}\)/);
  assert.match(opener, /chrome\.tabs\.create\(\{[\s\S]*windowId: targetWindow\.id/);
  assert.match(opener, /chrome\.windows\.create\(\{[\s\S]*url, type: 'normal'/);
  const close = worker.slice(worker.indexOf("command.kind === 'tabs.close'"), worker.indexOf("command.kind === 'prepare_action'"));
  assert.match(close, /closesLastWindowTab/);
  assert.ok(close.indexOf("reply(command, { tab_id: String(tabId), closed: true })") < close.indexOf('await chrome.tabs.remove(tabId)'));
  assert.match(worker, /chrome\.windows\.onRemoved\.addListener\(\(\) => \{ reconnectAfterWindowRemoval\(\); \}\)/);
  const windowReconnect = worker.slice(worker.indexOf('async function reconnectAfterWindowRemoval'), worker.indexOf('async function brokerRequest'));
  assert.match(windowReconnect, /if \(brokerConnectionId\) return/);
  assert.doesNotMatch(windowReconnect, /\.disconnect\(\)/);
  assert.match(windowReconnect, /await connectBroker\(\)/);
});

test('the Collector reads the full document URL in the same collect as the objects', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  // URL must come from the frame context built during collect, never bolted on
  // afterwards by the Runtime, so document_id + document_url + objects always
  // describe one instant.
  assert.match(collector, /origin: location\.origin, url: location\.href/);
  assert.match(collector, /document_url: parent\.url/);
  // Child same-origin frames carry their own URL so iframe navigation attributes
  // correctly instead of inheriting the top document's URL.
  assert.match(collector, /url: child\.location\.href/);
});

test('a URL change advances the revision even when no object changed', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  // A pure hash or pushState change mutates no DOM. Without this the delta is
  // suppressed and same-document link verification is impossible.
  assert.match(collector, /lastUrlFingerprint/);
  assert.match(collector, /changes\.length === 0\s*&& urlFingerprint === lastUrlFingerprint/);
  // The fingerprint is per frame, so an iframe URL change is not masked by the
  // top document staying put.
  assert.match(collector, /frame\.frame_id.*frame\.document_url/);
});

test('same-document navigation events reach the Collector, and pushState is relayed', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  for (const event of ['hashchange', 'popstate', 'pageshow']) {
    assert.ok(collector.includes(`'${event}'`), `collector must observe ${event}`);
  }
  assert.match(collector, /collector\.recollect/);
  // history.pushState/replaceState never reach an ISOLATED world, so the
  // Service Worker relays chrome.tabs.onUpdated URL changes instead.
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(worker, /change\.url && change\.status === undefined/);
  assert.match(worker, /kind: 'collector\.recollect'/);
});

test('geometry is read in the same collect and viewport_revision tracks geometry only', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  // Read inside collect, emitted on the same snapshot as objects and frames.
  assert.match(collector, /coordinate_space: 'content_viewport'/);
  assert.match(collector, /unit: 'css_px'/);
  assert.match(collector, /viewport_width: document\.documentElement\.clientWidth/);
  assert.match(collector, /device_pixel_ratio: devicePixelRatio/);
  assert.match(collector, /geometry: \{ \.\.\.geometry, viewport_revision: viewportRevision \}/);
  // A DOM-only or URL-only change must not advance viewport_revision.
  assert.match(collector, /if \(!unchanged && geometryChanged\) viewportRevision \+= 1;/);
  assert.ok(!/\n\s*viewportRevision \+= 1;/.test(collector.replace(/if \(!unchanged && geometryChanged\) viewportRevision \+= 1;/, '')),
    'viewport_revision must only advance behind the geometry check');
  // Geometry change alone must still produce an observation.
  assert.match(collector, /&& !geometryChanged/);
});

test('links declare a disposition only when this document cannot verify them', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  // A download or a new browsing context leaves this document's URL untouched,
  // so act must hand off instead of dispatching an unverifiable click.
  assert.match(collector, /hasAttribute\('download'\)/);
  assert.match(collector, /'download'/);
  assert.match(collector, /'new_context'/);
  // An ordinary same-context link declares nothing, keeping the field additive.
  assert.match(collector, /if \(disposition !== 'self'\) object\.navigation_disposition = disposition;/);
});

test('software typing sets a value through the native setter and never reaches protected fields', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /SOFTWARE_TYPE_ROLES/);
  assert.match(collector, /if \(request\.operation === 'type'\) return softType\(request, preflight\);/);
  // Frameworks patch the value property, so assign through the prototype setter
  // or React and Angular never observe the change.
  assert.match(collector, /Object\.getOwnPropertyDescriptor\(prototype, 'value'\)\?\.set/);
  assert.match(collector, /element\.isContentEditable/);
  assert.match(collector, /new view\.InputEvent\('input'/);
  assert.match(collector, /new view\.Event\('change'/);
  // A protected field carries no type affordance, so prepare() refuses it
  // before any software typing can run.
  const textField = fs.readFileSync(path.join(__dirname, '../src/controls/text_field.js'), 'utf8');
  assert.match(textField, /signals\.protected \|\| signals\.enabled === false \|\| signals\.readonly \? \[\] : \['type'\]/);
});

test('software typing dispatches a cancelable beforeinput before any mutation and honors cancellation', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const softType = collector.slice(collector.indexOf('function softType('), collector.indexOf('function softAction('));
  // prepare() already focuses the element for the 'type' operation; softType
  // must not dispatch a second, redundant focus.
  assert.ok(!/\.focus\(/.test(softType), 'softType must not call focus itself — prepare() already did');
  // beforeinput must be dispatched, cancelable, and its result checked before
  // any value or DOM mutation runs.
  const beforeinputIndex = softType.indexOf("new view.InputEvent('beforeinput'");
  const cancelableIndex = softType.indexOf('cancelable: true');
  const checkIndex = softType.indexOf('if (!proceed)');
  const mutationIndex = softType.indexOf('replaceContentEditable(element, text)');
  assert.ok(beforeinputIndex >= 0, 'beforeinput must be dispatched');
  assert.ok(cancelableIndex >= 0 && cancelableIndex < mutationIndex, 'beforeinput must be cancelable');
  assert.ok(checkIndex >= 0 && checkIndex < mutationIndex,
    'the dispatchEvent result must be checked before any mutation');
  assert.match(softType, /const proceed = element\.dispatchEvent\(new view\.InputEvent\('beforeinput'/);
  assert.match(softType, /if \(!proceed\) throw actionFailure\('dispatch', 'page_canceled_beforeinput', false/);
});

test('software typing is frame-realm safe and verifies rich text without returning its contents', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const softType = collector.slice(collector.indexOf('function normalizedEditableValue('), collector.indexOf('function linkedFileInput('));
  assert.match(softType, /const view = element\.ownerDocument\.defaultView/);
  assert.match(softType, /view\.HTMLTextAreaElement\.prototype/);
  assert.match(softType, /view\.HTMLInputElement\.prototype/);
  assert.match(softType, /document\.createRange\(\)/);
  assert.match(softType, /document\.execCommand\('insertText', false, text\)/);
  assert.match(softType, /await waitForEditableValue\(element, text, request\.timeout_ms\)/);
  assert.match(softType, /code: element\.isContentEditable \? 'editable_content_observed' : 'field_value_observed'/);
  assert.match(softType, /verified: locallyVerified/);
  assert.doesNotMatch(softType, /semantic_postcondition[\s\S]{0,200}(?:text|value): text/);
});

test('form toggles and choices return value-free local semantic postconditions', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /code: 'toggle_state_observed'/);
  assert.match(collector, /verified: toggleAfter !== toggleBefore/);
  assert.match(collector, /code: 'selection_state_observed'/);
  assert.match(collector, /option\.selected === true && target\.selectedIndex === option\.index/);
  assert.doesNotMatch(collector, /semantic_postcondition:\s*\{[^}]*value:/);
});

test('software preparation keeps a zero-wait fast path and bounds local actionability waiting', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const prepare = collector.slice(collector.indexOf('function prepare('), collector.indexOf('function softClick('));
  assert.match(prepare, /request\.defer_scroll === true/);
  assert.match(prepare, /scrollIntoView/);
  assert.match(prepare, /const focusElement = target\.controlElement \|\| target\.element/);
  assert.match(prepare, /request\.operation === 'type' \|\| request\.operation === 'select'/);
  assert.match(prepare, /focusElement\.focus\(\{ preventScroll: true \}\)/);
  assert.match(prepare, /focusElement\.ownerDocument\.activeElement === focusElement/);
  assert.doesNotMatch(prepare, /top\.document\.hasFocus\(\)/);
  assert.match(prepare, /function waitForSoftwarePreparation/);
  assert.match(prepare, /function softwarePreparationPolicy/);
  assert.match(prepare, /request\.operation === 'click' && target\.role === 'reflex_target'/);
  assert.match(prepare, /const focusRequired = request\.operation === 'type' \|\| request\.operation === 'select'/);
  assert.match(prepare, /require_topmost: !reflexClick/);
  assert.match(prepare, /require_focus: focusRequired/);
  assert.match(prepare, /require_stable_geometry: !reflexClick/);
  assert.match(prepare, /!policy\.require_stable_geometry \|\| !targetGeometryIsAnimating/);
  assert.match(prepare, /function currentSoftwareRequest\(request\)/);
  assert.match(prepare, /request\.basis_revision < revision/);
  assert.match(prepare, /request\.document_id === documentId/);
  assert.match(prepare, /target\.objectId === request\.object_id/);
  assert.match(prepare, /target\.affordances\.includes\(request\.operation\)/);
  assert.match(prepare, /activeRequest = currentSoftwareRequest\(activeRequest\)/);
  assert.match(prepare, /function waitForPreparationFrame\(deadline\)/);
  assert.match(prepare, /requestAnimationFrame\(\(\) => finish\(true\)\)/);
  assert.match(prepare, /setTimeout\(\(\) => \{\s*cancelAnimationFrame\(frameId\);\s*finish\(false\);/s);
  assert.match(prepare, /if \(!await waitForPreparationFrame\(deadline\)\) break/);
  assert.match(prepare, /!policy\.require_stable_geometry \|\| stableFrames >= 2/);
  assert.match(prepare, /collect\(\);\s+activeRequest = currentSoftwareRequest\(activeRequest\);\s+prepared = prepare\(activeRequest\)/);
  assert.match(prepare, /actionability_timeout_/);
  assert.match(prepare, /request\.timeout_ms/);
  assert.match(prepare, /targetEnabled/);
  assert.match(prepare, /prepared\.local_wait_ms = 0/);
  assert.match(prepare, /performance\.now\(\) - startedAt/);
  const softType = collector.slice(collector.indexOf('function softType('), collector.indexOf('function softAction('));
  assert.match(softType, /await waitForSoftwarePreparation\(request\)/);
  assert.match(softType, /local_wait_ms: prepared\.local_wait_ms/);
  assert.match(collector, /dispatch_document_id: prepared\.document_id/);
  assert.match(collector, /dispatch_basis_revision: prepared\.basis_revision/);
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(worker, /saccade_action_error\|\$\{stage\}\|\$\{code\}\|\$\{retrySafe\}/);
});

test('form batch preflights all steps then revalidates each exact token before dispatch', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const batch = collector.slice(
    collector.indexOf('async function softActionBatch('),
    collector.indexOf('function schedule()'),
  );
  const preflight = batch.indexOf('prepared.push(await waitForSoftwarePreparation');
  const dispatch = batch.indexOf('const result = await softAction({ ...step, timeout_ms: remaining })');
  assert.ok(preflight >= 0 && preflight < dispatch, 'the complete batch must preflight before dispatch');
  assert.match(batch, /if \(index > 0\) collect\(\)/);
  assert.match(batch, /partial_dispatch: receipts\.length > 0/);
  assert.match(batch, /retry_safe: receipts\.length === 0/);
  assert.doesNotMatch(batch, /softAction\(step, prepared\[index\]\)/);
});

test('native preparation removes symmetric window borders from the content origin', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const origin = collector.slice(
    collector.indexOf('function contentScreenOrigin('),
    collector.indexOf('function prepare('),
  );
  assert.match(origin, /\(outerWidth - innerWidth\) \/ 2/);
  assert.match(origin, /x: screenX \+ sideBorder/);
  assert.match(origin, /outerHeight - innerHeight - sideBorder/);
  assert.doesNotMatch(collector, /x: screenX \+ topBox\.x/);
});

test('the browser harness edits through the same statements as the collector', () => {
  // The harness is what Chrome and Edge actually execute to prove the event
  // order. It only proves anything about the product while it edits the way
  // the collector edits, so the shared statements are compared directly.
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const harness = fs.readFileSync(
    path.join(__dirname, '../../fixtures/controls/software_type_harness.html'), 'utf8');
  const shared = [
    'const view = element.ownerDocument.defaultView;',
    "const proceed = element.dispatchEvent(new view.InputEvent('beforeinput', {",
    "bubbles: true, cancelable: true, composed: true, inputType: 'insertText', data: text,",
    'if (element.isContentEditable) {',
    'replaceContentEditable(element, text);',
    "const prototype = element.tagName === 'TEXTAREA'",
    '? view.HTMLTextAreaElement.prototype : view.HTMLInputElement.prototype;',
    "const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;",
    'if (setter) setter.call(element, text); else element.value = text;',
    "element.dispatchEvent(new view.InputEvent('input', {",
    "element.dispatchEvent(new view.Event('change', { bubbles: true }));",
  ];
  for (const statement of shared) {
    assert.ok(collector.includes(statement), `collector must contain: ${statement}`);
    assert.ok(harness.includes(statement), `harness has drifted from the collector: ${statement}`);
  }
  assert.match(collector, /if \(!proceed\) throw actionFailure/);
  assert.match(harness, /if \(!proceed\) throw new Error/);
});
