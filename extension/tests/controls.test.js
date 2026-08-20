'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const registry = require('../src/controls/registry.js');
const select = require('../src/controls/select.js');
const reflexTarget = require('../src/controls/reflex_target.js');

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
  assert.deepEqual(registry.observe('select', { expanded: false, expandable: true }).affordances, ['click', 'select']);
  assert.deepEqual(registry.observe('select', { expanded: false, expandable: false }).affordances, ['select']);
  assert.deepEqual(select.option('Pizza', false, true, true).affordances, ['click']);
  assert.deepEqual(select.option('Pizza', false, false, true).affordances, []);
  assert.deepEqual(select.option('Pizza', false, true, false).affordances, []);
  assert.deepEqual(registry.option('Pizza', false, true, true).affordances, ['click']);
  assert.equal(registry.observe('link', { current: 'page' }).kind, 'link');
  assert.deepEqual(registry.observe('file_input', { hasValue: false }).affordances, ['upload']);
  assert.deepEqual(registry.observe('reflex_target', { enabled: true }), {
    kind: 'control', role: 'reflex_target', affordances: ['click'], state: { enabled: 'true', reflex_occurrence: '0' }, protected: false,
  });
  assert.deepEqual(registry.observe('slider', { enabled: true }).affordances, ['drag']);
  assert.deepEqual(registry.observe('label', {}).affordances, []);
  assert.deepEqual(registry.observe('generic_control', { affordance: 'drag' }).affordances, ['drag']);
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

test('reflex occurrence accepts an authored counter or a visible score', () => {
  assert.equal(reflexTarget.occurrence('17', 'SCORE 9'), '17');
  assert.equal(reflexTarget.occurrence('', 'TIME\n28\nSCORE\n16\nESC TO END'), '16');
  assert.equal(reflexTarget.occurrence(null, 'No score here'), '0');
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
    'reflex_target.js', 'file_input.js', 'slider.js', 'label.js', 'generic_control.js', 'registry.js']) {
    vm.runInContext(fs.readFileSync(path.join(__dirname, '../src/controls', file), 'utf8'), context);
  }
  assert.equal(context.SaccadeControls.registry.observe('button', {}).role, 'button');
  assert.equal(context.SaccadeControls.registry.observe('reflex_target', {}).role, 'reflex_target');
  assert.equal(context.SaccadeControls.registry.observe('file_input', {}).role, 'file_input');
  assert.equal(context.SaccadeControls.registry.observe('menu_item', {}).role, 'menu_item');
});

test('a protected field carries no type affordance in any typeable role', () => {
  const { isProtectedFieldType } = require('../src/consent.js');
  // The classifier and the registry are the two halves of the gate: the first
  // decides a value is protected, the second refuses to mint a type affordance
  // for it. Without an affordance the collector never mints an action token,
  // so prepare() rejects the request before any mutation can be attempted.
  const protectedIdentities = [
    { type: 'password', autocomplete: '', hint: 'Password' },
    { type: 'text', autocomplete: 'current-password', hint: 'Current password' },
    { type: 'text', autocomplete: 'new-password', hint: 'Choose a password' },
    { type: 'text', autocomplete: '', hint: 'Social Security Number' },
    { type: 'text', autocomplete: '', hint: 'SSN' },
    { type: 'text', autocomplete: '', hint: 'Employer Identification Number' },
    { type: 'text', autocomplete: '', hint: 'EIN' },
    { type: 'text', autocomplete: '', hint: 'Federal Tax Identification Number' },
  ];
  for (const role of ['text_field', 'search_field', 'text_area', 'spin_button']) {
    for (const identity of protectedIdentities) {
      const isProtected = isProtectedFieldType(identity.type, identity.autocomplete, identity.hint);
      assert.equal(isProtected, true, `${identity.hint} must classify as protected`);
      const observed = registry.observe(role, { enabled: true, hasValue: false, protected: isProtected });
      assert.deepEqual(observed.affordances, [],
        `${role} holding ${identity.hint} must expose no affordance`);
      assert.equal(observed.protected, true);
      // Truth still reports whether a human-only channel has filled the field,
      // and nothing more: has_value is the only evidence, never the value.
      assert.equal(observed.state.has_value, 'false');
      assert.equal(Object.values(observed.state).includes(identity.hint), false);
    }
  }
});

test('an ordinary field of the same roles keeps exactly one type affordance', () => {
  // The negative gate above is only meaningful if the positive case still
  // works, or "no affordance" would be indistinguishable from a broken registry.
  for (const role of ['text_field', 'search_field', 'text_area', 'spin_button']) {
    for (const hint of ['Email', 'Telephone', 'Search', 'Full name', 'Quantity']) {
      assert.equal(require('../src/consent.js').isProtectedFieldType('text', '', hint), false,
        `${hint} must not be classified as protected`);
      assert.deepEqual(
        registry.observe(role, { enabled: true, hasValue: false, protected: false }).affordances,
        ['type'],
      );
    }
  }
  // readonly and disabled remove the affordance for the same structural reason.
  assert.deepEqual(registry.observe('text_field', { enabled: true, readonly: true }).affordances, []);
  assert.deepEqual(registry.observe('text_field', { enabled: false }).affordances, []);
  assert.deepEqual(registry.observe('content_editable', { enabled: true }).affordances, ['type']);
  assert.deepEqual(registry.observe('content_editable', { enabled: true, readonly: true }).affordances, []);
});
