'use strict';

const assert = require('node:assert/strict');
const test = require('node:test');

const { agentResult, agentText, methodForTool, tools } = require('../src/mcp');

test('Node MCP exposes exactly six tab-scoped tools', () => {
  const listed = tools();
  assert.deepEqual(listed.map((tool) => tool.name), [
    'saccade.system.capabilities', 'saccade.tabs.list', 'saccade.tabs.open',
    'saccade.tabs.close', 'saccade.truth.read', 'saccade.act',
  ]);
  const truth = listed.find((tool) => tool.name === 'saccade.truth.read');
  assert.deepEqual(truth.inputSchema.required, ['tab_id', 'mode']);
  assert.deepEqual(truth.inputSchema.properties.mode.enum, ['full', 'delta']);
  assert.equal(truth.inputSchema.properties.min_objects.minimum, 1);
  assert.equal(truth.inputSchema.properties.timeout_ms.maximum, 30000);
  const act = listed.find((tool) => tool.name === 'saccade.act');
  const open = listed.find((tool) => tool.name === 'saccade.tabs.open');
  assert.equal(open.inputSchema.properties.browser_instance_id.maxLength, 256);
  assert.equal(act.inputSchema.properties.steps.maxItems, 32);
  assert.equal(act.inputSchema.properties.max_actions.maximum, 1000);
  assert.equal(act.inputSchema.properties.start_object_id.minLength, 1);
  assert.equal(act.inputSchema.properties.timeout_ms.maximum, 60000);
  assert.ok(act.inputSchema.properties.operation.enum.includes('upload'));
  assert.equal(act.inputSchema.properties.file_path.maxLength, 4096);
  assert.equal(act.inputSchema.properties.file_sha256.pattern, '^[a-f0-9]{64}$');
  assert.ok(!act.inputSchema.properties.steps.items.properties.operation.enum.includes('upload'));
  assert.equal(act.inputSchema.anyOf, undefined);
  assert.equal(listed.some((tool) => tool.inputSchema.anyOf || tool.inputSchema.oneOf || tool.inputSchema.allOf), false);
});

test('public tool mapping has no native or reference actuator route', () => {
  assert.equal(methodForTool('saccade.act'), 'act');
  assert.equal(methodForTool('saccade.reference.act_native'), undefined);
  assert.equal(methodForTool('saccade.native'), undefined);
});

test('MCP gives Agents compact self-describing Truth while preserving semantic geometry', () => {
  const compact = JSON.parse(agentText({
    schema: 'saccade.agent-truth/2', tab_id: '7', document_id: 'document-1',
    revision: 3, mode: 'full', complete: true, next_basis_revision: 3,
    frames: [{ frame_id: 'frame-1', status: 'observed' }],
    geometry: { unit: 'css_px', coordinate_space: 'content_viewport' },
    objects: [{
      object_id: 'object.1', object_revision: 3, frame_id: 'frame-1',
      kind: 'control', role: 'button', name: 'Continue', affordances: ['click'],
      state: { enabled: 'true' }, protected: false,
      document_bounds: { x: 1, y: 2, width: 3, height: 4 },
      viewport_bounds: { x: 5, y: 6, width: 3, height: 4 },
      visibility: 'visible', transition: 'none', action_token: 'not-for-model',
    }],
    changes: [], limitations: [], gap: false,
  }));
  assert.equal(compact.encoding, 'compact_rows/1');
  assert.deepEqual(compact.objects[0].slice(0, 5), ['object.1', 3, 0, 'control', 'button']);
  assert.deepEqual(compact.objects[0][11], [1, 2, 3, 4]);
  assert.equal(compact.objects[0][15], true);
  assert.doesNotMatch(JSON.stringify(compact), /not-for-model|action_token/);
  assert.deepEqual(agentResult({ schema: 'other/1', ok: true }), { schema: 'other/1', ok: true });
});
