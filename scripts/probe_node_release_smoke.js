#!/usr/bin/env node
'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const {
  closeSession,
  createSession,
  rpc,
} = require('../packages/setup/src/broker_client');
const {
  optionValue,
  releaseTargetFromCapabilities,
} = require('./node_release_target');

const ARGS = process.argv.slice(2);
const BASE_URL = optionValue(ARGS, 'base-url') || 'http://127.0.0.1:8765';
const ROOT = path.resolve(__dirname, '..');
let requestId = 1;
let currentStage = 'startup';

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function objectLabel(object) {
  return object.name || object.text || '';
}

function uniqueObject(view, role, name) {
  const matches = (view.objects || []).filter(
    (object) => object.role === role && objectLabel(object) === name,
  );
  assert(matches.length === 1, `expected one ${role}:${name}, found ${matches.length}`);
  return matches[0];
}

function cacheBustedFixture(requestPath) {
  const relative = requestPath.split('?', 1)[0].replace(/^\//, '');
  const digest = crypto.createHash('sha256')
    .update(fs.readFileSync(path.join(ROOT, relative))).digest('hex').slice(0, 16);
  return `${requestPath}${requestPath.includes('?') ? '&' : '?'}fixture_digest=${digest}`;
}

async function main() {
  const session = await createSession();
  const tabs = new Set();
  const call = (method, params = {}, timeoutMs = 10_000) => (
    rpc(session, method, params, timeoutMs, requestId++)
  );
  let releaseTarget;
  const open = async (requestPath) => {
    const result = await call('tabs.open', {
      url: `${BASE_URL}${cacheBustedFixture(requestPath)}`, active: true,
      browser_instance_id: releaseTarget.browser_instance_id,
    }, 25_000);
    tabs.add(String(result.tab_id));
    return result;
  };
  const read = (tabId, query) => call('truth.read', {
    tab_id: String(tabId),
    mode: 'full',
    ...(query ? { query } : {}),
  });
  const waitForQuery = async (tabId, query, minimum = 1) => {
    return call('truth.read', {
      tab_id: String(tabId), mode: 'full', query,
      min_objects: minimum, timeout_ms: 10_000,
    }, 10_000);
  };
  const close = async (tabId) => {
    await call('tabs.close', { tab_id: String(tabId) });
    tabs.delete(String(tabId));
  };
  const report = {
    schema: 'saccade.node-release-smoke/1',
    base_url: BASE_URL,
    suites: {},
  };

  try {
    const capabilities = await call('system.capabilities');
    currentStage = 'capabilities';
    releaseTarget = releaseTargetFromCapabilities(capabilities, ARGS);
    assert(capabilities.runtime === 'node', 'runtime is not Node');
    assert(capabilities.rust === false, 'Rust route is present');
    assert(capabilities.native_host === false, 'Native Host route is present');
    assert(capabilities.extension_connected === true, 'Extension is disconnected');
    report.capabilities = {
      runtime: capabilities.runtime,
      rust: capabilities.rust,
      native_host: capabilities.native_host,
      extension_connected: capabilities.extension_connected,
      browser_family: releaseTarget.browser,
      browser_instance_id: releaseTarget.browser_instance_id,
      extension_candidate: releaseTarget.extension_candidate,
      exact_tab_routing: capabilities.exact_tab_routing,
    };

    {
      currentStage = 'frames-and-shadow';
      const opened = await open('/fixtures/structural/frames_and_shadow.html');
      const frameView = await waitForQuery(opened.tab_id, {
        text: 'Frame toggle', roles: ['button'], max_objects: 4,
      });
      const shadowView = await waitForQuery(opened.tab_id, {
        text: 'Open shadow toggle', roles: ['button'], max_objects: 4,
      });
      const opaqueView = await read(opened.tab_id, { text: 'Opaque button', max_objects: 4 });
      const observed = (frameView.frames || []).filter((frame) => frame.status === 'observed');
      const restricted = (frameView.frames || []).filter((frame) => frame.status !== 'observed');
      uniqueObject(frameView, 'button', 'Frame toggle');
      uniqueObject(shadowView, 'button', 'Open shadow toggle');
      assert(!(opaqueView.objects || []).some((object) => objectLabel(object) === 'Opaque button'),
        'opaque iframe descendant leaked into Truth');
      assert(observed.length === 2, `expected two observed frames, found ${observed.length}`);
      assert(restricted.length === 1, `expected one restricted frame, found ${restricted.length}`);
      report.suites.frames = {
        passed: true,
        observed_frames: observed.length,
        restricted_frames: restricted.length,
        same_origin_object: true,
        open_shadow_object: true,
      };
      await close(opened.tab_id);
    }

    {
      currentStage = 'semantic-table';
      const opened = await open('/fixtures/controls/all.html?release-smoke=table');
      const view = await read(opened.tab_id, {
        roles: ['table', 'row', 'cell'],
        max_objects: 32,
      });
      const roles = new Set((view.objects || []).map((object) => object.role));
      for (const role of ['table', 'row', 'cell']) assert(roles.has(role), `table Truth omitted ${role}`);
      report.suites.table = {
        passed: true,
        match_count: view.match_count,
        roles: [...roles].sort(),
      };
      await close(opened.tab_id);
    }

    {
      currentStage = 'form-batch';
      const opened = await open('/fixtures/controls/software_type.html');
      const view = await read(opened.tab_id, {
        roles: ['text_field', 'text_area', 'content_editable'],
        max_objects: 32,
      });
      const plain = uniqueObject(view, 'text_field', 'Plain text');
      const email = uniqueObject(view, 'text_field', 'Email');
      const telephone = uniqueObject(view, 'text_field', 'Telephone');
      const richMatches = (view.objects || []).filter((object) => object.role === 'content_editable');
      assert(richMatches.length === 1, `expected one content_editable, found ${richMatches.length}`);
      const rich = richMatches[0];
      const started = performance.now();
      const receipt = await call('act', {
        tab_id: String(opened.tab_id),
        document_id: view.document_id,
        basis_revision: view.revision,
        steps: [
          { object_id: plain.object_id, operation: 'type', text: 'release-smoke-one' },
          { object_id: email.object_id, operation: 'type', text: 'release-smoke@example.test' },
          { object_id: telephone.object_id, operation: 'type', text: '3125550100' },
        ],
        timeout_ms: 10_000,
      }, 10_000);
      assert(receipt.outcome === 'accepted', `form batch outcome was ${receipt.outcome}`);
      assert(receipt.occurrence === 'observed', `form batch occurrence was ${receipt.occurrence}`);
      assert(receipt.semantic_postcondition?.verified === true, 'form batch was not verified');
      const richView = await read(opened.tab_id, { roles: ['content_editable'], max_objects: 4 });
      const richCurrent = richView.objects[0];
      const richReceipt = await call('act', {
        tab_id: String(opened.tab_id),
        document_id: richView.document_id,
        basis_revision: richView.revision,
        object_id: richCurrent.object_id,
        operation: 'type',
        text: 'release-smoke-rich',
        timeout_ms: 10_000,
      }, 10_000);
      assert(richReceipt.outcome === 'accepted', 'content_editable was not accepted');
      assert(richReceipt.semantic_postcondition?.verified === true, 'content_editable was not verified');
      report.suites.form_batch = {
        passed: true,
        steps: 3,
        elapsed_ms: Math.round((performance.now() - started) * 1000) / 1000,
        final_revision: receipt.final_revision,
        relevant_change_count: receipt.relevant_delta?.changes?.length
          || receipt.relevant_delta?.changed_steps?.length || 0,
        content_editable_has_semantic_name: objectLabel(rich).length > 0,
        content_editable_verified_separately: true,
      };
      await close(opened.tab_id);
    }

    for (const layout of ['buttons', 'canvas']) {
      currentStage = `mouse-accuracy-${layout}`;
      const role = layout === 'canvas' ? 'reflex_target' : 'button';
      const opened = await open(`/fixtures/conformance/mouse_accuracy.html?layout=${layout}&difficulty=hard`);
      let view = await waitForQuery(opened.tab_id, {
        roles: [role],
        max_objects: 32,
      }, 24);
      const targets = (view.objects || []).filter(
        (object) => object.role === role && /^Accuracy \d{2}$/.test(objectLabel(object)),
      );
      assert(targets.length === 24, `${layout} exposed ${targets.length}/24 accuracy targets`);
      const times = [];
      let basis = view.revision;
      let failure = null;
      for (const target of targets) {
        const started = performance.now();
        const receipt = await call('act', {
          tab_id: String(opened.tab_id),
          document_id: view.document_id,
          basis_revision: basis,
          object_id: target.object_id,
          operation: 'click',
          timeout_ms: 5_000,
        }, 5_000);
        const elapsed = performance.now() - started;
        if (receipt.outcome !== 'accepted'
          || receipt.occurrence !== 'observed'
          || receipt.semantic_postcondition?.verified !== true) {
          failure = {
            target: objectLabel(target),
            outcome: receipt.outcome,
            occurrence: receipt.occurrence,
            semantic_postcondition: receipt.semantic_postcondition,
            retry_safe: receipt.retry_safe,
            external_execution_required: receipt.external_execution_required,
            elapsed_ms: Math.round(elapsed * 1000) / 1000,
          };
          break;
        }
        times.push(elapsed);
        basis = receipt.next_basis_revision;
      }
      const sorted = [...times].sort((a, b) => a - b);
      report.suites[`mouse_accuracy_${layout}`] = {
        passed: failure === null,
        hits: times.length,
        misses: targets.length - times.length,
        mean_ms: times.length
          ? Math.round((times.reduce((sum, value) => sum + value, 0) / times.length) * 1000) / 1000
          : null,
        p95_ms: times.length
          ? Math.round(sorted[Math.ceil(sorted.length * 0.95) - 1] * 1000) / 1000
          : null,
        max_ms: times.length ? Math.round(sorted.at(-1) * 1000) / 1000 : null,
        failure,
      };
      await close(opened.tab_id);
    }

    report.passed = Object.values(report.suites).every((suite) => suite.passed === true);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    if (!report.passed) process.exitCode = 1;
  } finally {
    for (const tabId of tabs) {
      await call('tabs.close', { tab_id: tabId }).catch(() => null);
    }
    await closeSession(session);
  }
}

main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ passed: false, stage: currentStage, error: error.message })}\n`);
  process.exitCode = 1;
});
