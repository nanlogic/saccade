'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const registry = require('../src/controls/registry.js');
const select = require('../src/controls/select.js');

test('registry exposes only safe state for the four first-slice controls', () => {
  assert.deepEqual(registry.observe('button', { enabled: true, pressed: false }), {
    kind: 'control', role: 'button', affordances: ['click'], state: { enabled: 'true', pressed: 'false' }, protected: false,
  });
  assert.equal(registry.observe('checkbox', { checked: true }).state.checked, 'true');
  assert.equal(registry.observe('select', { hasValue: true }).state.has_value, 'true');
});

test('unavailable and protected controls do not advertise Host actions', () => {
  assert.deepEqual(registry.observe('button', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('text_field', { readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('select', { enabled: false }).affordances, []);
});

test('text contents and submitted option values cannot enter projections', () => {
  const field = registry.observe('text_field', { hasValue: true, value: 'SENTINEL', protected: false });
  const protectedField = registry.observe('text_field', { hasValue: true, value: 'SENTINEL', protected: true });
  const option = select.option('Visible label', true);
  const wire = JSON.stringify({ field, protectedField, option });
  assert.equal(wire.includes('SENTINEL'), false);
  assert.deepEqual(protectedField.affordances, []);
  assert.equal(option.name, 'Visible label');
  assert.equal(Object.hasOwn(option, 'value'), false);
});

test('control files can populate the browser global without CommonJS', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const vm = require('node:vm');
  const context = vm.createContext({});
  for (const file of ['common.js', 'button.js', 'text_field.js', 'checkbox.js', 'select.js', 'registry.js']) {
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/controls', file), 'utf8'), context);
  }
  assert.equal(context.SaccadeControls.registry.observe('button', {}).role, 'button');
});
