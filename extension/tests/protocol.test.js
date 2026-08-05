'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const { HOST_PROTOCOL, OBSERVATION_SCHEMA, envelope, parseHostMessage, randomToken } = require('../src/protocol.js');
const { normalizeOrigin, isProtectedFieldType } = require('../src/consent.js');
const { compileChanges } = require('../src/truth_delta.js');

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

test('consent helpers normalize origins and recognize protected field types', () => {
  assert.equal(normalizeOrigin('https://Example.test/path'), 'https://example.test');
  assert.equal(normalizeOrigin('not a URL'), null);
  assert.equal(isProtectedFieldType('password'), true);
  assert.equal(isProtectedFieldType('text', 'one-time-code'), true);
  assert.equal(isProtectedFieldType('text', 'name'), false);
});

test('development manifest preserves identity and excludes out-of-scope capabilities', () => {
  const manifest = JSON.parse(fs.readFileSync(path.join(__dirname, '../manifest.json'), 'utf8'));
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  assert.equal(manifest.manifest_version, 3);
  const digest = crypto.createHash('sha256').update(Buffer.from(manifest.key, 'base64')).digest('hex').slice(0, 32);
  const extensionId = [...digest].map((digit) => String.fromCharCode(97 + Number.parseInt(digit, 16))).join('');
  assert.equal(extensionId, 'bobfbgjplflcigednmccmbhlgclomgod');
  assert.deepEqual(manifest.permissions, ['tabs', 'nativeMessaging', 'storage']);
  assert.equal(manifest.content_scripts.length, 1);
  assert.equal(manifest.content_scripts[0].run_at, 'document_start');
  assert.equal(manifest.content_scripts[0].world, 'ISOLATED');
  assert.equal(manifest.content_scripts[0].js.at(-1), 'src/collector.js');
  assert.ok(manifest.content_scripts[0].js.indexOf('src/truth_delta.js') < manifest.content_scripts[0].js.indexOf('src/collector.js'));
  assert.equal(manifest.action.default_popup, 'popup.html');
  assert.match(worker, /com\.nanlogic\.saccade\.dev/);
  assert.doesNotMatch(worker, /chrome\.(downloads|debugger|scripting)/);
  assert.doesNotMatch(worker, /Playwright|CDP|protected_fill|loop\.start/);
});

test('tab sharing UI mutates only the session ACL and revocation clears collector authority', () => {
  const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  const popup = fs.readFileSync(path.join(__dirname, '../popup.js'), 'utf8');
  assert.match(worker, /sender\.url !== chrome\.runtime\.getURL\('popup\.html'\)/);
  assert.match(worker, /userSharedTabs\.add\(tabId\)/);
  assert.match(worker, /userSharedTabs\.delete\(tabId\)/);
  assert.match(worker, /collector\.deauthorize/);
  assert.match(worker, /Agent-owned tabs are revoked by closing the tab/);
  assert.match(collector, /function deauthorize/);
  assert.match(collector, /tokenTargets\.clear\(\)/);
  assert.match(popup, /ui\.tab\.share/);
  assert.match(popup, /ui\.tab\.revoke/);
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

test('Extension compiler emits semantic Truth Layer deltas and ignores authority churn', () => {
  const collector = fs.readFileSync(path.join(__dirname, '../src/collector.js'), 'utf8');
  assert.match(collector, /compiledObjects && changes\.length === 0/);
  const base = {
    object_id: 'o.internal', object_revision: 1, role: 'button', kind: 'control',
    name: 'Save', state: { pressed: 'false' }, affordances: ['click'],
    action_token: 'action.old', document_bounds: { x: 1, y: 1, width: 10, height: 10 },
  };
  const authorityOnly = { ...base, object_revision: 2, action_token: 'action.new', document_bounds: { x: 2, y: 2, width: 10, height: 10 } };
  assert.deepEqual(compileChanges([base], [authorityOnly]), []);
  const changed = { ...authorityOnly, state: { pressed: 'true' } };
  assert.deepEqual(compileChanges([base], [changed]), [
    { kind: 'updated', object_id: 'o.internal', object_revision: 2 },
  ]);
  assert.equal(compileChanges([], [base])[0].kind, 'appeared');
  assert.equal(compileChanges([base], [])[0].kind, 'disappeared');
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
  const open = worker.slice(worker.indexOf("command.kind === 'tabs.open'"), worker.indexOf("command.kind === 'prepare_action'"));
  assert.ok(open.indexOf('agentOwnedTabs.add(tab.id)') < open.indexOf('reply(command'));
  assert.match(open, /isSupportedUrl\(current\.url\).*authorizeTab\(tab\.id\)/s);
  assert.match(worker, /change\.status === 'loading'.*sessions\.delete\(tabId\)/s);
  assert.match(worker, /change\.url \|\| change\.status === 'loading' \|\| change\.status === 'complete'/);
  assert.match(worker, /attempt < 40/);
  assert.doesNotMatch(worker, /executeScript/);
  assert.match(worker, /authorizationPromises\.get\(tabId\)/);
  assert.match(worker, /existing\?\.url === tab\.url/);
  assert.match(worker, /existing\.promise\.catch\(\(\) => \{\}\)\.then/);
  assert.match(worker, /session\?\.url === tab\.url && \(session\.configuring \|\| session\.configured\)/);
  assert.match(worker, /tab URL changed during collector authorization/);
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
  assert.match(collector, /SCORE\\s\*\(\\d\+\)/);
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
