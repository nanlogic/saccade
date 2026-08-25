'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const { spawnSync } = require('node:child_process');

const PACKAGE_ROOT = path.resolve(__dirname, '..');
const CLI = path.join(PACKAGE_ROOT, 'bin', 'saccade-setup.js');
const ORIGIN = 'chrome-extension://abcdefghijklmnopabcdefghijklmnop/';
const CANDIDATE = {
  schema: 'saccade.extension-candidate/1',
  id: 'c34eb1214f470328dde1758c9e9367e6dc6e9f7a9ad142027a6b6af69dd66c7f',
  version: '0.3.22',
};
const CONTRACT_HASH = 'a'.repeat(64);

async function fixture() {
  const root = await fsp.mkdtemp(path.join(os.tmpdir(), 'saccade-setup-'));
  const home = path.join(root, 'home');
  const runtime = path.join(root, 'runtime');
  const release = path.join(root, 'release.json');
  await fsp.mkdir(home, { recursive: true });
  await writeRuntime(runtime, 'one');
  await writeRelease(release, runtime, '1.0.0');
  return { root, home, runtime, release };
}

async function writeRuntime(
  target,
  marker,
  candidate = CANDIDATE,
  runtimeVersion = '1.0.0',
  contractHash = CONTRACT_HASH,
  ready = true,
) {
  const doctor = JSON.stringify({
    schema: 'saccade.doctor/1',
    runtime_version: runtimeVersion,
    mcp_contract_hash: contractHash,
    observation_schema: 'saccade.observation/1',
    host_protocol: 'saccade-extension-host/1',
    ready,
    detail: ready ? undefined : 'Native Host is not connected',
    capabilities: {
      schema: 'saccade.capabilities/6',
      extension_candidate: candidate,
      expected_extension_candidate: candidate,
    },
  });
  await fsp.writeFile(target, `#!/bin/sh\nif [ "$1" = doctor ]; then printf '%s\\n' '${doctor}'; fi\n# ${marker}\n`, { mode: 0o700 });
}

async function writeRelease(
  target,
  runtime,
  version,
  checksum,
  contractHash = CONTRACT_HASH,
  platform = 'darwin-arm64',
  extra = {},
) {
  const data = await fsp.readFile(runtime);
  const value = {
    schema: 'saccade.setup-release/1',
    published: true,
    version,
    mcp_contract_hash: contractHash,
    extension_candidate: CANDIDATE,
    native_host: { name: 'com.nanlogic.saccade', allowed_origins: [ORIGIN] },
    artifacts: {
      [platform]: {
        url: pathToFileURL(runtime).toString(),
        sha256: checksum || crypto.createHash('sha256').update(data).digest('hex'),
      },
    },
    ...extra,
  };
  await fsp.writeFile(target, `${JSON.stringify(value, null, 2)}\n`);
}

test('a source build points the Agent at the exact unpacked Extension', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const extension = path.join(values.root, 'extension');
  await fsp.mkdir(extension);
  await fsp.writeFile(path.join(extension, 'manifest.json'), '{}');
  await writeRuntime(values.runtime, 'source', CANDIDATE, '1.0.0', CONTRACT_HASH, false);
  await writeRelease(
    values.release,
    values.runtime,
    '1.0.0',
    undefined,
    CONTRACT_HASH,
    'darwin-arm64',
    { source_build: true, source_extension: extension },
  );

  const installed = run(['--release-manifest', values.release], values);
  assert.equal(installed.status, 0, installed.stderr);
  const pendingLine = installed.stdout.split('\n')
    .find((line) => line.startsWith('SACCADE_EXTENSION_PENDING '));
  assert.ok(pendingLine);
  assert.deepEqual(JSON.parse(pendingLine.slice('SACCADE_EXTENSION_PENDING '.length)), {
    path: extension,
    id: 'abcdefghijklmnopabcdefghijklmnop',
    version: CANDIDATE.version,
  });
  assert.doesNotMatch(installed.stdout, /chromewebstore\.google\.com/);
});

