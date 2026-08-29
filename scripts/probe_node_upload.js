#!/usr/bin/env node
'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { closeSession, createSession, rpc } = require('../packages/setup/src/broker_client');

const url = process.argv[2]
  || 'http://127.0.0.1:8765/fixtures/controls/file_input.html';
const filePath = path.resolve(process.argv[3] || 'extension/icons/icon-128.png');
const requestedBrowser = process.argv[4];
const inventoryOnly = process.argv.includes('--inventory');
const broadInventory = process.argv.includes('--broad-inventory');
const pageInventory = process.argv.includes('--page-inventory');
const capabilitiesOnly = process.argv.includes('--capabilities');
const queryArgument = process.argv.find((argument) => argument.startsWith('--query='));
const queryText = queryArgument ? queryArgument.slice('--query='.length) : undefined;
const roleArgument = process.argv.find((argument) => argument.startsWith('--role='));
const queryRole = roleArgument ? roleArgument.slice('--role='.length) : undefined;
const affordanceArgument = process.argv.find((argument) => argument.startsWith('--affordance='));
const queryAffordance = affordanceArgument ? affordanceArgument.slice('--affordance='.length) : undefined;
const minimumArgument = process.argv.find((argument) => argument.startsWith('--min='));
const minimumObjects = minimumArgument ? Number(minimumArgument.slice('--min='.length)) : 1;
const expectedCandidate = JSON.parse(fs.readFileSync(
  path.resolve('extension/candidate.json'), 'utf8'));
let requestId = 1;

async function main() {
  const content = fs.readFileSync(filePath);
  const sha256 = crypto.createHash('sha256').update(content).digest('hex');
  const session = await createSession();
  let tabId;
  const call = (method, params = {}, timeoutMs = 10_000) => (
    rpc(session, method, params, timeoutMs, requestId++)
  );
  try {
    const capabilities = await call('system.capabilities');
    if (capabilitiesOnly) {
      process.stdout.write(`${JSON.stringify({
        ok: true,
        connected_extensions: capabilities.connected_extensions,
      }, null, 2)}\n`);
      return;
    }
    const candidates = capabilities.connected_extensions || [];
    const currentCandidates = candidates.filter((candidate) => (
      candidate.extension_candidate?.id === expectedCandidate.id
    ));
    const selected = requestedBrowser
      ? currentCandidates.find((candidate) => candidate.browser_family === requestedBrowser)
      : currentCandidates.length === 1 ? currentCandidates[0] : undefined;
    if (!selected) {
      throw new Error(requestedBrowser
        ? `no connected ${requestedBrowser} Extension candidate`
        : `expected exactly one current Extension, found ${currentCandidates.length}`);
    }
    const opened = await call('tabs.open', {
      url, active: true, browser_instance_id: selected.browser_instance_id,
    }, 25_000);
    tabId = String(opened.tab_id);
    const truth = await call('truth.read', {
      tab_id: tabId, mode: 'full',
      min_objects: minimumObjects, timeout_ms: 10_000,
      query: pageInventory
        ? { max_objects: 32 }
        : queryAffordance !== undefined
        ? { affordances: [queryAffordance], max_objects: 64 }
        : queryRole !== undefined
        ? { roles: [queryRole], max_objects: 64 }
        : queryText !== undefined
        ? { text: queryText, max_objects: 64 }
        : broadInventory
        ? { text: 'upload', max_objects: 32 }
        : { roles: ['file_input'], text: 'Upload', max_objects: 16 },
    }, 10_000);
    if (inventoryOnly || broadInventory || pageInventory
      || queryText !== undefined || queryRole !== undefined || queryAffordance !== undefined) {
      process.stdout.write(`${JSON.stringify({
        ok: true,
        browser_family: selected.browser_family,
        extension_candidate: selected.extension_candidate,
        document_id: truth.document_id,
        revision: truth.revision,
        document_url: truth.frames?.find((frame) => !frame.parent_frame_id)?.document_url,
        objects: (truth.objects || []).map((object) => ({
          object_id: object.object_id,
          role: object.role,
          name: object.name,
          description: object.description,
          text: object.text,
          affordances: object.affordances,
          state: object.state,
          visibility: object.visibility,
        })),
      }, null, 2)}\n`);
      return;
    }
    const matches = (truth.objects || []).filter((object) => object.name === 'Upload');
    if (matches.length !== 1) {
      throw new Error(`expected one dynamic Upload object, found ${matches.length}`);
    }
    const target = matches[0];
    const receipt = await call('act', {
      tab_id: tabId, document_id: truth.document_id, basis_revision: truth.revision,
      object_id: target.object_id, operation: 'upload',
      file_path: filePath, file_sha256: sha256, timeout_ms: 15_000,
    }, 15_000);
    if (receipt.outcome !== 'accepted'
      || receipt.semantic_postcondition?.code !== 'file_selection_observed'
      || receipt.semantic_postcondition?.verified !== true
      || receipt.upload?.sha256 !== sha256) {
      throw new Error(`upload receipt was not verified: ${receipt.outcome}`);
    }
    process.stdout.write(`${JSON.stringify({
      ok: true,
      browser_family: selected.browser_family,
      extension_candidate: selected.extension_candidate,
      initial_revision: truth.revision,
      final_revision: receipt.final_revision,
      occurrence: receipt.occurrence,
      semantic_postcondition: receipt.semantic_postcondition,
      upload: receipt.upload,
    }, null, 2)}\n`);
  } finally {
    if (tabId) await call('tabs.close', { tab_id: tabId }, 10_000).catch(() => null);
    await closeSession(session).catch(() => null);
  }
}

main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ ok: false, error: error.message })}\n`);
  process.exitCode = 1;
});
