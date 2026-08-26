'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const test = require('node:test');
const setup = require('../src/setup');

async function fixture() {
  const root = await fsp.mkdtemp(path.join(os.tmpdir(), 'saccade-node-setup-'));
  return { root, environment: {
    SACCADE_SETUP_HOME: root,
    SACCADE_SETUP_CODEX_CONFIG: path.join(root, 'codex.toml'),
    SACCADE_SETUP_CLAUDE_CONFIG: path.join(root, 'claude.json'),
    SACCADE_SETUP_SKIP_BROKER: '1',
  } };
}

test('setup is platform-independent Node configuration only', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const paths = await setup.install({}, values.environment);
  const state = JSON.parse(await fsp.readFile(paths.state, 'utf8'));
  assert.equal(state.runtime, 'node');
  assert.deepEqual(state.mcp, { command: 'npx', args: ['-y', '@nanlogic/saccade', 'mcp'] });
  assert.match(await fsp.readFile(paths.codexConfig, 'utf8'), /@nanlogic\/saccade/);
  assert.deepEqual(JSON.parse(await fsp.readFile(paths.claudeConfig, 'utf8')).mcpServers.saccade, state.mcp);
  assert.equal('platform' in state, false);
  assert.equal('native_host' in state, false);
});

test('update and uninstall preserve a customized Profile', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const paths = await setup.install({}, values.environment);
  await fsp.writeFile(paths.profile, '{"custom":true}\n');
  await setup.install({}, values.environment);
  assert.equal(await fsp.readFile(paths.profile, 'utf8'), '{"custom":true}\n');
  assert.equal((await setup.doctor(values.environment)).ok, true);
  await fsp.writeFile(paths.brokerState, '{"recovery":"metadata"}\n');
  await setup.uninstall({}, values.environment);
  assert.equal(fs.existsSync(paths.state), false);
  assert.equal(fs.existsSync(paths.brokerState), false);
  assert.equal(fs.existsSync(paths.profile), true);
  assert.doesNotMatch(await fsp.readFile(paths.codexConfig, 'utf8'), /mcp_servers\.saccade/);
});

test('purge removes only the dedicated Saccade data directory', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const paths = await setup.install({}, values.environment);
  await setup.uninstall({ purge: true }, values.environment);
  assert.equal(fs.existsSync(paths.root), false);
});
