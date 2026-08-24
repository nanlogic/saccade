'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const fsp = require('node:fs/promises');
const https = require('node:https');
const os = require('node:os');
const path = require('node:path');
const { fileURLToPath } = require('node:url');
const { spawnSync } = require('node:child_process');

const PACKAGE_ROOT = path.resolve(__dirname, '..');
const DEFAULT_RELEASE = path.join(PACKAGE_ROOT, 'release.json');
const DEFAULT_PROFILE = path.join(PACKAGE_ROOT, 'default-profile.json');
const STATE_SCHEMA = 'saccade.setup-state/1';
const EXTENSION_CANDIDATE_SCHEMA = 'saccade.extension-candidate/1';
const CAPABILITIES_SCHEMA = 'saccade.capabilities/6';
const OBSERVATION_SCHEMA = 'saccade.observation/1';
const HOST_PROTOCOL = 'saccade-extension-host/1';
const SETUP_COMMAND = 'npx -y @nanlogic/saccade';

function parseArguments(argv) {
  const options = { command: 'install', purge: false, releaseManifest: DEFAULT_RELEASE };
  const values = [...argv];
  if (values[0] && !values[0].startsWith('-')) options.command = values.shift();
  if (!['install', 'update', 'doctor', 'uninstall'].includes(options.command)) {
    throw new Error(`unknown command ${options.command}`);
  }
  while (values.length) {
    const flag = values.shift();
    if (flag === '--purge' && options.command === 'uninstall') options.purge = true;
    else if (flag === '--release-manifest' && values.length) {
      options.releaseManifest = path.resolve(values.shift());
    } else if (flag === '--help' || flag === '-h') options.help = true;
    else throw new Error(`unknown option ${flag}`);
  }
  return options;
}

function printHelp() {
  console.log(`Usage: saccade-setup [install|doctor|update|uninstall] [options]

Commands:
  install      Install or repair the current package release (default)
  doctor       Verify files, client configuration, and live Extension connectivity
  update       Install the Runtime release bundled with this setup package
  uninstall   Remove Saccade Runtime and managed configuration

Options:
  --purge      With uninstall, also remove the user Profile and Runtime data
  --help       Show this help`);
}

function platformKey(environment = process.env) {
  const platform = environment.SACCADE_SETUP_PLATFORM || process.platform;
  const architecture = environment.SACCADE_SETUP_ARCH || process.arch;
  if (platform === 'darwin' && ['arm64', 'x64'].includes(architecture)) {
    return `${platform}-${architecture}`;
  }
  if (platform === 'win32' && architecture === 'x64') {
    return `${platform}-${architecture}`;
  }
  if (!['darwin', 'win32'].includes(platform)) {
    throw new Error(`unsupported platform ${platform}; setup supports macOS and Windows x64`);
  }
  if (platform === 'win32') {
    throw new Error(`unsupported Windows architecture ${architecture}; setup supports Windows x64`);
  }
  if (!['arm64', 'x64'].includes(architecture)) {
    throw new Error(`unsupported architecture ${architecture}`);
  }
  throw new Error(`unsupported platform ${platform}-${architecture}`);
}

