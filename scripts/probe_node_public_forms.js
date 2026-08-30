#!/usr/bin/env node
'use strict';

const { closeSession, createSession, rpc } = require('../packages/setup/src/broker_client');
const { releaseTargetFromCapabilities } = require('./node_release_target');

let requestId = 1;
const ARGS = process.argv.slice(2);
const ALLOW_SUBMIT = ARGS.includes('--submit');

function label(object) {
  return String(object.name || object.text || '');
}

function semanticKey(value) {
  return String(value || '').replace(/\s+/g, ' ').trim().toLocaleLowerCase('en-US');
}

function findObject(view, role, name) {
  const matches = (view.objects || []).filter(
    (object) => object.role === role && semanticKey(label(object)) === semanticKey(name),
  );
  if (matches.length !== 1) {
    throw new Error(`expected one ${role}:${name}, found ${matches.length}; names=${JSON.stringify(
      (view.objects || []).filter((object) => object.role === role).map(label),
    )}`);
  }
  return matches[0];
}

function findDescription(view, role, description) {
  const matches = (view.objects || []).filter(
    (object) => object.role === role
      && semanticKey(object.description) === semanticKey(description),
  );
  if (matches.length !== 1) {
    throw new Error(`expected one ${role} description:${description}, found ${matches.length}`);
  }
  return matches[0];
}

function findContextActionObject(view, role, name, operation, descriptionPrefix) {
  const semanticMatches = (view.objects || []).filter((object) => (
    object.role === role && semanticKey(label(object)) === semanticKey(name)
      && (object.affordances || []).includes(operation)
  ));
  const contextualMatches = semanticMatches.filter((object) => (
    String(object.description || '').startsWith(descriptionPrefix)
  ));
  if (contextualMatches.length === 1) return contextualMatches[0];
  if (contextualMatches.length === 0 && semanticMatches.length === 1) {
    return semanticMatches[0];
  }
  const candidates = (view.objects || []).slice(0, 4).map((object) => ({
    role: object.role,
    name: label(object),
    description: String(object.description || '').slice(0, 120),
    affordances: object.affordances || [],
  }));
  throw new Error(
    `expected one strict ${role}:${name} with ${operation}; contextual=${contextualMatches.length}, semantic=${semanticMatches.length}; candidates=${JSON.stringify(candidates)}`,
  );
}

function prepareStale(error) {
  const message = String(error?.message || error);
  return (error?.retry_safe === true || /\|true\|/.test(message))
    && /prepare|OBJECT_UNKNOWN/i.test(message)
    && /stale_action_(?:basis|token)|OBJECT_UNKNOWN/i.test(message);
}

