#!/usr/bin/env node
'use strict';

const { closeSession, createSession, rpc } = require('../packages/setup/src/broker_client');
const { releaseTargetFromCapabilities } = require('./node_release_target');

const ARGS = process.argv.slice(2);

const CASES = [
  {
    id: 'bestbuy_homepage_navigation',
    url: 'https://www.bestbuy.com/',
    expected: ['Deal of the Day', 'Top Deals'],
  },
  {
    id: 'nanmesh_homepage_identity',
    url: 'https://www.nanmesh.ai/',
    expected: ['The trust and outcome protocol layer for AI agents'],
  },
  {
    id: 'nanlogic_homepage_product',
    url: 'https://nanlogic.com/',
    expected: ['NaNDesk', 'Try NaNDesk free'],
  },
  {
    id: 'mythcastera_homepage_navigation',
    url: 'https://mythcastera.com/',
    expected: ['What is Mythcast Era?', 'Join waitlist'],
  },
];

let requestId = 1;

function label(object) {
  return String(object.name || object.text || '');
}

async function main() {
  const session = await createSession();
  const tabs = new Set();
  const call = (method, params = {}, timeoutMs = 10_000) => (
    rpc(session, method, params, timeoutMs, requestId++)
  );
  const report = {
    schema: 'saccade.node-public-truth/1',
    cases: [],
  };
  let releaseTarget;

  try {
    const capabilities = await call('system.capabilities');
    if (!capabilities.extension_connected) throw new Error('Extension disconnected');
    releaseTarget = releaseTargetFromCapabilities(capabilities, ARGS);
    report.browser_family = releaseTarget.browser;
    report.browser_instance_id = releaseTarget.browser_instance_id;
    report.extension_candidate = releaseTarget.extension_candidate;
    for (const testCase of CASES) {
      const started = performance.now();
      const opened = await call('tabs.open', {
        url: testCase.url,
        active: true,
        browser_instance_id: releaseTarget.browser_instance_id,
      }, 25_000);
      const tabId = String(opened.tab_id);
      tabs.add(tabId);
      const phrases = [];
      let lastRevision = opened.initial_revision;
      for (const expected of testCase.expected) {
        const view = await call('truth.read', {
          tab_id: tabId,
          mode: 'full',
          query: { text: expected, max_objects: 8 },
          min_objects: 1,
          timeout_ms: 15_000,
        }, 15_000);
        const matches = (view.objects || [])
          .map(label)
          .filter((value) => value.toLowerCase().includes(expected.toLowerCase()));
        phrases.push({ expected, found: matches.length > 0, match_count: matches.length });
        lastRevision = Math.max(lastRevision, view.revision);
      }
      const passed = phrases.every((phrase) => phrase.found);
      report.cases.push({
        id: testCase.id,
        passed,
        readiness: opened.readiness,
        initial_revision: opened.initial_revision,
        final_revision: lastRevision,
        elapsed_ms: Math.round((performance.now() - started) * 1000) / 1000,
        phrases,
      });
      await call('tabs.close', { tab_id: tabId });
      tabs.delete(tabId);
    }
    report.passed = report.cases.every((testCase) => testCase.passed);
    process.stdout.write(`${JSON.stringify(report, null, 2)}\n`);
    if (!report.passed) process.exitCode = 1;
  } finally {
    for (const tabId of tabs) await call('tabs.close', { tab_id: tabId }).catch(() => null);
    await closeSession(session);
  }
}

main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ passed: false, error: error.message })}\n`);
  process.exitCode = 1;
});