function installPaths(environment = process.env) {
  const platform = environment.SACCADE_SETUP_PLATFORM || process.platform;
  const home = path.resolve(environment.SACCADE_SETUP_HOME || os.homedir());
  if (platform === 'win32') {
    const localAppData = path.resolve(
      environment.SACCADE_SETUP_LOCALAPPDATA
        || environment.LOCALAPPDATA
        || path.join(home, 'AppData', 'Local'),
    );
    const appData = path.resolve(
      environment.SACCADE_SETUP_APPDATA
        || environment.APPDATA
        || path.join(home, 'AppData', 'Roaming'),
    );
    const root = path.join(localAppData, 'Saccade');
    const runtimeDir = path.join(root, 'runtime');
    return {
      platform,
      home,
      root,
      runtimeDir,
      runtime: path.join(runtimeDir, 'saccade-runtime.exe'),
      profile: path.join(root, 'profile.json'),
      expectedExtensionCandidate: path.join(root, 'expected-extension-candidate.json'),
      state: path.join(root, 'setup-state.json'),
      nativeHostDirectories: [],
      nativeHostManifest: path.join(root, 'native-messaging', 'com.nanlogic.saccade.json'),
      nativeHostRegistryKeys: [
        'HKCU\\Software\\Google\\Chrome\\NativeMessagingHosts\\com.nanlogic.saccade',
        'HKCU\\Software\\Microsoft\\Edge\\NativeMessagingHosts\\com.nanlogic.saccade',
      ],
      claudeDesktop: environment.SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG
        ? path.resolve(environment.SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG)
        : path.join(appData, 'Claude', 'claude_desktop_config.json'),
    };
  }
  const root = path.join(home, 'Library', 'Application Support', 'Saccade');
  const runtimeDir = path.join(root, 'runtime');
  return {
    platform,
    home,
    root,
    runtimeDir,
    runtime: path.join(runtimeDir, 'saccade-runtime'),
    profile: path.join(root, 'profile.json'),
    expectedExtensionCandidate: path.join(root, 'expected-extension-candidate.json'),
    state: path.join(root, 'setup-state.json'),
    nativeHostDirectories: [
      path.join(home, 'Library', 'Application Support', 'Google', 'Chrome', 'NativeMessagingHosts'),
      path.join(home, 'Library', 'Application Support', 'Microsoft Edge', 'NativeMessagingHosts'),
    ],
    nativeHostManifest: null,
    nativeHostRegistryKeys: [],
    claudeDesktop: environment.SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG
      ? path.resolve(environment.SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG)
      : path.join(home, 'Library', 'Application Support', 'Claude', 'claude_desktop_config.json'),
  };
}

function sha256(data) {
  return crypto.createHash('sha256').update(data).digest('hex');
}

async function readJson(file) {
  return JSON.parse(await fsp.readFile(file, 'utf8'));
}

function validateRelease(release, key) {
  if (release.schema !== 'saccade.setup-release/1') throw new Error('invalid setup release schema');
  if (!release.published) throw new Error('this setup package has no published Runtime release yet');
  if (!/^\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$/.test(release.version || '')) {
    throw new Error('invalid release version');
  }
  const host = release.native_host || {};
  if (host.name !== 'com.nanlogic.saccade') throw new Error('invalid Native Host name');
  if (!Array.isArray(host.allowed_origins) || host.allowed_origins.length === 0) {
    throw new Error('release has no browser Extension origin');
  }
  for (const origin of host.allowed_origins) {
    if (!/^chrome-extension:\/\/[a-p]{32}\/$/.test(origin)) {
      throw new Error(`invalid browser Extension origin ${origin}`);
    }
  }
  const extensionCandidate = release.extension_candidate || {};
  if (extensionCandidate.schema !== EXTENSION_CANDIDATE_SCHEMA
      || !/^[a-f0-9]{64}$/.test(extensionCandidate.id || '')
      || !/^\d+\.\d+\.\d+([+-][0-9A-Za-z.-]+)?$/.test(extensionCandidate.version || '')) {
    throw new Error('release has no valid Extension candidate identity');
  }
  const mcpContractHash = release.mcp_contract_hash;
  if (!/^[a-f0-9]{64}$/.test(mcpContractHash || '')) {
    throw new Error('release has no valid MCP contract hash');
  }
  const artifact = release.artifacts && release.artifacts[key];
  if (!artifact || typeof artifact.url !== 'string' || !/^[a-f0-9]{64}$/.test(artifact.sha256 || '')) {
    throw new Error(`release has no valid ${key} Runtime artifact`);
  }
  return { host, artifact, extensionCandidate, mcpContractHash };
}

function fetchHttps(url, redirects = 0) {
  if (redirects > 5) return Promise.reject(new Error('too many Runtime download redirects'));
  return new Promise((resolve, reject) => {
    const request = https.get(url, { headers: { 'User-Agent': '@nanlogic/saccade' } }, (response) => {
      if (response.statusCode >= 300 && response.statusCode < 400 && response.headers.location) {
        response.resume();
        const next = new URL(response.headers.location, url).toString();
        resolve(fetchHttps(next, redirects + 1));
        return;
      }
      if (response.statusCode !== 200) {
        response.resume();
        reject(new Error(`Runtime download returned HTTP ${response.statusCode}`));
        return;
      }
      const chunks = [];
      response.on('data', (chunk) => chunks.push(chunk));
      response.on('end', () => resolve(Buffer.concat(chunks)));
    });
    request.on('error', reject);
  });
}