test('Codex config fallback handles an installed but unlaunchable CLI', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const codexConfig = path.join(values.home, '.codex', 'config.toml');
  await fsp.mkdir(path.dirname(codexConfig), { recursive: true });
  await fsp.writeFile(codexConfig, `[mcp_servers.saccade]\ncommand = "C:\\\\Program Files\\\\Saccade\\\\saccade-mcp.exe"\nargs = ["serve-stdio"]\n`);
  const environment = {
    SACCADE_SETUP_DISABLE_CLIENTS: '0',
    SACCADE_SETUP_CODEX: values.root,
    SACCADE_SETUP_CODEX_CONFIG: codexConfig,
    SACCADE_SETUP_CLAUDE: path.join(values.root, 'missing-claude'),
  };

  const installed = run(['--release-manifest', values.release], values, environment);
  assert.equal(installed.status, 0, installed.stderr);
  assert.match(installed.stdout, /Clients: codex/);
  const config = await fsp.readFile(codexConfig, 'utf8');
  assert.match(config, /# saccade-setup:start/);
  assert.match(config, /\[mcp_servers\.saccade\]/);
  assert.doesNotMatch(config, /serve-stdio/);

  const removed = run(['uninstall'], values, environment);
  assert.equal(removed.status, 0, removed.stdout + removed.stderr);
  assert.doesNotMatch(await fsp.readFile(codexConfig, 'utf8'), /mcp_servers\.saccade/);
});

async function writeFakeRegistry(target, stateFile) {
  const source = `#!/usr/bin/env node
const fs = require('node:fs');
const args = process.argv.slice(2);
const action = args[0];
const key = args[1];
let state = {};
try { state = JSON.parse(fs.readFileSync(process.env.SACCADE_TEST_REGISTRY_STATE, 'utf8')); } catch {}
if (action === 'query') {
  if (!(key in state)) process.exit(1);
  console.log('    (Default)    REG_SZ    ' + state[key]);
} else if (action === 'add') {
  const index = args.indexOf('/d');
  if (index < 0 || !args[index + 1]) process.exit(2);
  state[key] = args[index + 1];
  fs.writeFileSync(process.env.SACCADE_TEST_REGISTRY_STATE, JSON.stringify(state));
} else if (action === 'delete') {
  if (!(key in state)) process.exit(1);
  delete state[key];
  fs.writeFileSync(process.env.SACCADE_TEST_REGISTRY_STATE, JSON.stringify(state));
} else process.exit(2);
`;
  await fsp.writeFile(target, source, { mode: 0o700 });
  await fsp.writeFile(stateFile, '{}');
}

function run(args, values, extra = {}) {
  return spawnSync(process.execPath, [CLI, ...args], {
    encoding: 'utf8',
    env: {
      ...process.env,
      SACCADE_SETUP_HOME: values.home,
      SACCADE_SETUP_PLATFORM: 'darwin',
      SACCADE_SETUP_ARCH: 'arm64',
      SACCADE_SETUP_DISABLE_CLIENTS: '1',
      ...extra,
    },
  });
}

