'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const registry = require('../src/controls/registry.js');
const select = require('../src/controls/select.js');

test('registry exposes only safe state for cataloged controls', () => {
  assert.deepEqual(registry.observe('button', { enabled: true, pressed: false }), {
    kind: 'control', role: 'button', affordances: ['click'], state: { enabled: 'true', pressed: 'false' }, protected: false,
  });
  assert.deepEqual(registry.observe('search_field', { hasValue: true }).state, {
    enabled: 'true', has_value: 'true', required: 'false', readonly: 'false', invalid: 'false',
  });
  assert.deepEqual(registry.observe('text_area', { hasValue: true }).state, {
    enabled: 'true', has_value: 'true', required: 'false', readonly: 'false', invalid: 'false',
  });
  assert.deepEqual(registry.observe('spin_button', { hasValue: true }).state, {
    enabled: 'true', has_value: 'true', required: 'false', readonly: 'false', invalid: 'false',
  });
  assert.equal(registry.observe('checkbox', { checked: true }).state.checked, 'true');
  assert.equal(registry.observe('radio', { checked: true }).state.checked, 'true');
  assert.equal(registry.observe('switch', { checked: true }).state.checked, 'true');
  assert.equal(registry.observe('tab', { selected: true }).state.selected, 'true');
  assert.equal(registry.observe('menu_item', { expanded: true }).state.expanded, 'true');
  assert.equal(registry.observe('select', { hasValue: true }).state.has_value, 'true');
  assert.equal(registry.observe('link', { current: 'page' }).kind, 'link');
  assert.deepEqual(registry.observe('file_input', { hasValue: false }).affordances, ['upload']);
  assert.deepEqual(registry.observe('reflex_target', { enabled: true }), {
    kind: 'control', role: 'reflex_target', affordances: ['click'], state: { enabled: 'true', reflex_occurrence: '0' }, protected: false,
  });
});

test('unavailable and protected controls do not advertise Host actions', () => {
  assert.deepEqual(registry.observe('button', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('text_field', { readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('select', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('search_field', { readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('text_area', { readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('spin_button', { readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('content_editable', { readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('radio', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('switch', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('tab', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('menu_item', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('content_editable', { required: true, invalid: true, hasValue: true }).state, {
    has_value: 'true', readonly: 'false',
  });
});

test('text contents and submitted option values cannot enter projections', () => {
  const field = registry.observe('text_field', { hasValue: true, value: 'SENTINEL', protected: false });
  const protectedField = registry.observe('text_field', { hasValue: true, value: 'SENTINEL', protected: true });
  const content = registry.observe('content_editable', { hasValue: true, value: 'SENTINEL' });
  const option = select.option('Visible label', true);
  const wire = JSON.stringify({ field, protectedField, content, option });
  assert.equal(wire.includes('SENTINEL'), false);
  assert.deepEqual(protectedField.affordances, []);
  assert.equal(option.name, 'Visible label');
  assert.equal(Object.hasOwn(option, 'value'), false);
  assert.equal(content.role, 'content_editable');
});

test('control files can populate the browser global without CommonJS', () => {
  const fs = require('node:fs');
  const path = require('node:path');
  const vm = require('node:vm');
  const context = vm.createContext({});
  for (const file of ['common.js', 'button.js', 'link.js', 'text_field.js', 'search_field.js', 'text_area.js', 'content_editable.js', 'spin_button.js',
    'checkbox.js', 'radio.js', 'switch.js', 'select.js', 'tab.js', 'menu_item.js',
    'reflex_target.js', 'file_input.js', 'registry.js']) {
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/controls', file), 'utf8'), context);
  }
  assert.equal(context.SaccadeControls.registry.observe('button', {}).role, 'button');
  assert.equal(context.SaccadeControls.registry.observe('reflex_target', {}).role, 'reflex_target');
  assert.equal(context.SaccadeControls.registry.observe('file_input', {}).role, 'file_input');
  assert.equal(context.SaccadeControls.registry.observe('menu_item', {}).role, 'menu_item');
});
