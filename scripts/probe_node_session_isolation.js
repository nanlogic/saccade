#!/usr/bin/env node
'use strict';

const { closeSession, createSession, rpc } = require('../packages/setup/src/broker_client');
const { optionValue, releaseTargetFromCapabilities } = require('./node_release_target');

const ARGS = process.argv.slice(2);
const BASE_URL = (optionValue(ARGS, 'base-url') || 'http://127.0.0.1:8765').replace(/\/$/, '');
let requestId = 1;

async function main() {
  const agentA = await createSession();
  const agentB = await createSession();
  const call = (session, method, params = {}, timeoutMs = 10_000) => (
    rpc(session, method, params, timeoutMs, requestId++)
  );
  const owned = [];
  try {
    if (agentA.agent_session_id === agentB.agent_session_id) {
      throw new Error('two MCP connections received the same Agent session');
    }
    const targetA = releaseTargetFromCapabilities(await call(agentA, 'system.capabilities'), ARGS);
    const targetB = releaseTargetFromCapabilities(await call(agentB, 'system.capabilities'), ARGS);
    if (targetA.browser_instance_id !== targetB.browser_instance_id) {
      throw new Error('Agent sessions resolved different browser instances');
    }
    const openA = await call(agentA, 'tabs.open', {
      url: `${BASE_URL}/fixtures/controls/software_type.html?release-agent=a`,
      active: true,
      browser_instance_id: targetA.browser_instance_id,
    }, 25_000);
    owned.push([agentA, String(openA.tab_id)]);
    const listB = await call(agentB, 'tabs.list');
    if ((listB.tabs || []).some((tab) => String(tab.tab_id) === String(openA.tab_id))) {
      throw new Error('Agent B listed Agent A tab');
    }
    let crossReadDenied = false;
    try {
      await call(agentB, 'truth.read', { tab_id: String(openA.tab_id), mode: 'full' });
    } catch (error) {
      crossReadDenied = ['TAB_OWNERSHIP', 'TAB_NOT_LEASED'].includes(error.code)
        || /leased to another Agent|not leased/i.test(error.message);
    }
    if (!crossReadDenied) throw new Error('Agent B cross-session Truth read was not denied');

    const openB = await call(agentB, 'tabs.open', {
      url: `${BASE_URL}/fixtures/controls/software_type.html?release-agent=b`,
      active: true,
      browser_instance_id: targetB.browser_instance_id,
    }, 25_000);
    owned.push([agentB, String(openB.tab_id)]);
    const [listA, finalListB] = await Promise.all([
      call(agentA, 'tabs.list'), call(agentB, 'tabs.list'),
    ]);
    const idsA = (listA.tabs || []).map((tab) => String(tab.tab_id));
    const idsB = (finalListB.tabs || []).map((tab) => String(tab.tab_id));
    if (!idsA.includes(String(openA.tab_id)) || idsA.includes(String(openB.tab_id))
      || !idsB.includes(String(openB.tab_id)) || idsB.includes(String(openA.tab_id))) {
      throw new Error('Agent tab inventories were not isolated');
    }
    process.stdout.write(`${JSON.stringify({
      schema: 'saccade.node-session-isolation/1',
      passed: true,
      browser_family: targetA.browser,
      browser_instance_id: targetA.browser_instance_id,
      extension_candidate: targetA.extension_candidate,
      distinct_agent_sessions: true,
      cross_read_denied: true,
      exact_tab_inventories: true,
      one_writer_per_tab: true,
    }, null, 2)}\n`);
  } finally {
    for (const [session, tabId] of owned.reverse()) {
      await call(session, 'tabs.close', { tab_id: tabId }).catch(() => null);
    }
    await Promise.all([closeSession(agentA), closeSession(agentB)]);
  }
}

main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ passed: false, error: error.message })}\n`);
  process.exitCode = 1;
});