test('install, doctor, update, and uninstall preserve the Profile', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const installed = run(['--release-manifest', values.release], values);
  assert.equal(installed.status, 0, installed.stderr);
  assert.match(installed.stdout, /Start a new Codex or Claude task/);
  const runtime = path.join(values.home, 'Library/Application Support/Saccade/runtime/saccade-runtime');
  const profile = path.join(values.home, 'Library/Application Support/Saccade/profile.json');
  const expectedCandidate = path.join(values.home, 'Library/Application Support/Saccade/expected-extension-candidate.json');
  assert.match(await fsp.readFile(runtime, 'utf8'), /# one/);
  assert.deepEqual(JSON.parse(await fsp.readFile(expectedCandidate, 'utf8')), CANDIDATE);
  const installedProfile = JSON.parse(await fsp.readFile(profile, 'utf8'));
  assert.match(installedProfile.behavior, /deferred or lazy registry/);
  assert.match(installedProfile.behavior, /instead of silently falling back/);
  const customized = { name: 'mine', behavior: 'keep this', ban: [] };
  await fsp.writeFile(profile, `${JSON.stringify(customized)}\n`);
  const doctor = run(['doctor'], values);
  assert.equal(doctor.status, 0, doctor.stdout + doctor.stderr);
  assert.match(doctor.stdout, /OK exact Extension → Native Host → Runtime → MCP candidate and contract/);

  await writeRuntime(values.runtime, 'two', CANDIDATE, '1.1.0');
  await writeRelease(values.release, values.runtime, '1.1.0');
  const updated = run(['update', '--release-manifest', values.release], values);
  assert.equal(updated.status, 0, updated.stderr);
  assert.match(await fsp.readFile(runtime, 'utf8'), /# two/);
  assert.deepEqual(JSON.parse(await fsp.readFile(profile, 'utf8')), customized);

  const removed = run(['uninstall'], values);
  assert.equal(removed.status, 0, removed.stderr);
  assert.equal(fs.existsSync(runtime), false);
  assert.equal(fs.existsSync(expectedCandidate), false);
  assert.equal(fs.existsSync(profile), true);
  assert.match(removed.stdout, /Profile was preserved/);
});

test('Windows x64 installs one Runtime, one manifest, and Chrome and Edge registry values', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  await writeRelease(values.release, values.runtime, '1.0.0', undefined, CONTRACT_HASH, 'win32-x64');
  const registry = path.join(values.root, 'reg.exe');
  const registryState = path.join(values.root, 'registry.json');
  await writeFakeRegistry(registry, registryState);
  const localAppData = path.join(values.home, 'AppData', 'Local');
  const appData = path.join(values.home, 'AppData', 'Roaming');
  const environment = {
    SACCADE_SETUP_PLATFORM: 'win32',
    SACCADE_SETUP_ARCH: 'x64',
    SACCADE_SETUP_LOCALAPPDATA: localAppData,
    SACCADE_SETUP_APPDATA: appData,
    SACCADE_SETUP_REG: registry,
    SACCADE_TEST_REGISTRY_STATE: registryState,
  };

  const installed = run(['--release-manifest', values.release], values, environment);
  assert.equal(installed.status, 0, installed.stdout + installed.stderr);
  const root = path.join(localAppData, 'Saccade');
  const runtime = path.join(root, 'runtime', 'saccade-runtime.exe');
  const expectedCandidate = path.join(root, 'expected-extension-candidate.json');
  const manifest = path.join(root, 'native-messaging', 'com.nanlogic.saccade.json');
  assert.equal(fs.existsSync(runtime), true);
  assert.deepEqual(JSON.parse(await fsp.readFile(expectedCandidate, 'utf8')), CANDIDATE);
  assert.equal(JSON.parse(await fsp.readFile(manifest, 'utf8')).path, runtime);
  const registrations = JSON.parse(await fsp.readFile(registryState, 'utf8'));
  assert.equal(registrations['HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\com.nanlogic.saccade'], manifest);
  assert.equal(registrations['HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.nanlogic.saccade'], manifest);
  const state = JSON.parse(await fsp.readFile(path.join(root, 'setup-state.json'), 'utf8'));
  assert.equal(state.platform, 'win32-x64');
  assert.equal(state.native_manifests[0], manifest);
  assert.equal(state.native_registrations.length, 2);

  const doctor = run(['doctor'], values, environment);
  assert.equal(doctor.status, 0, doctor.stdout + doctor.stderr);
  assert.match(doctor.stdout, /OK Native Host Chrome registration/);
  assert.match(doctor.stdout, /OK Native Host Microsoft Edge registration/);

  const removed = run(['uninstall'], values, environment);
  assert.equal(removed.status, 0, removed.stdout + removed.stderr);
  assert.deepEqual(JSON.parse(await fsp.readFile(registryState, 'utf8')), {});
  assert.equal(fs.existsSync(runtime), false);
  assert.equal(fs.existsSync(expectedCandidate), false);
  assert.equal(fs.existsSync(manifest), false);
});