async function loadArtifact(url) {
  if (url.startsWith('https://')) return fetchHttps(url);
  if (url.startsWith('file://')) return fsp.readFile(fileURLToPath(url));
  if (!url.includes('://') && process.env.SACCADE_SETUP_ALLOW_LOCAL_ARTIFACT === '1') {
    return fsp.readFile(path.resolve(url));
  }
  throw new Error('Runtime artifact URL must use HTTPS');
}

class FileTransaction {
  constructor() {
    this.backups = new Map();
  }

  async capture(target) {
    if (this.backups.has(target)) return;
    try {
      const stat = await fsp.lstat(target);
      if (!stat.isFile()) throw new Error(`refusing to replace non-file ${target}`);
      this.backups.set(target, { data: await fsp.readFile(target), mode: stat.mode & 0o777 });
    } catch (error) {
      if (error.code === 'ENOENT') this.backups.set(target, null);
      else throw error;
    }
  }

  async write(target, data, mode) {
    await this.capture(target);
    await atomicWrite(target, data, mode);
  }

  async rollback() {
    for (const [target, backup] of [...this.backups.entries()].reverse()) {
      if (backup === null) await fsp.rm(target, { force: true });
      else {
        await fsp.mkdir(path.dirname(target), { recursive: true, mode: 0o700 });
        await fsp.writeFile(target, backup.data, { mode: backup.mode });
        await fsp.chmod(target, backup.mode);
      }
    }
  }
}

async function atomicWrite(target, data, mode) {
  await fsp.mkdir(path.dirname(target), { recursive: true, mode: 0o700 });
  const temporary = `${target}.installing-${process.pid}-${crypto.randomBytes(4).toString('hex')}`;
  try {
    await fsp.writeFile(temporary, data, { mode });
    await fsp.chmod(temporary, mode);
    await fsp.rename(temporary, target);
  } finally {
    await fsp.rm(temporary, { force: true });
  }
}

function nativeManifest(host, runtime) {
  return {
    name: host.name,
    description: 'Saccade live semantic Truth Layer Native Host',
    path: runtime,
    type: 'stdio',
    allowed_origins: host.allowed_origins,
  };
}

function expectedMcp(runtime, runtimeDir) {
  return { command: runtime, args: ['mcp'], env: { SACCADE_RUNTIME_DIR: runtimeDir } };
}

function executableOnPath(name, environment = process.env) {
  const platform = environment.SACCADE_SETUP_PLATFORM || process.platform;
  const names = platform === 'win32' ? [name, `${name}.exe`] : [name];
  for (const directory of (environment.PATH || '').split(path.delimiter)) {
    if (!directory) continue;
    for (const executable of names) {
      const candidate = path.join(directory, executable);
      try {
        fs.accessSync(candidate, fs.constants.X_OK);
        return candidate;
      } catch {}
    }
  }
  return null;
}

function findCodex(environment = process.env) {
  if (environment.SACCADE_SETUP_CODEX) return path.resolve(environment.SACCADE_SETUP_CODEX);
  const candidates = [
    '/Applications/Codex.app/Contents/Resources/codex',
    '/Applications/ChatGPT.app/Contents/Resources/codex',
    executableOnPath('codex', environment),
  ];
  return candidates.find((candidate) => candidate && fs.existsSync(candidate)) || null;
}

function findClaude(environment = process.env) {
  if (environment.SACCADE_SETUP_CLAUDE) return path.resolve(environment.SACCADE_SETUP_CLAUDE);
  return executableOnPath('claude', environment);
}

function run(command, args, environment = process.env) {
  return spawnSync(command, args, { encoding: 'utf8', env: environment });
}

function commandFailure(result) {
  return String(result.stderr || result.stdout || result.error && result.error.message || 'command unavailable').trim();
}

function registryCommand(environment = process.env) {
  return environment.SACCADE_SETUP_REG || 'reg.exe';
}

function readRegistryValue(key, environment = process.env) {
  const result = run(registryCommand(environment), ['query', key, '/ve'], environment);
  if (result.status !== 0) return null;
  const match = String(result.stdout || '').match(/REG_SZ\s+([^\r\n]+)\s*$/m);
  return match ? match[1].trim() : null;
}

