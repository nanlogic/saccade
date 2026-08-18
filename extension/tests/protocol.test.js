'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { HOST_PROTOCOL, OBSERVATION_SCHEMA, envelope, parseHostMessage, randomToken } = require('../src/protocol.js');
const { normalizeOrigin, isProtectedFieldType, redactProtectedText } = require('../src/consent.js');
const { compileChanges } = require('../src/truth_delta.js');
const registry = require('../src/controls/registry.js');

test('Native Messaging envelope preserves the v1 wire names', () => {
  assert.deepEqual(envelope('hello', { browser_instance_id: 'browser.test' }, 7), {
    protocol: HOST_PROTOCOL, kind: 'hello', payload: { browser_instance_id: 'browser.test' }, request_id: 7,
  });
  assert.equal(HOST_PROTOCOL, 'saccade-extension-host/1');
  assert.equal(OBSERVATION_SCHEMA, 'saccade.observation/1');
  assert.equal(parseHostMessage({ protocol: 'wrong', kind: 'tabs.list' }), null);
  assert.equal(parseHostMessage({ protocol: HOST_PROTOCOL, kind: 'tabs.list', request_id: -1 }), null);
  assert.equal(parseHostMessage({ protocol: HOST_PROTOCOL, kind: 'tabs.list', selector: 'button' }), null);
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

test('development manifest preserves identity and excludes out-of-scope capabilities', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '../manifest.json'), 'utf8'));
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.equal(manifest.manifest_version, 3);
  const digest = crypto.createHash('sha256').update(Buffer.from(manifest.key, 'base64')).digest('hex').slice(0, 32);
  const extensionId = [...digest].map((digit) => String.fromCharCode(97 + Number.parseInt(digit, 16))).join('');
  assert.equal(extensionId, 'bobfbgjplflcigednmccmbhlgclomgod');
  assert.deepEqual(manifest.permissions, ['tabs', 'nativeMessaging', 'storage', 'alarms']);
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
  assert.match(worker, /com\.nanlogic\.saccade\.dev/);
  assert.match(worker, /com\.nanlogic\.saccade'/);
  assert.match(worker, /getManifest\(\)\.name\.includes\('\(Development\)'\)/);
  assert.match(worker, /extension_candidate: LOADED_CANDIDATE/);
  assert.match(worker, /reloadIfCandidateChanged/);
  assert.match(worker, /sameCandidate\(ping\.extension_candidate\)/);
  assert.match(collector, /extension_candidate: globalThis\.SaccadeCandidate/);
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
  assert.match(collector, /\(dialogText \|\| genericText\) \? 'text'/);
  assert.match(collector, /state\.modal = String\(element\.getAttribute\('aria-modal'\) === 'true'\)/);
  assert.match(collector, /deferred_content_possible/);
  assert.match(collector, /transitionend/);
  assert.match(collector, /animationend/);
  assert.match(collector, /kind: 'text', role, text, state, affordances: \[\], protected: false/);
  assert.match(collector, /element\.closest\(CONTROL_SELECTOR\)/);
  assert.match(collector, /TextEncoder/);
  assert.match(collector, /document\.readyState === 'loading'/);
  assert.match(collector, /document\.readyState === 'loading'\) \{\s*schedule\(\);\s*document\.addEventListener\('DOMContentLoaded', collect/s);
  assert.doesNotMatch(collector, /function collect\(\) \{\s*if \(!config\) return null;\s*if \(document\.readyState === 'loading'\) return null;/s);
  assert.match(collector, /document\.readyState === 'loading'\) \{\s*for \(const object of objects\).*object\.affordances = \[\].*delete object\.action_token.*tokenTargets\.clear\(\)/s);
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

test('editable placeholders are explicitly distinguished from current values', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /return `Placeholder: \$\{placeholder\}`/);
  assert.doesNotMatch(collector, /if \(placeholder && placeholder !== name\) return placeholder;/);
});

test('Extension compiler emits semantic and geometry Truth Layer deltas while ignoring authority churn', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /compiledObjects && changes\.length === 0/);
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
  assert.match(worker, /async function resetAclForBrowserStartup/);
  assert.match(worker, /chrome\.storage\.local\.remove\(TAB_ACL_KEY\)/);
  assert.match(worker, /chrome\.runtime\.onStartup\.addListener.*resetAclForBrowserStartup\(\)\.then\(connectHost\)/s);
  assert.match(worker, /connectHost\(\)\.catch\(scheduleReconnect\);\s*$/);
  assert.doesNotMatch(worker, /bootTimer|setTimeout\(\(\) => \{ connectHost/);
  assert.doesNotMatch(worker, /chrome\.storage\.session\.(get|set)\(TAB_ACL_KEY/);
});

test('prepare checks the revision basis after tab activation and focus', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(worker, /if \(!browserWindow\.focused\).*chrome\.windows\.update/s);
  assert.match(worker, /if \(!tab\.active\).*chrome\.tabs\.update/s);
  assert.ok(worker.indexOf('chrome.windows.update') < worker.indexOf("kind: 'collector.prepare_action'"));
  assert.match(collector, /request\.basis_revision !== revision/);
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
  assert.match(collector, /software click is not registered for the current control/);
  assert.ok(collector.indexOf('prepare(request);') < collector.indexOf('target.element.dispatchEvent'));
  assert.match(collector, /reflexOccurrence/);
  assert.match(collector, /isMouseAccuracyGame\(document\).*scheduleVisual/s);
  assert.match(collector, /target\.element\.dispatchEvent/);
  assert.match(worker, /command\.kind === 'soft_action'/);
  assert.match(collector, /choiceOwner\(option\) !== target/);
  assert.match(collector, /option\.selected = true/);
  assert.match(collector, /Array\(prepared\.selection_index\)\.fill\('ArrowDown'\)/);
  assert.match(collector, /requestAnimationFrame\(collect\)/);
  assert.match(worker, /command\.kind === 'soft_click'/);
  assert.match(worker, /collector\.soft_click/);
});

test('managed Chrome and Edge routes share one protocol and keep browser evidence separate', () => {
  const dev = fs.readFileSync(path.join(__dirname, '../../scripts/dev.sh'), 'utf8');
  const probe = fs.readFileSync(path.join(__dirname, '../../scripts/dev_probe.py'), 'utf8');
  const host = fs.readFileSync(path.join(__dirname, '../../scripts/dev/com.nanlogic.saccade.dev.json.in'), 'utf8');
  assert.match(dev, /Microsoft Edge\/NativeMessagingHosts/);
  assert.match(dev, /profile-v\$BROWSER_PROFILE_GENERATION/);
  assert.match(dev, /--disable-session-crashed-bubble/);
  assert.match(dev, /--window-size=800,747/);
  assert.match(dev, /profile\["exit_type"\] = "Normal"/);
  assert.match(dev, /test \[chrome\|edge\|all\]/);
  assert.match(dev, /compare \[chrome\|edge\|all\]/);
  assert.match(dev, /external_dogfood\.py/);
  assert.match(dev, /reference\/playwright/);
  assert.match(dev, /accuracy \[chrome\|edge\|all\]/);
  assert.match(probe, /mouse_accuracy/);
  assert.match(probe, /ACCURACY_WINDOW_PHASES/);
  assert.match(dev, /--window-pid/);
  assert.match(dev, /EVIDENCE_DIR\/\$test_stamp\/\$test_browser/);
  assert.match(probe, /--browser/);
  assert.match(probe, /"browser": browser/);
  assert.match(host, /chrome-extension:\/\/bobfbgjplflcigednmccmbhlgclomgod\//);
});

test('Native Host ownership conflicts recover without requiring a popup wake-up', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const reconnect = worker.slice(worker.indexOf('function scheduleReconnect'), worker.indexOf('async function connectHost'));
  assert.match(reconnect, /Math\.min\(250 \* \(2 \*\* reconnectAttempts\+\+\), 4000\)/);
  assert.doesNotMatch(reconnect, /reconnectAttempts\s*>=/);
  assert.match(reconnect, /chrome\.alarms\.create\(RECONNECT_ALARM/);
  assert.match(worker, /RECONNECT_ALARM_DELAY_MS = 30_000/);
  assert.match(worker, /async function settleReconnect\(port\)/);
  assert.match(worker, /chrome\.windows\.getAll\(\{ windowTypes: \['normal'\] \}\)/);
  assert.match(worker, /if \(windows\.length\) chrome\.alarms\.clear\(RECONNECT_ALARM\)/);
  assert.match(worker, /chrome\.alarms\.onAlarm\.addListener/);
});

test('Native Host hello supplies a fixed browser lifecycle wake route', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.match(worker, /BROWSER_FAMILY = navigator\.userAgent\.includes\('Edg\/'\) \? 'edge' : 'chrome'/);
  assert.match(worker, /browser_family: BROWSER_FAMILY/);
  assert.match(worker, /development: NATIVE_HOST\.endsWith\('\.dev'\)/);
  assert.match(worker, /wake_url: chrome\.runtime\.getURL\('popup\.html'\)/);
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
  const windowReconnect = worker.slice(worker.indexOf('async function reconnectAfterWindowRemoval'), worker.indexOf('async function connectHost'));
  assert.match(windowReconnect, /if \(nativePort\) return/);
  assert.doesNotMatch(windowReconnect, /\.disconnect\(\)/);
  assert.match(windowReconnect, /await connectHost\(\)/);
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
  assert.match(collector, /changes\.length === 0 && urlFingerprint === lastUrlFingerprint/);
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
  assert.match(collector, /if \(geometryChanged\) viewportRevision \+= 1;/);
  assert.ok(!/\n\s*viewportRevision \+= 1;/.test(collector.replace(/if \(geometryChanged\) viewportRevision \+= 1;/, '')),
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
  assert.match(collector, /if \(request\.operation === 'type'\) return softType\(request\);/);
  // Frameworks patch the value property, so assign through the prototype setter
  // or React and Angular never observe the change.
  assert.match(collector, /Object\.getOwnPropertyDescriptor\(prototype, 'value'\)\?\.set/);
  assert.match(collector, /element\.isContentEditable/);
  assert.match(collector, /new Event\('input'/);
  assert.match(collector, /new Event\('change'/);
  // A protected field carries no type affordance, so prepare() refuses it
  // before any software typing can run.
  const textField = fs.readFileSync(path.join(__dirname, '../src/controls/text_field.js'), 'utf8');
  assert.match(textField, /signals\.protected \|\| signals\.enabled === false \|\| signals\.readonly \? \[\] : \['type'\]/);
});