test('a disconnected install prints exact Extension installation steps', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  await writeRuntime(values.runtime, 'disconnected', CANDIDATE, '1.0.0', CONTRACT_HASH, false);
  await writeRelease(values.release, values.runtime, '1.0.0');

  const installed = run(['--release-manifest', values.release], values);
  assert.equal(installed.status, 0, installed.stderr);
  assert.match(installed.stderr, /Browser Extension connectivity is pending/);
  assert.match(installed.stdout, /Saccade browser Extension setup/);
  assert.match(installed.stdout, /https:\/\/chromewebstore\.google\.com\/detail\/saccade\/abcdefghijklmnopabcdefghijklmnop/);
  assert.match(installed.stdout, /Add to Chrome/);
  assert.match(installed.stdout, /allow extensions from other stores/);
  assert.match(installed.stdout, /npx -y @nanlogic\/saccade doctor/);
  assert.match(installed.stdout, /Restart Codex or Claude/);
  assert.match(installed.stdout, /Expected Extension version: 0\.3\.22/);

  const doctor = run(['doctor'], values);
  assert.equal(doctor.status, 1);
  assert.match(doctor.stdout, /Native Host is not connected/);
  assert.match(doctor.stdout, /Saccade browser Extension setup/);
});

test('doctor without setup state tells the user how to install', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const result = run(['doctor'], values);
  assert.equal(result.status, 1);
  assert.match(result.stdout, /Next: install Saccade with npx -y @nanlogic\/saccade/);
});

test('checksum failure leaves no installation behind', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  await writeRelease(values.release, values.runtime, '1.0.0', '0'.repeat(64));
  const result = run(['--release-manifest', values.release], values);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /checksum verification failed/);
  assert.equal(fs.existsSync(path.join(values.home, 'Library/Application Support/Saccade')), false);
});

test('repeat install is idempotent and preserves a custom Profile', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const profile = path.join(values.home, 'Library/Application Support/Saccade/profile.json');
  const state = path.join(values.home, 'Library/Application Support/Saccade/setup-state.json');
  const customized = { name: 'mine', behavior: 'stay mine', ban: [] };
  await fsp.writeFile(profile, `${JSON.stringify(customized)}\n`);
  const before = JSON.parse(await fsp.readFile(state, 'utf8'));
  const repeated = run(['--release-manifest', values.release], values);
  assert.equal(repeated.status, 0, repeated.stderr);
  assert.deepEqual(JSON.parse(await fsp.readFile(profile, 'utf8')), customized);
  assert.deepEqual(JSON.parse(await fsp.readFile(state, 'utf8')), before);
});

test('failed update rolls back Runtime and setup state', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const installedRuntime = path.join(values.home, 'Library/Application Support/Saccade/runtime/saccade-runtime');
  const state = path.join(values.home, 'Library/Application Support/Saccade/setup-state.json');
  const beforeRuntime = await fsp.readFile(installedRuntime);
  const beforeState = await fsp.readFile(state);
  const expectedCandidate = path.join(values.home, 'Library/Application Support/Saccade/expected-extension-candidate.json');
  const beforeExpectedCandidate = await fsp.readFile(expectedCandidate);
  await writeRuntime(values.runtime, 'update-that-must-roll-back', CANDIDATE, '1.1.0');
  await writeRelease(values.release, values.runtime, '1.1.0');
  const chromeManifest = path.join(values.home, 'Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nanlogic.saccade.json');
  await fsp.rm(chromeManifest);
  await fsp.mkdir(chromeManifest);
  const result = run(['update', '--release-manifest', values.release], values);
  assert.equal(result.status, 1);
  assert.deepEqual(await fsp.readFile(installedRuntime), beforeRuntime);
  assert.deepEqual(await fsp.readFile(state), beforeState);
  assert.deepEqual(await fsp.readFile(expectedCandidate), beforeExpectedCandidate);
});

