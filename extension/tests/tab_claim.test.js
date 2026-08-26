'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const test = require('node:test');

const worker = fs.readFileSync(path.join(__dirname, '../src/service_worker.js'), 'utf8');

test('agent-created tabs use a single-use origin-bound claim', () => {
  const claim = worker.slice(worker.indexOf('function armTabClaim'), worker.indexOf('async function authorizeTab'));
  assert.match(claim, /normalizeOrigin\(navigableUrl\(url\)\)/);
  assert.match(claim, /expiresAt: Date\.now\(\) \+ CLAIM_TTL_MS/);
  assert.match(claim, /pendingClaim = undefined/);
  assert.match(claim, /claim\.latchedTabId/);
  assert.match(claim, /requestedTabId !== claim\.latchedTabId/);
  assert.match(claim, /normalizeOrigin\(tab\.url\) !== claim\.origin/);
  assert.match(claim, /claimedAgentTabs\.add\(requestedTabId\)/);
});

test('ordinary tabs and opener popups never inherit authorization', () => {
  assert.match(worker, /chrome\.tabs\.onCreated\.addListener\(\(tab\) => \{\s*noteClaimCandidate\(tab\)/s);
  assert.doesNotMatch(worker, /openerTabId.*agentOwnedTabs\.add/s);
  assert.match(worker, /if \(isAuthorized\(tab\.id\)\) return/);
  assert.match(worker, /first qualifying tab wins/);
});

test('Broker loss keeps tab ACL but forces a fresh connection and full Truth', () => {
  const connect = worker.slice(worker.indexOf('function startCommandLoop'), worker.indexOf('function numericTabId'));
  assert.match(connect, /connected\.require_full_truth/);
  assert.match(connect, /requestCollectorSnapshot\(tabId\)/);
  assert.match(connect, /brokerConnectionId = undefined/);
  assert.doesNotMatch(connect, /revokeTabAccess|revokeClaimedTabs|tabs\.remove/);
});

test('command responses belong to one Broker connection and are not replayed', () => {
  const flush = worker.slice(worker.indexOf('async function flushEvents'), worker.indexOf('async function connectBroker'));
  assert.match(flush, /connectionId === brokerConnectionId/);
  assert.match(flush, /event\.kind !== 'response'/);
  assert.match(flush, /command_id: command\.command_id/);
  assert.match(flush, /retry_safe: error\.saccadeRetrySafe === true/);
});

test('tab close is scoped to Agent-owned tabs and does not scan unrelated tabs', () => {
  const close = worker.slice(worker.indexOf("command.kind === 'tabs.close'"), worker.indexOf("command.kind === 'prepare_action'"));
  assert.match(close, /if \(!agentOwnedTabs\.has\(tabId\)\) throw new Error/);
  assert.match(close, /chrome\.tabs\.remove\(tabId\)/);
  assert.doesNotMatch(close, /chrome\.tabs\.query\(\{\}\)/);
});
