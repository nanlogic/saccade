const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const ROOT = path.resolve(__dirname, '../../..');
const manifest = JSON.parse(fs.readFileSync(
  path.join(ROOT, 'conformance/release-0.2.0.json'),
  'utf8',
));

test('0.2.0 release matrix freezes the Node-only two-browser route', () => {
  assert.equal(manifest.schema, 'saccade.release-conformance/1');
  assert.equal(manifest.release, '0.2.0');
  assert.deepEqual(manifest.candidate_policy.browsers, ['chrome', 'edge']);
  assert.equal(manifest.candidate_policy.same_extension_candidate_required, true);
  assert.equal(manifest.candidate_policy.production_route, 'extension_node_broker_mcp');
  for (const route of ['rust', 'native_host', 'platform_driver', 'playwright', 'cdp']) {
    assert.ok(manifest.candidate_policy.forbidden_routes.includes(route));
  }
});

test('0.2.0 release matrix contains every required product lane', () => {
  const suites = new Map(manifest.suites.map((suite) => [suite.id, suite]));
  for (const id of [
    'local-controls-and-forms',
    'local-tables',
    'local-frames',
    'local-mouse-accuracy',
    'public-framework-forms',
    'public-heavy-pages',
    'local-standard-upload',
    'authenticated-steamworks-upload',
    'transport-and-isolation',
  ]) {
    assert.ok(suites.has(id), `missing release suite ${id}`);
  }

  assert.ok(suites.get('public-framework-forms').cases.includes('angular-material-public-select'));
  assert.ok(suites.get('public-framework-forms').cases.includes('demoqa-react-practice-form'));
  assert.ok(suites.get('public-heavy-pages').cases.includes('bestbuy_homepage_navigation'));
  assert.equal(suites.get('public-framework-forms').blocking, false);
  assert.equal(suites.get('public-heavy-pages').blocking, false);
});

test('standard upload is blocking while trusted Steamworks upload is a truthful limitation', () => {
  const suites = new Map(manifest.suites.map((suite) => [suite.id, suite]));
  const standard = suites.get('local-standard-upload');
  assert.equal(standard.class, 'deterministic');
  assert.equal(standard.blocking, true);
  assert.ok(standard.requirements.includes('object_addressed_file_upload'));
  assert.ok(standard.requirements.includes('workspace_bounded_absolute_path'));
  assert.ok(standard.requirements.includes('dynamic_file_chooser_capture'));
  assert.ok(standard.requirements.includes('file_selection_observed'));

  const steamworks = manifest.suites.find((suite) => suite.id === 'authenticated-steamworks-upload');
  assert.equal(steamworks.class, 'authenticated_human_authorized');
  assert.equal(steamworks.blocking, false);
  assert.equal(steamworks.expected_outcome,
    'external_execution_required_when_native_trust_is_required');
  assert.ok(steamworks.requirements.includes('truthful_unsupported_or_external_execution_required'));
  assert.ok(steamworks.requirements.includes('no_false_persistence_claim'));
  assert.ok(steamworks.requirements.includes('no_automatic_replay'));
  assert.ok(steamworks.limitations.includes('no_cdp_native_host_platform_driver_or_site_specific_api'));
  for (const boundary of ['password', 'otp', 'captcha', 'payment', 'publish']) {
    assert.ok(steamworks.stopping_points.includes(boundary));
  }
});

test('historical evidence cannot satisfy the release gate', () => {
  assert.equal(manifest.evidence_policy.fresh_candidate_evidence_only, true);
  assert.equal(manifest.evidence_policy.historical_reports_are_not_release_proof, true);
  assert.equal(manifest.evidence_policy.editable_values_retained, false);
  assert.equal(manifest.evidence_policy.credentials_retained, false);
  assert.equal(manifest.evidence_policy.non_blocking_compatibility_suites_cannot_fail_the_release, true);
});