test('doctor rejects a stale live Extension candidate', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const installedRuntime = path.join(values.home, 'Library/Application Support/Saccade/runtime/saccade-runtime');
  await writeRuntime(installedRuntime, 'stale', { ...CANDIDATE, id: 'f'.repeat(64) });
  const state = JSON.parse(await fsp.readFile(path.join(values.home, 'Library/Application Support/Saccade/setup-state.json'), 'utf8'));
  state.runtime_sha256 = crypto.createHash('sha256').update(await fsp.readFile(installedRuntime)).digest('hex');
  await fsp.writeFile(path.join(values.home, 'Library/Application Support/Saccade/setup-state.json'), `${JSON.stringify(state)}\n`);
  const doctor = run(['doctor'], values);
  assert.equal(doctor.status, 1);
  assert.match(doctor.stdout, /FAIL exact Extension → Native Host → Runtime → MCP candidate and contract/);
});

test('doctor reports a missing expected Extension candidate file', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const expectedCandidate = path.join(
    values.home,
    'Library/Application Support/Saccade/expected-extension-candidate.json',
  );
  await fsp.rm(expectedCandidate);
  const doctor = run(['doctor'], values);
  assert.equal(doctor.status, 1);
  assert.match(doctor.stdout, /FAIL expected Extension candidate: missing or invalid/);
});

test('doctor rejects a stale Runtime MCP contract', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const installedRuntime = path.join(values.home, 'Library/Application Support/Saccade/runtime/saccade-runtime');
  await writeRuntime(installedRuntime, 'stale-contract', CANDIDATE, '1.0.0', 'b'.repeat(64));
  const statePath = path.join(values.home, 'Library/Application Support/Saccade/setup-state.json');
  const state = JSON.parse(await fsp.readFile(statePath, 'utf8'));
  state.runtime_sha256 = crypto.createHash('sha256').update(await fsp.readFile(installedRuntime)).digest('hex');
  await fsp.writeFile(statePath, `${JSON.stringify(state)}\n`);
  const doctor = run(['doctor'], values);
  assert.equal(doctor.status, 1);
  assert.match(doctor.stdout, /FAIL exact Extension → Native Host → Runtime → MCP candidate and contract/);
});

test('install rejects a published release without an MCP contract identity', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const release = JSON.parse(await fsp.readFile(values.release, 'utf8'));
  delete release.mcp_contract_hash;
  await fsp.writeFile(values.release, `${JSON.stringify(release)}\n`);
  const result = run(['--release-manifest', values.release], values);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /valid MCP contract hash/);
});

test('doctor remains exact across fresh Runtime and Host invocations', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  for (let restart = 0; restart < 2; restart += 1) {
    const doctor = run(['doctor'], values);
    assert.equal(doctor.status, 0, doctor.stdout + doctor.stderr);
    assert.match(doctor.stdout, /OK exact Extension → Native Host → Runtime → MCP candidate and contract/);
  }
});

test('purge removes Profile and the complete managed root', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const managedRoot = path.join(values.home, 'Library/Application Support/Saccade');
  const removed = run(['uninstall', '--purge'], values);
  assert.equal(removed.status, 0, removed.stderr);
  assert.equal(fs.existsSync(managedRoot), false);
});

test('purge after an ordinary uninstall still removes the preserved Profile', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const managedRoot = path.join(values.home, 'Library/Application Support/Saccade');
  const profile = path.join(managedRoot, 'profile.json');

  const removed = run(['uninstall'], values);
  assert.equal(removed.status, 0, removed.stderr);
  assert.equal(fs.existsSync(profile), true);

  const purged = run(['uninstall', '--purge'], values);
  assert.equal(purged.status, 0, purged.stderr);
  assert.match(purged.stdout, /purged/);
  assert.equal(fs.existsSync(managedRoot), false);

  const again = run(['uninstall', '--purge'], values);
  assert.equal(again.status, 0, again.stderr);
  assert.match(again.stdout, /not installed/);
});