function writeRegistryValue(key, value, environment = process.env) {
  const result = run(registryCommand(environment), [
    'add', key, '/ve', '/t', 'REG_SZ', '/d', value, '/f',
  ], environment);
  if (result.status !== 0) {
    throw new Error(`Native Host registry write failed for ${key}: ${commandFailure(result)}`);
  }
}

function removeRegistryValue(key, environment = process.env) {
  const result = run(registryCommand(environment), ['delete', key, '/f'], environment);
  return result.status === 0;
}

function nativeRegistrationLabel(key) {
  if (key.includes('\\Google\\Chrome\\')) return 'Chrome';
  if (key.includes('\\Microsoft\\Edge\\')) return 'Microsoft Edge';
  return key;
}

function codexMatches(value, expected) {
  const transport = value && value.transport;
  return Boolean(transport
    && transport.type === 'stdio'
    && transport.command === expected.command
    && JSON.stringify(transport.args || []) === JSON.stringify(expected.args)
    && transport.env
    && transport.env.SACCADE_RUNTIME_DIR === expected.env.SACCADE_RUNTIME_DIR);
}

async function configureClients(paths, previousState, transaction, rollbackActions, environment) {
  if (environment.SACCADE_SETUP_DISABLE_CLIENTS === '1') return { clients: [], warnings: [] };
  const expected = expectedMcp(paths.runtime, paths.root);
  const owned = new Set((previousState && previousState.clients) || []);
  const clients = [];
  const warnings = [];
  const codex = findCodex(environment);
  if (codex) {
    const current = run(codex, ['mcp', 'get', 'saccade', '--json'], environment);
    if (current.status === 0) {
      try {
        if (codexMatches(JSON.parse(current.stdout), expected)) clients.push('codex');
        else warnings.push('Codex already has a different MCP entry named saccade; it was preserved.');
      } catch {
        warnings.push('Codex returned an unreadable saccade MCP entry; it was preserved.');
      }
    } else {
      const added = run(codex, [
        'mcp', 'add', 'saccade', '--env', `SACCADE_RUNTIME_DIR=${paths.root}`,
        '--', paths.runtime, 'mcp',
      ], environment);
      if (added.status === 0) {
        clients.push('codex');
        rollbackActions.push(() => run(codex, ['mcp', 'remove', 'saccade'], environment));
      } else warnings.push(`Codex MCP configuration failed: ${commandFailure(added)}`);
    }
  }
  const claude = findClaude(environment);
  if (claude) {
    const current = run(claude, ['mcp', 'get', 'saccade'], environment);
    if (current.status === 0) {
      if (owned.has('claude-code')
          || (current.stdout.includes(paths.runtime) && current.stdout.includes(paths.root))) {
        clients.push('claude-code');
      } else warnings.push('Claude Code already has a different MCP entry named saccade; it was preserved.');
    } else {
      const added = run(claude, [
        'mcp', 'add', 'saccade', '--scope', 'user',
        '-e', `SACCADE_RUNTIME_DIR=${paths.root}`,
        '--', paths.runtime, 'mcp',
      ], environment);
      if (added.status === 0) {
        clients.push('claude-code');
        rollbackActions.push(() => run(claude, ['mcp', 'remove', 'saccade', '--scope', 'user'], environment));
      } else warnings.push(`Claude Code MCP configuration failed: ${commandFailure(added)}`);
    }
  }
  const desktopExplicit = Boolean(environment.SACCADE_SETUP_CLAUDE_DESKTOP_CONFIG);
  const desktopDetected = desktopExplicit || fs.existsSync(paths.claudeDesktop)
    || fs.existsSync('/Applications/Claude.app');
  if (desktopDetected) {
    let configuration = {};
    let configurationReadable = true;
    if (fs.existsSync(paths.claudeDesktop)) {
      try { configuration = await readJson(paths.claudeDesktop); }
      catch {
        configurationReadable = false;
        warnings.push('Claude Desktop configuration is not valid JSON; it was preserved.');
      }
    }
    if (configurationReadable) {
      if (!configuration.mcpServers) configuration.mcpServers = {};
      const current = configuration.mcpServers.saccade;
      if (!current || JSON.stringify(current) === JSON.stringify(expected)) {
        if (!current) {
          configuration.mcpServers.saccade = expected;
          await transaction.write(paths.claudeDesktop, `${JSON.stringify(configuration, null, 2)}\n`, 0o600);
        }
        clients.push('claude-desktop');
      } else {
        warnings.push('Claude Desktop already has a different MCP entry named saccade; it was preserved.');
      }
    }
  }
  if (!codex && !claude && !desktopDetected) {
    warnings.push('No supported local Codex or Claude client was detected.');
  }
  return { clients: [...new Set(clients)], warnings };
}

