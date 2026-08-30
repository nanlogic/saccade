const assert = require('node:assert/strict');
const test = require('node:test');

const {
  boundedOutput,
  parseBrowsers,
  releasePassed,
} = require('../../../scripts/gate_node_release_candidate');

test('release candidate gate requires the complete Chrome and Edge pair', () => {
  assert.deepEqual(parseBrowsers(), ['chrome', 'edge']);
  assert.deepEqual(parseBrowsers('edge,chrome'), ['edge', 'chrome']);
  assert.throws(() => parseBrowsers('chrome'), /exactly/);
  assert.throws(() => parseBrowsers('chrome,chrome'), /exactly/);
});

test('public compatibility failures remain visible but cannot fail deterministic release gates', () => {
  const staticChecks = [{ passed: true }];
  assert.equal(releasePassed(staticChecks, [
    { id: 'chrome-deterministic', passed: true, blocking: true },
    { id: 'chrome-public-forms', passed: false, blocking: false },
  ]), true);
  assert.equal(releasePassed(staticChecks, [
    { id: 'edge-standard-upload', passed: false, blocking: true },
  ]), false);
});

test('release candidate diagnostics remain bounded', () => {
  assert.equal(boundedOutput('safe'), 'safe');
  const bounded = boundedOutput('x'.repeat(4_000), 100);
  assert.equal(bounded.length, 100);
});

test('release candidate gate includes exact-session live isolation', () => {
  const source = require('node:fs').readFileSync(
    require('node:path').resolve(__dirname, '../../../scripts/gate_node_release_candidate.js'),
    'utf8',
  );
  assert.match(source, /probe_node_session_isolation\.js/);
  assert.match(source, /browser_instance_id/);
  assert.match(source, /fixture_server\.py/);
  assert.match(source, /--directory/);
});

test('release probes use Broker semantic waits instead of model polling loops', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  for (const name of [
    'probe_node_release_smoke.js', 'probe_node_public_forms.js', 'probe_node_public_truth.js',
  ]) {
    const source = fs.readFileSync(path.resolve(__dirname, '../../../scripts', name), 'utf8');
    assert.match(source, /min_objects/);
    assert.match(source, /timeout_ms/);
    assert.doesNotMatch(source, /while \(\(view\.objects \|\| \[\]\)\.length/);
  }
  const forms = fs.readFileSync(
    path.resolve(__dirname, '../../../scripts/probe_node_public_forms.js'), 'utf8',
  );
  assert.match(forms, /angular-material-public-select/);
  assert.match(forms, /option_object_id/);
  assert.match(forms, /withFreshPrepare/);
  assert.match(forms, /prepareStale/);
  assert.match(forms, /semanticKey\(label\(object\)\) === semanticKey\(name\)/);
  assert.match(forms, /toLocaleLowerCase\('en-US'\)/);
  assert.match(forms, /findContextActionObject/);
  assert.match(forms, /Basic mat-select/);
  assert.match(forms, /semanticMatches\.length === 1/);
  assert.match(forms, /\}, 10\)/);
  assert.match(forms, /saccade_release_probe=\$\{Date\.now\(\)\}/);
  assert.doesNotMatch(forms, /retry_safe:\s*true/);
});

test('deterministic canvas targets expose page-owned reflex occurrence', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const fixture = fs.readFileSync(
    path.resolve(__dirname, '../../../fixtures/conformance/mouse_accuracy.html'), 'utf8',
  );
  assert.match(fixture, /data-saccade-reflex-occurrence/);
  assert.match(fixture, /pressed \? '1' : '0'/);
});
