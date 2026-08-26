'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const { methodForTool, tools } = require('../src/mcp');

test('Node MCP exposes exactly six tab-scoped tools', () => {
  const listed = tools();
  assert.deepEqual(listed.map((tool) => tool.name), [
    'saccade.system.capabilities', 'saccade.tabs.list', 'saccade.tabs.open',
    'saccade.tabs.close', 'saccade.truth.read', 'saccade.act',
  ]);
  const truth = listed.find((tool) => tool.name === 'saccade.truth.read');
  assert.deepEqual(truth.inputSchema.required, ['tab_id', 'mode']);
  assert.deepEqual(truth.inputSchema.properties.mode.enum, ['full', 'delta']);
  const act = listed.find((tool) => tool.name === 'saccade.act');
  assert.equal(act.inputSchema.properties.steps.maxItems, 32);
  assert.deepEqual(act.inputSchema.anyOf, [{ required: ['object_id'] }, { required: ['steps'] }]);
});

test('public tool mapping has no native or reference actuator route', () => {
  assert.equal(methodForTool('saccade.act'), 'act');
  assert.equal(methodForTool('saccade.reference.act_native'), undefined);
  assert.equal(methodForTool('saccade.native'), undefined);
});