async function install(options, environment = process.env) {
  const paths = installPaths(environment);
  const key = platformKey(environment);
  const release = await readJson(options.releaseManifest);
  const { host, artifact, extensionCandidate, mcpContractHash } = validateRelease(release, key);
  const runtimeData = await loadArtifact(artifact.url);
  const digest = sha256(runtimeData);
  if (digest !== artifact.sha256) throw new Error('Runtime checksum verification failed');
  let previousState = null;
  try {
    previousState = await readJson(paths.state);
    if (previousState.schema !== STATE_SCHEMA) previousState = null;
  } catch {}
  const transaction = new FileTransaction();
  const rollbackActions = [];
  const manifest = nativeManifest(host, paths.runtime);
  const manifestText = `${JSON.stringify(manifest, null, 2)}\n`;
  const manifestPaths = paths.nativeHostManifest
    ? [paths.nativeHostManifest]
    : paths.nativeHostDirectories.map((directory) => path.join(directory, `${host.name}.json`));
  const registryValues = new Map();
  if (!previousState && fs.existsSync(paths.runtime)) {
    throw new Error(`unmanaged Runtime already exists at ${paths.runtime}`);
  }
  if (!previousState) {
    for (const target of manifestPaths) {
      if (fs.existsSync(target) && !jsonEqual(await readJson(target), manifest)) {
        throw new Error(`unmanaged Native Host manifest already exists at ${target}`);
      }
    }
    for (const key of paths.nativeHostRegistryKeys) {
      const current = readRegistryValue(key, environment);
      registryValues.set(key, current);
      if (current !== null && current !== paths.nativeHostManifest) {
        throw new Error(`unmanaged Native Host registry value already exists at ${key}`);
      }
    }
  } else {
    for (const key of paths.nativeHostRegistryKeys) {
      registryValues.set(key, readRegistryValue(key, environment));
    }
  }
  try {
    await transaction.write(paths.runtime, runtimeData, 0o700);
    if (!fs.existsSync(paths.profile)) {
      await transaction.write(paths.profile, await fsp.readFile(DEFAULT_PROFILE), 0o600);
    }
    await transaction.write(
      paths.expectedExtensionCandidate,
      `${JSON.stringify(extensionCandidate, null, 2)}\n`,
      0o600,
    );
    for (const target of manifestPaths) await transaction.write(target, manifestText, 0o600);
    for (const key of paths.nativeHostRegistryKeys) {
      const previous = registryValues.get(key);
      writeRegistryValue(key, paths.nativeHostManifest, environment);
      rollbackActions.push(() => {
        if (previous === null) removeRegistryValue(key, environment);
        else writeRegistryValue(key, previous, environment);
      });
    }
    const configured = await configureClients(paths, previousState, transaction, rollbackActions, environment);
    const state = {
      schema: STATE_SCHEMA,
      version: release.version,
      platform: key,
      runtime_sha256: digest,
      mcp_contract_hash: mcpContractHash,
      extension_candidate: extensionCandidate,
      native_host: host,
      native_manifests: manifestPaths,
      native_registrations: paths.nativeHostRegistryKeys,
      clients: configured.clients,
    };
    await transaction.write(paths.state, `${JSON.stringify(state, null, 2)}\n`, 0o600);
    console.log(`Saccade ${release.version} installed.`);
    console.log(`Runtime: ${paths.runtime}`);
    console.log(`Clients: ${configured.clients.length ? configured.clients.join(', ') : 'none'}`);
    console.log('Start a new Codex or Claude task, or restart the client, to load the Saccade MCP tools.');
    for (const warning of configured.warnings) console.warn(`Warning: ${warning}`);
    const health = await doctor(environment, false);
    if (!health.connected) {
      console.warn('Browser Extension connectivity is pending.');
      printExtensionInstructions(state);
    }
  } catch (error) {
    for (const action of rollbackActions.reverse()) action();
    await transaction.rollback();
    throw error;
  }
}