async function main() {
  const session = await createSession();
  const tabs = new Set();
  const call = (method, params = {}, timeoutMs = 10_000) => (
    rpc(session, method, params, timeoutMs, requestId++)
  );
  let releaseTarget;
  const open = async (url) => {
    const opened = await call('tabs.open', {
      url, active: true, browser_instance_id: releaseTarget.browser_instance_id,
    }, 25_000);
    tabs.add(String(opened.tab_id));
    return opened;
  };
  const read = (tabId, query) => call('truth.read', {
    tab_id: String(tabId), mode: 'full', query,
  }, 10_000);
  const waitRead = async (tabId, query, minimum = 1) => {
    return call('truth.read', {
      tab_id: String(tabId), mode: 'full', query,
      min_objects: minimum, timeout_ms: 15_000,
    }, 15_000);
  };
  const withFreshPrepare = async (operation) => {
    for (let attempt = 1; attempt <= 3; attempt += 1) {
      try { return { receipt: await operation(), attempts: attempt }; }
      catch (error) {
        if (!prepareStale(error) || attempt === 3) throw error;
      }
    }
    throw new Error('unreachable fresh-prepare loop');
  };
  const close = async (tabId) => {
    await call('tabs.close', { tab_id: String(tabId) });
    tabs.delete(String(tabId));
  };
  const report = { schema: 'saccade.node-public-forms/1', cases: [] };

  try {
    releaseTarget = releaseTargetFromCapabilities(await call('system.capabilities'), ARGS);
    report.browser_family = releaseTarget.browser;
    report.browser_instance_id = releaseTarget.browser_instance_id;
    report.extension_candidate = releaseTarget.extension_candidate;
    {
      const result = { id: 'selenium-official-web-form', passed: false };
      let opened;
      const started = performance.now();
      try {
        opened = await open('https://www.selenium.dev/selenium/web/web-form.html');
        const view = await waitRead(opened.tab_id, {
          roles: ['text_field', 'text_area', 'select', 'option', 'checkbox', 'radio'],
          max_objects: 32,
        });
        const text = findObject(view, 'text_field', 'Text input');
        const area = findObject(view, 'text_area', 'Textarea');
        const select = findObject(view, 'select', 'Dropdown (select)');
        const option = findObject(view, 'option', 'Two');
        const checkbox = findObject(view, 'checkbox', 'Default checkbox');
        const radio = findObject(view, 'radio', 'Default radio');
        const steps = [
          { object_id: text.object_id, operation: 'type', text: 'release-selenium-text' },
          { object_id: area.object_id, operation: 'type', text: 'release selenium line one\nline two' },
          { object_id: select.object_id, operation: 'select', option_object_id: option.object_id },
          ...(checkbox.state?.checked === 'true' ? [] : [{ object_id: checkbox.object_id, operation: 'click' }]),
          ...(radio.state?.checked === 'true' ? [] : [{ object_id: radio.object_id, operation: 'click' }]),
        ];
        result.preflight_objects = [text, area, select, option, checkbox, radio].map((object) => ({
          role: object.role,
          name: label(object),
          affordances: object.affordances || [],
          has_action_authority: typeof object.action_token === 'string',
          enabled: object.state?.enabled,
          readonly: object.state?.readonly,
          protected: object.protected,
          checked: object.state?.checked,
          selected: object.state?.selected,
        }));
        const batchStarted = performance.now();
        const batch = await call('act', {
          tab_id: String(opened.tab_id),
          document_id: view.document_id,
          basis_revision: view.revision,
          steps,
          timeout_ms: 10_000,
        }, 10_000);
        if (batch.outcome !== 'accepted' || batch.semantic_postcondition?.verified !== true) {
          throw new Error(`Selenium batch was not verified: ${batch.outcome}`);
        }
        let submitReceipt = null;
        if (ALLOW_SUBMIT) {
          const submitView = await read(opened.tab_id, {
            text: 'Submit', roles: ['button'], max_objects: 4,
          });
          const submit = findObject(submitView, 'button', 'Submit');
          submitReceipt = await call('act', {
            tab_id: String(opened.tab_id),
            document_id: submitView.document_id,
            basis_revision: submitView.revision,
            object_id: submit.object_id,
            operation: 'click',
            timeout_ms: 10_000,
          }, 10_000);
          if (submitReceipt.outcome !== 'accepted' || submitReceipt.occurrence !== 'observed') {
            throw new Error(`Selenium submit was not observed: ${submitReceipt.outcome}`);
          }
        }
        result.passed = true;
        result.complete = ALLOW_SUBMIT;
        result.batch_steps = steps.length;
        result.batch_ms = Math.round((performance.now() - batchStarted) * 1000) / 1000;
        result.submit_status = ALLOW_SUBMIT ? 'executed' : 'not_run_requires_explicit_approval';
        result.submit_verified = submitReceipt?.semantic_postcondition?.verified === true;
        result.final_revision = submitReceipt?.final_revision || batch.final_revision;
      } catch (error) {
        result.error = error.message;
      } finally {
        if (opened) await close(opened.tab_id).catch(() => null);
      }
      result.elapsed_ms = Math.round((performance.now() - started) * 1000) / 1000;
      report.cases.push(result);
    }

    {
      const result = { id: 'demoqa-react-practice-form', passed: false };
      let opened;
      const started = performance.now();
      try {
        opened = await open('https://demoqa.com/automation-practice-form');
        const view = await waitRead(opened.tab_id, {
          roles: ['text_field', 'text_area', 'radio', 'checkbox'],
          max_objects: 32,
        });
        result.projected_objects = (view.objects || []).map((object) => ({
          role: object.role,
          name: label(object),
          description: object.description || '',
          affordances: object.affordances || [],
          has_action_authority: typeof object.action_token === 'string',
        }));
        const first = findDescription(view, 'text_field', 'Placeholder: First Name');
        const last = findDescription(view, 'text_field', 'Placeholder: Last Name');
        const email = findDescription(view, 'text_field', 'Placeholder: name@example.com');
        const mobile = findDescription(view, 'text_field', 'Placeholder: Mobile Number');
        const address = findDescription(view, 'text_area', 'Placeholder: Current Address');
        const male = findObject(view, 'radio', 'Male');
        const sports = findObject(view, 'checkbox', 'Sports');
        const steps = [
          { object_id: first.object_id, operation: 'type', text: 'Saccade' },
          { object_id: last.object_id, operation: 'type', text: 'Protocol' },
          { object_id: email.object_id, operation: 'type', text: 'saccade.qa@example.test' },
          { object_id: mobile.object_id, operation: 'type', text: '3125550198' },
          { object_id: address.object_id, operation: 'type', text: 'Truth Layer React release test' },
          ...(male.state?.checked === 'true' ? [] : [{ object_id: male.object_id, operation: 'click' }]),
          ...(sports.state?.checked === 'true' ? [] : [{ object_id: sports.object_id, operation: 'click' }]),
        ];
        const batchStarted = performance.now();
        const batch = await call('act', {
          tab_id: String(opened.tab_id),
          document_id: view.document_id,
          basis_revision: view.revision,
          steps,
          timeout_ms: 15_000,
        }, 15_000);
        if (batch.outcome !== 'accepted' || batch.semantic_postcondition?.verified !== true) {
          throw new Error(`React batch was not verified: ${batch.outcome}`);
        }
        let submitReceipt = null;
        if (ALLOW_SUBMIT) {
          const submitView = await read(opened.tab_id, {
            text: 'Submit', roles: ['button'], max_objects: 4,
          });
          const submit = findObject(submitView, 'button', 'Submit');
          submitReceipt = await call('act', {
            tab_id: String(opened.tab_id),
            document_id: submitView.document_id,
            basis_revision: submitView.revision,
            object_id: submit.object_id,
            operation: 'click',
            timeout_ms: 10_000,
          }, 10_000);
          if (submitReceipt.outcome !== 'accepted' || submitReceipt.occurrence !== 'observed') {
            throw new Error(`React submit was not observed: ${submitReceipt.outcome}`);
          }
        }
        result.passed = true;
        result.complete = ALLOW_SUBMIT;
        result.batch_steps = steps.length;
        result.batch_ms = Math.round((performance.now() - batchStarted) * 1000) / 1000;
        result.submit_status = ALLOW_SUBMIT ? 'executed' : 'not_run_requires_explicit_approval';
        result.submit_verified = submitReceipt?.semantic_postcondition?.verified === true;
        result.final_revision = submitReceipt?.final_revision || batch.final_revision;
      } catch (error) {
        result.error = error.message;
      } finally {
        if (opened) await close(opened.tab_id).catch(() => null);
      }
      result.elapsed_ms = Math.round((performance.now() - started) * 1000) / 1000;
      report.cases.push(result);
    }

    {
      const result = { id: 'angular-material-public-select', passed: false };
      let opened;
      let angularSelectObjectId;
      const started = performance.now();
      try {
        // A unique, inert query avoids browser scroll restoration causing the
        // documentation site's viewport-lazy examples to start at a prior
        // position. It does not select or address a page object.
        opened = await open(
          `https://material.angular.dev/components/select/examples?saccade_release_probe=${Date.now()}`,
        );
        const openedSelect = await withFreshPrepare(async () => {
          const selectView = await waitRead(opened.tab_id, {
            roles: ['select'], max_objects: 32,
          }, 10);
          // Wait for the hydrated semantic working set, then resolve a single
          // actionable semantic object. This avoids both transient first paint
          // and volatile documentation-shell description text.
          const select = findContextActionObject(
            selectView, 'select', 'Favorite food', 'click', 'Basic mat-select',
          );
          const receipt = await call('act', {
            tab_id: String(opened.tab_id),
            document_id: selectView.document_id,
            basis_revision: selectView.revision,
            object_id: select.object_id,
            operation: 'click',
            timeout_ms: 10_000,
          }, 10_000);
          angularSelectObjectId = select.object_id;
          return receipt;
        });
        const openReceipt = openedSelect.receipt;
        if (openReceipt.outcome !== 'accepted' || openReceipt.semantic_postcondition?.verified !== true) {
          throw new Error(`Angular select open was not verified: ${openReceipt.outcome}`);
        }
        const selectedPizza = await withFreshPrepare(async () => {
          const optionView = await waitRead(opened.tab_id, {
            roles: ['option'], text: 'Pizza', max_objects: 8,
          });
          const pizza = findObject(optionView, 'option', 'Pizza');
          const current = await read(opened.tab_id, {
            object_ids: [angularSelectObjectId, pizza.object_id], max_objects: 4,
          });
          const currentSelect = findObject(current, 'select', 'Favorite food');
          const currentPizza = findObject(current, 'option', 'Pizza');
          return call('act', {
            tab_id: String(opened.tab_id),
            document_id: current.document_id,
            basis_revision: current.revision,
            object_id: currentSelect.object_id,
            operation: 'select',
            option_object_id: currentPizza.object_id,
            timeout_ms: 10_000,
          }, 10_000);
        });
        const selectReceipt = selectedPizza.receipt;
        if (selectReceipt.outcome !== 'accepted'
          || selectReceipt.semantic_postcondition?.verified !== true) {
          throw new Error(`Angular Pizza selection was not verified: ${selectReceipt.outcome}`);
        }
        result.passed = true;
        result.complete = true;
        result.open_verified = true;
        result.selection_verified = true;
        result.open_prepare_attempts = openedSelect.attempts;
        result.selection_prepare_attempts = selectedPizza.attempts;
        result.final_revision = selectReceipt.final_revision;
      } catch (error) {
        result.error = error.message;
      } finally {
        if (opened) await close(opened.tab_id).catch(() => null);
      }
      result.elapsed_ms = Math.round((performance.now() - started) * 1000) / 1000;
      report.cases.push(result);
    }

    report.passed = report.cases.every((testCase) => testCase.passed);
    report.complete = report.cases.every((testCase) => testCase.complete);
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
