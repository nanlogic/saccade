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
    'local-legacy-admin-workflow',
    'authenticated-steamworks-workflows',
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

test('legacy admin mechanics are blocking while authenticated Steamworks remains truthful', () => {
  const suites = new Map(manifest.suites.map((suite) => [suite.id, suite]));
  const legacyAdmin = suites.get('local-legacy-admin-workflow');
  assert.equal(legacyAdmin.class, 'deterministic');
  assert.equal(legacyAdmin.blocking, true);
  assert.ok(legacyAdmin.requirements.includes('same_origin_iframe_rich_text'));
  assert.ok(legacyAdmin.requirements.includes('save_is_separate_from_form_batch'));

  const standard = suites.get('local-standard-upload');
  assert.equal(standard.class, 'deterministic');
  assert.equal(standard.blocking, true);
  assert.ok(standard.requirements.includes('object_addressed_file_upload'));
  assert.ok(standard.requirements.includes('workspace_bounded_absolute_path'));
  assert.ok(standard.requirements.includes('dynamic_file_chooser_capture'));
  assert.ok(standard.requirements.includes('file_selection_observed'));

  const steamworks = suites.get('authenticated-steamworks-workflows');
  assert.equal(steamworks.class, 'authenticated_human_authorized');
  assert.equal(steamworks.blocking, false);
  assert.equal(steamworks.expected_outcome,
    'generic_controls_execute_and_verify_or_truthfully_stop_at_boundary');
  for (const requirement of [
    'same_origin_iframe_rich_text',
    'dynamic_choice_refresh_without_rebinding',
    'object_addressed_upload',
    'save_submit_navigation_are_separate_actions',
    'page_owned_postcondition_or_outcome_unknown',
  ]) assert.ok(steamworks.requirements.includes(requirement));
  assert.ok(steamworks.requirements.includes('truthful_unsupported_or_external_execution_required'));
  assert.ok(steamworks.requirements.includes('no_false_persistence_claim'));
  assert.ok(steamworks.requirements.includes('no_automatic_replay'));
  assert.ok(steamworks.limitations.includes('no_cdp_native_host_platform_driver_or_site_specific_api'));
  assert.ok(steamworks.cases.includes('builds_depots_packages_and_branches'));
  assert.ok(steamworks.cases.includes('checklist_save_review_and_publish_transitions'));
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