function jsonEqual(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function candidateMatches(left, right) {
  return Boolean(left && right
    && left.schema === EXTENSION_CANDIDATE_SCHEMA
    && right.schema === EXTENSION_CANDIDATE_SCHEMA
    && left.id === right.id
    && left.version === right.version);
}

function extensionStoreUrl(host) {
  for (const origin of host && host.allowed_origins || []) {
    const match = /^chrome-extension:\/\/([a-p]{32})\/$/.exec(origin);
    if (match) return `https://chromewebstore.google.com/detail/saccade/${match[1]}`;
  }
  return null;
}

function printExtensionInstructions(state, log = console.log) {
  const storeUrl = extensionStoreUrl(state && state.native_host);
  if (!storeUrl) return false;
  const version = state.extension_candidate && state.extension_candidate.version;
  log('');
  log('Saccade browser Extension setup:');
  log(`1. Open this page in Chrome or Edge: ${storeUrl}`);
  log('2. Chrome: click "Add to Chrome" and approve the browser confirmation.');
  log('   Edge: if prompted, allow extensions from other stores, then click "Add to Chrome".');
  log(`3. Keep the browser open, then run: ${SETUP_COMMAND} doctor`);
  log('4. Restart Codex or Claude if the Saccade MCP tools are not visible yet.');
  if (version) log(`Expected Extension version: ${version}`);
  return true;
}

async function doctor(environment = process.env, print = true) {
  const paths = installPaths(environment);
  const checks = [];
  let state;
  try {
    state = await readJson(paths.state);
    checks.push({
      name: 'setup state',
      ok: state.schema === STATE_SCHEMA
        && candidateMatches(state.extension_candidate, state.extension_candidate)
        && /^[a-f0-9]{64}$/.test(state.mcp_contract_hash || ''),
      detail: state.schema === STATE_SCHEMA
        && (!state.extension_candidate || !state.mcp_contract_hash)
        ? 'installed state predates exact candidate/contract verification; run update'
        : undefined,
    });
  } catch {
    state = null;
    checks.push({ name: 'setup state', ok: false, detail: 'not installed' });
  }
  if (!state) {
    if (print) {
      for (const check of checks) console.log(`${check.ok ? 'OK' : 'FAIL'} ${check.name}${check.detail ? `: ${check.detail}` : ''}`);
      console.log(`Next: install Saccade with ${SETUP_COMMAND}`);
    }
    return { ok: false, connected: false, checks };
  }
  try {
    const data = await fsp.readFile(paths.runtime);
    const stat = await fsp.stat(paths.runtime);
    const executable = paths.platform === 'win32' || Boolean(stat.mode & 0o100);
    checks.push({ name: 'Runtime checksum', ok: sha256(data) === state.runtime_sha256 && executable });
  } catch { checks.push({ name: 'Runtime checksum', ok: false, detail: 'missing' }); }
  try {
    const profile = await readJson(paths.profile);
    checks.push({ name: 'Profile', ok: profile && typeof profile.name === 'string' && typeof profile.behavior === 'string' && Array.isArray(profile.ban) });
  } catch { checks.push({ name: 'Profile', ok: false, detail: 'missing or invalid' }); }
  try {
    const expectedExtensionCandidate = await readJson(paths.expectedExtensionCandidate);
    checks.push({
      name: 'expected Extension candidate',
      ok: candidateMatches(expectedExtensionCandidate, state.extension_candidate),
    });
  } catch { checks.push({ name: 'expected Extension candidate', ok: false, detail: 'missing or invalid' }); }
  const expectedManifest = nativeManifest(state.native_host, paths.runtime);
  for (const target of state.native_manifests || []) {
    try { checks.push({ name: `Native Host ${path.basename(path.dirname(path.dirname(target)))}`, ok: jsonEqual(await readJson(target), expectedManifest) }); }
    catch { checks.push({ name: `Native Host ${target}`, ok: false, detail: 'missing or invalid' }); }
  }
  for (const key of state.native_registrations || []) {
    checks.push({
      name: `Native Host ${nativeRegistrationLabel(key)} registration`,
      ok: readRegistryValue(key, environment) === paths.nativeHostManifest,
    });
  }
  if (environment.SACCADE_SETUP_DISABLE_CLIENTS !== '1') {
    const expected = expectedMcp(paths.runtime, paths.root);
    for (const client of state.clients || []) {
      if (client === 'codex') {
        const codex = findCodex(environment);
        const current = codex && run(codex, ['mcp', 'get', 'saccade', '--json'], environment);
        let matches = false;
        try { matches = current && current.status === 0 && codexMatches(JSON.parse(current.stdout), expected); } catch {}
        checks.push({ name: 'Codex MCP', ok: Boolean(matches) });
      } else if (client === 'claude-code') {
        const claude = findClaude(environment);
        const current = claude && run(claude, ['mcp', 'get', 'saccade'], environment);
        checks.push({
          name: 'Claude Code MCP',
          ok: Boolean(current && current.status === 0
            && current.stdout.includes(paths.runtime) && current.stdout.includes(paths.root)),
        });
      } else if (client === 'claude-desktop') {
        try {
          const configuration = await readJson(paths.claudeDesktop);
          checks.push({ name: 'Claude Desktop MCP', ok: jsonEqual(configuration.mcpServers && configuration.mcpServers.saccade, expected) });
        } catch { checks.push({ name: 'Claude Desktop MCP', ok: false }); }
      }
    }
  }
  let connected = false;
  const localChecksOk = checks.every((check) => check.ok);
  if (localChecksOk) {
    const result = run(paths.runtime, ['doctor'], { ...environment, SACCADE_RUNTIME_DIR: paths.root });
    try {
      const value = JSON.parse(result.stdout);
      const capabilities = value.capabilities || {};
      connected = result.status === 0
        && value.schema === 'saccade.doctor/1'
        && value.runtime_version === state.version
        && value.mcp_contract_hash === state.mcp_contract_hash
        && value.observation_schema === OBSERVATION_SCHEMA
        && value.host_protocol === HOST_PROTOCOL
        && value.ready === true
        && capabilities.schema === CAPABILITIES_SCHEMA
        && candidateMatches(capabilities.extension_candidate, state.extension_candidate)
        && candidateMatches(capabilities.expected_extension_candidate, state.extension_candidate);
      checks.push({
        name: 'exact Extension → Native Host → Runtime → MCP candidate and contract',
        ok: connected,
        detail: connected ? undefined : value.detail || 'live identity does not match the installed release',
      });
    } catch { checks.push({ name: 'Extension → Native Host → MCP', ok: false, detail: 'Runtime doctor failed' }); }
  }
  const ok = checks.every((check) => check.ok);
  if (print) {
    for (const check of checks) console.log(`${check.ok ? 'OK' : 'FAIL'} ${check.name}${check.detail ? `: ${check.detail}` : ''}`);
    if (!connected && localChecksOk) {
      printExtensionInstructions(state);
    }
  }
  return { ok, connected, checks };
}

async function removeClientConfiguration(paths, state, environment, warnings) {
  const expected = expectedMcp(paths.runtime, paths.root);
  if ((state.clients || []).includes('codex')) {
    const codex = findCodex(environment);
    if (codex) {
      const current = run(codex, ['mcp', 'get', 'saccade', '--json'], environment);
      try {
        if (current.status !== 0 || codexMatches(JSON.parse(current.stdout), expected)) {
          run(codex, ['mcp', 'remove', 'saccade'], environment);
        } else warnings.push('Codex saccade entry changed after setup and was preserved.');
      } catch { warnings.push('Codex saccade entry could not be verified and was preserved.'); }
    } else warnings.push('Codex is unavailable, so its saccade entry was not removed.');
  }
  if ((state.clients || []).includes('claude-code')) {
    const claude = findClaude(environment);
    if (claude) {
      const current = run(claude, ['mcp', 'get', 'saccade'], environment);
      if (current.status !== 0 || (current.stdout.includes(paths.runtime) && current.stdout.includes(paths.root))) {
        run(claude, ['mcp', 'remove', 'saccade', '--scope', 'user'], environment);
      } else warnings.push('Claude Code saccade entry changed after setup and was preserved.');
    } else warnings.push('Claude Code is unavailable, so its saccade entry was not removed.');
  }
  if ((state.clients || []).includes('claude-desktop')) {
    if (fs.existsSync(paths.claudeDesktop)) {
      try {
        const configuration = await readJson(paths.claudeDesktop);
        const current = configuration.mcpServers && configuration.mcpServers.saccade;
        if (current === undefined) {
          // A previous uninstall attempt may already have removed the managed entry.
        } else if (jsonEqual(current, expected)) {
          delete configuration.mcpServers.saccade;
          const mode = (await fsp.stat(paths.claudeDesktop)).mode & 0o777;
          await atomicWrite(paths.claudeDesktop, `${JSON.stringify(configuration, null, 2)}\n`, mode);
        } else warnings.push('Claude Desktop saccade entry changed after setup and was preserved.');
      } catch { warnings.push('Claude Desktop configuration could not be verified and was preserved.'); }
    } else warnings.push('Claude Desktop configuration is unavailable, so its saccade entry was not removed.');
  }
}

async function uninstall(options, environment = process.env) {
  const paths = installPaths(environment);
  let state;
  try { state = await readJson(paths.state); }
  catch {
    if (options.purge && fs.existsSync(paths.root)) {
      await fsp.rm(paths.root, { recursive: true, force: true });
      console.log('Saccade setup state was already removed. The remaining Profile and Runtime data were purged.');
      return true;
    }
    console.log('Saccade setup is not installed.');
    return true;
  }
  const warnings = [];
  if (environment.SACCADE_SETUP_DISABLE_CLIENTS !== '1') {
    await removeClientConfiguration(paths, state, environment, warnings);
  }
  const expectedManifest = nativeManifest(state.native_host, paths.runtime);
  for (const key of state.native_registrations || []) {
    const current = readRegistryValue(key, environment);
    if (current === paths.nativeHostManifest) {
      if (!removeRegistryValue(key, environment)) {
        warnings.push(`Native Host registry value could not be removed: ${key}`);
      }
    } else if (current !== null) {
      warnings.push(`Native Host registry value changed after setup and was preserved: ${key}`);
    }
  }
  for (const target of state.native_manifests || []) {
    try {
      if (jsonEqual(await readJson(target), expectedManifest)) await fsp.rm(target, { force: true });
      else warnings.push(`Native Host manifest changed after setup and was preserved: ${target}`);
    } catch {}
  }
  try {
    const current = await fsp.readFile(paths.runtime);
    if (sha256(current) === state.runtime_sha256) await fsp.rm(paths.runtime, { force: true });
    else warnings.push('Runtime changed after setup and was preserved.');
  } catch {}
  try {
    const current = await readJson(paths.expectedExtensionCandidate);
    if (candidateMatches(current, state.extension_candidate)) {
      await fsp.rm(paths.expectedExtensionCandidate, { force: true });
    } else warnings.push('Expected Extension candidate changed after setup and was preserved.');
  } catch {}
  if (options.purge) {
    try {
      await fsp.rm(paths.root, { recursive: true, force: true });
    } catch (error) {
      await atomicWrite(paths.state, `${JSON.stringify(state, null, 2)}\n`, 0o600);
      if (['EBUSY', 'EPERM', 'ENOTEMPTY'].includes(error.code)) {
        throw new Error('Saccade Runtime is still in use; close Chrome and Edge, then retry uninstall --purge');
      }
      throw error;
    }
  } else if (warnings.length === 0) {
    await fsp.rm(paths.state, { force: true });
  }
  if (warnings.length && !options.purge) {
    console.log('Saccade uninstall is incomplete. Modified files and setup state were preserved.');
  } else {
    console.log(options.purge ? 'Saccade and its Profile were removed.' : 'Saccade was removed. The Profile was preserved.');
  }
  for (const warning of warnings) console.warn(`Warning: ${warning}`);
  return warnings.length === 0 || options.purge;
}

async function main(argv, environment = process.env) {
  const options = parseArguments(argv);
  if (options.help) { printHelp(); return; }
  if (options.command === 'doctor') {
    const result = await doctor(environment, true);
    if (!result.ok) process.exitCode = 1;
  } else if (options.command === 'uninstall') {
    const complete = await uninstall(options, environment);
    if (!complete) process.exitCode = 1;
  }
  else await install(options, environment);
}

module.exports = {
  candidateMatches,
  codexMatches,
  doctor,
  extensionStoreUrl,
  expectedMcp,
  install,
  installPaths,
  main,
  nativeManifest,
  parseArguments,
  platformKey,
  printExtensionInstructions,
  sha256,
  uninstall,
  validateRelease,
};