test('Claude Desktop configuration is additive and removed only when unchanged', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const desktop = path.join(values.root, 'claude-desktop.json');
  await fsp.writeFile(desktop, `${JSON.stringify({ mcpServers: { existing: { command: 'keep' } }, preference: true })}\n`);
  const environment = {
    SACCADE_SETUP_DISABLE_CLIENTS: '0',
    SACCADE_SETUP_CODEX: path.join(values.root, 'missing-codex'),
    SACCADE_SETUP_CLAUDE: path.join(values.root, 'missing-claude'),
    SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG: desktop,
  };
  const installed = run(['--release-manifest', values.release], values, environment);
  assert.equal(installed.status, 0, installed.stderr);
  let configuration = JSON.parse(await fsp.readFile(desktop, 'utf8'));
  assert.deepEqual(configuration.mcpServers.existing, { command: 'keep' });
  assert.equal(configuration.preference, true);
  assert.equal(configuration.mcpServers.saccade.args[0], 'mcp');

  const removed = run(['uninstall'], values, environment);
  assert.equal(removed.status, 0, removed.stderr);
  configuration = JSON.parse(await fsp.readFile(desktop, 'utf8'));
  assert.deepEqual(configuration.mcpServers, { existing: { command: 'keep' } });
});

test('invalid Claude Desktop JSON is preserved', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const desktop = path.join(values.root, 'claude-desktop.json');
  await fsp.writeFile(desktop, '{broken');
  const result = run(['--release-manifest', values.release], values, {
    SACCADE_SETUP_DISABLE_CLIENTS: '0',
    SACCADE_SETUP_CODEX: path.join(values.root, 'missing-codex'),
    SACCADE_SETUP_CLAUDE: path.join(values.root, 'missing-claude'),
    SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG: desktop,
  });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(await fsp.readFile(desktop, 'utf8'), '{broken');
  assert.match(result.stderr, /not valid JSON; it was preserved/);
});

test('Codex and Claude Code receive user-level stdio MCP entries', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const stateFile = path.join(values.root, 'client-state.json');
  const fakeClient = `#!/usr/bin/env node
const fs = require('node:fs');
const path = require('node:path');
const kind = path.basename(process.argv[1]);
const args = process.argv.slice(2);
let state = {};
try { state = JSON.parse(fs.readFileSync(process.env.SACCADE_TEST_CLIENT_STATE, 'utf8')); } catch {}
const entry = state[kind];
if (args[0] !== 'mcp') process.exit(2);
// Both real CLIs take the server name immediately after the subcommand. Claude
// Code rejects a name that follows "-e", because it reads it as the next
// environment pair. Reproduce that contract so argument order stays covered.
if (args[1] === 'add' || args[1] === 'remove') {
  if (args[2] !== 'saccade') {
    console.error('Invalid environment variable format: ' + args[2]);
    process.exit(1);
  }
}
if (args[1] === 'get') {
  if (!entry) process.exit(1);
  if (kind === 'codex') console.log(JSON.stringify({transport:{type:'stdio',command:entry.command,args:['mcp'],env:{SACCADE_RUNTIME_DIR:entry.runtimeDir}}}));
  else console.log('Command: ' + entry.command + '\\nSACCADE_RUNTIME_DIR=' + entry.runtimeDir);
} else if (args[1] === 'add') {
  const separator = args.indexOf('--');
  const environmentArgument = args.find((value) => value.startsWith('SACCADE_RUNTIME_DIR='));
  state[kind] = { command: args[separator + 1], runtimeDir: environmentArgument.slice('SACCADE_RUNTIME_DIR='.length) };
  fs.writeFileSync(process.env.SACCADE_TEST_CLIENT_STATE, JSON.stringify(state));
} else if (args[1] === 'remove') {
  delete state[kind];
  fs.writeFileSync(process.env.SACCADE_TEST_CLIENT_STATE, JSON.stringify(state));
} else process.exit(2);
`;
  const codex = path.join(values.root, 'codex');
  const claude = path.join(values.root, 'claude');
  await fsp.writeFile(codex, fakeClient, { mode: 0o700 });
  await fsp.writeFile(claude, fakeClient, { mode: 0o700 });
  const environment = {
    SACCADE_SETUP_DISABLE_CLIENTS: '0',
    SACCADE_SETUP_CODEX: codex,
    SACCADE_SETUP_CLAUDE: claude,
    SACCADE_TEST_CLIENT_STATE: stateFile,
  };
  const installed = run(['--release-manifest', values.release], values, environment);
  assert.equal(installed.status, 0, installed.stderr);
  const clientState = JSON.parse(await fsp.readFile(stateFile, 'utf8'));
  assert.equal(clientState.codex.command.endsWith('/saccade-runtime'), true);
  assert.deepEqual(clientState.codex, clientState.claude);
  const doctor = run(['doctor'], values, environment);
  assert.equal(doctor.status, 0, doctor.stdout + doctor.stderr);
  assert.match(doctor.stdout, /OK Codex MCP/);
  assert.match(doctor.stdout, /OK Claude Code MCP/);
  const removed = run(['uninstall'], values, environment);
  assert.equal(removed.status, 0, removed.stderr);
  assert.deepEqual(JSON.parse(await fsp.readFile(stateFile, 'utf8')), {});
});

