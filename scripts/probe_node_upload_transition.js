#!/usr/bin/env node
'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');
const { closeSession, createSession, rpc } = require('../packages/setup/src/broker_client');

const url = process.argv[2];
const filePath = path.resolve(process.argv[3]);
const requestedBrowser = process.argv[4] || 'edge';
const expectedCandidate = JSON.parse(fs.readFileSync(path.resolve('extension/candidate.json'), 'utf8'));
let requestId = 1;

function compact(object) {
  return {
    object_id: object.object_id,
    role: object.role,
    name: object.name,
    text: object.text,
    description: object.description,
    affordances: object.affordances,
    visibility: object.visibility,
    navigation_target: object.navigation_target,
  };
}

async function main() {
  if (!url || !process.argv[3]) throw new Error('usage: probe_node_upload_transition.js URL FILE [BROWSER]');
  const content = fs.readFileSync(filePath);
  const sha256 = crypto.createHash('sha256').update(content).digest('hex');
  const session = await createSession();
  const call = (method, params = {}, timeoutMs = 10_000) => (
    rpc(session, method, params, timeoutMs, requestId++)
  );
  let tabId;
  try {
    const capabilities = await call('system.capabilities');
    const selected = (capabilities.connected_extensions || []).find((candidate) => (
      candidate.browser_family === requestedBrowser
        && candidate.extension_candidate?.id === expectedCandidate.id
    ));
    if (!selected) throw new Error(`current ${requestedBrowser} Extension candidate is not connected`);
    const opened = await call('tabs.open', {
      url, active: true, browser_instance_id: selected.browser_instance_id,
    }, 25_000);
    tabId = String(opened.tab_id);

    await call('truth.read', {
      tab_id: tabId, mode: 'full', query: { affordances: ['upload'], max_objects: 8 },
      min_objects: 1, timeout_ms: 15_000,
    }, 15_000);
    const resolveUniqueUpload = async () => {
      const deadline = Date.now() + 20_000;
      let view;
      while (Date.now() < deadline) {
        view = await call('truth.read', {
          tab_id: tabId, mode: 'full', query: { affordances: ['upload'], max_objects: 8 },
        });
        if ((view.objects || []).length === 1) return view;
        const remaining = Math.max(1, deadline - Date.now());
        await call('truth.read', {
          tab_id: tabId, mode: 'delta', after_revision: view.revision,
          timeout_ms: Math.min(10_000, remaining),
        }, Math.min(10_000, remaining));
      }
      throw new Error(`upload candidates did not settle to one; last count=${(view?.objects || []).length}`);
    };

    let targetView = await resolveUniqueUpload();
    const baseline = await call('truth.read', { tab_id: tabId, mode: 'full' });
    const baselineDeleteCount = (baseline.objects || []).filter((object) => (
      object.role === 'link' && String(object.name || '').toLowerCase() === 'delete'
    )).length;
    let receipt;
    let prepareAttempts = 0;
    while (!receipt && prepareAttempts < 3) {
      prepareAttempts += 1;
      try {
        targetView = await resolveUniqueUpload();
      } catch (error) {
        const current = await call('truth.read', { tab_id: tabId, mode: 'full' });
        const uploadText = await call('truth.read', {
          tab_id: tabId, mode: 'full', query: { text: 'upload', max_objects: 32 },
        });
        const candidates = (current.objects || []).filter((object) => (
          object.role === 'file_input'
            || /upload/i.test(`${object.name || ''} ${object.text || ''} ${object.description || ''}`)
            || ((object.affordances || []).length > 0
              && Number(String(object.object_id).split('.').at(-1)) >= 175)
        )).concat(uploadText.objects || []).map(compact);
        throw new Error(`${error.message}; current upload candidates=${JSON.stringify(candidates)}`);
      }
      const target = targetView.objects[0];
      try {
        receipt = await call('act', {
          tab_id: tabId,
          document_id: targetView.document_id,
          basis_revision: targetView.revision,
          object_id: target.object_id,
          operation: 'upload',
          file_path: filePath,
          file_sha256: sha256,
          timeout_ms: 15_000,
        }, 15_000);
      } catch (error) {
        // A prepare-stage stale failure proves the FileList was never assigned.
        // Resolve the one current semantic upload object again; never replay an
        // accepted, dispatched, or outcome-unknown action.
        if (!/saccade_action_error\|prepare\|stale_action_(basis|token)\|/.test(error.message)) throw error;
      }
    }
    if (!receipt) throw new Error(`upload target remained stale across ${prepareAttempts} fresh resolutions`);
    if (receipt.outcome !== 'accepted'
      || !['file_selection_observed', 'file_drop_dispatched'].includes(receipt.semantic_postcondition?.code)
      || receipt.semantic_postcondition?.verified !== true) {
      throw new Error(`upload selection was not verified: ${receipt.outcome}`);
    }

    let revision = receipt.final_revision;
    let transitionError;
    let persistedDeleteCount = baselineDeleteCount;
    let persistedView;
    try {
      persistedView = await call('truth.read', {
        tab_id: tabId, mode: 'full', query: { text: 'delete', max_objects: 32 },
        min_objects: (baselineDeleteCount + 1) * 3, timeout_ms: 45_000,
      }, 45_000);
    } catch (error) {
      transitionError = error.message;
      persistedView = await call('truth.read', {
        tab_id: tabId, mode: 'full', query: { text: 'delete', max_objects: 32 },
      });
    }
    revision = persistedView.revision;
    persistedDeleteCount = (persistedView.objects || []).filter((object) => (
      object.role === 'link' && String(object.name || '').toLowerCase() === 'delete'
    )).length;

    const pageMessages = {};
    for (const term of ['error', 'invalid', 'failed', 'problem', 'processing', 'uploaded', 'success']) {
      const view = await call('truth.read', {
        tab_id: tabId, mode: 'full', query: { text: term, max_objects: 16 },
      });
      pageMessages[term] = (view.objects || []).map(compact);
    }

    process.stdout.write(`${JSON.stringify({
      ok: true,
      browser_family: selected.browser_family,
      extension_candidate: selected.extension_candidate,
      upload_receipt: {
        outcome: receipt.outcome,
        occurrence: receipt.occurrence,
        semantic_postcondition: receipt.semantic_postcondition,
        upload: receipt.upload,
        external_execution_required: receipt.external_execution_required,
        final_revision: receipt.final_revision,
      },
      prepare_attempts: prepareAttempts,
      observed_revision: revision,
      baseline_delete_count: baselineDeleteCount,
      persisted_delete_count: persistedDeleteCount,
      delete_objects: (persistedView.objects || []).filter((object) => (
        object.role === 'link' && String(object.name || '').toLowerCase() === 'delete'
      )).map(compact),
      page_messages: pageMessages,
      transition_error: transitionError,
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