test('package has an explicit CLI and no install-time hook', async () => {
  const packageJson = JSON.parse(await fsp.readFile(path.join(PACKAGE_ROOT, 'package.json'), 'utf8'));
  assert.equal(packageJson.name, '@nanlogic/saccade');
  assert.equal(packageJson.version, '0.1.3');
  assert.equal(packageJson.bin['saccade-setup'], 'bin/saccade-setup.js');
  assert.equal(packageJson.scripts.postinstall, undefined);
  const release = JSON.parse(await fsp.readFile(path.join(PACKAGE_ROOT, 'release.json'), 'utf8'));
  assert.equal(release.published, false);
});

test('a late install failure rolls back every managed file', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  const statePath = path.join(values.home, 'Library/Application Support/Saccade/setup-state.json');
  await fsp.mkdir(statePath, { recursive: true });
  const result = run(['--release-manifest', values.release], values);
  assert.equal(result.status, 1);
  assert.match(result.stderr, /refusing to replace non-file/);
  assert.equal(fs.existsSync(path.join(values.home, 'Library/Application Support/Saccade/runtime/saccade-runtime')), false);
  const chromeManifest = path.join(values.home, 'Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nanlogic.saccade.json');
  const edgeManifest = path.join(values.home, 'Library/Application Support/Microsoft Edge/NativeMessagingHosts/com.nanlogic.saccade.json');
  assert.equal(fs.existsSync(chromeManifest), false);
  assert.equal(fs.existsSync(edgeManifest), false);
});

test('uninstall preserves a modified managed manifest and its cleanup state', async (t) => {
  const values = await fixture();
  t.after(() => fsp.rm(values.root, { recursive: true, force: true }));
  assert.equal(run(['--release-manifest', values.release], values).status, 0);
  const manifest = path.join(values.home, 'Library/Application Support/Google/Chrome/NativeMessagingHosts/com.nanlogic.saccade.json');
  const state = path.join(values.home, 'Library/Application Support/Saccade/setup-state.json');
  await fsp.writeFile(manifest, '{"changed":true}\n');
  const removed = run(['uninstall'], values);
  assert.equal(removed.status, 1);
  assert.equal(fs.existsSync(manifest), true);
  assert.equal(fs.existsSync(state), true);
  assert.match(removed.stdout, /uninstall is incomplete/);
});
