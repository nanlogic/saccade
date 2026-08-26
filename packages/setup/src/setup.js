'use strict';

const fs = require('node:fs');
const fsp = require('node:fs/promises');
const os = require('node:os');
const path = require('node:path');

const CODEX_BLOCK_START = '# saccade-setup:start';
const CODEX_BLOCK_END = '# saccade-setup:end';
const MCP_COMMAND = 'npx';
const MCP_ARGS = ['-y', '@nanlogic/saccade', 'mcp'];

function parseArguments(argv) {
  const values = [...argv];
  const command = values[0] && !values[0].startsWith('-') ? values.shift() : 'install';
  if (!['install', 'update', 'doctor', 'uninstall'].includes(command)) throw new Error(`unknown command ${command}`);
  let purge = false;
  let help = false;
  for (const flag of values) {
    if (flag === '--purge' && command === 'uninstall') purge = true;
    else if (flag === '--help' || flag === '-h') help = true;
    else throw new Error(`unknown option ${flag}`);
  }
  return { command, purge, help };
}

function installPaths(environment = process.env) {
  const home = path.resolve(environment.SACCADE_SETUP_HOME || os.homedir());
  const root = path.join(home, '.saccade');
  const brokerRoot = path.resolve(environment.SACCADE_STATE_DIR || root);
  return {
    home, root,
    profile: path.join(root, 'profile.json'),
    state: path.join(root, 'setup-state.json'),
    brokerState: path.join(brokerRoot, 'broker-state.json'),
    codexConfig: path.resolve(environment.SACCADE_SETUP_CODEX_CONFIG || path.join(home, '.codex', 'config.toml')),
    claudeConfig: path.resolve(environment.SACCADE_SETUP_CLAUDE_CONFIG || path.join(home, '.claude.json')),
  };
}

function expectedMcp() { return { command: MCP_COMMAND, args: MCP_ARGS }; }

function codexBlock() {
  return `${CODEX_BLOCK_START}\n[mcp_servers.saccade]\ncommand = "${MCP_COMMAND}"\nargs = ["-y", "@nanlogic/saccade", "mcp"]\n${CODEX_BLOCK_END}`;
}

function replaceManagedBlock(source, replacement) {
  const pattern = new RegExp(`${CODEX_BLOCK_START}[\\s\\S]*?${CODEX_BLOCK_END}\\n?`, 'g');
  const clean = source.replace(pattern, '').trimEnd();
  return replacement ? `${clean}${clean ? '\n\n' : ''}${replacement}\n` : `${clean}${clean ? '\n' : ''}`;
}

async function writeJson(target, value) {
  await fsp.mkdir(path.dirname(target), { recursive: true });
  await fsp.writeFile(target, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
}

async function configureCodex(paths) {
  let source = '';
  try { source = await fsp.readFile(paths.codexConfig, 'utf8'); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  await fsp.mkdir(path.dirname(paths.codexConfig), { recursive: true });
  await fsp.writeFile(paths.codexConfig, replaceManagedBlock(source, codexBlock()), { mode: 0o600 });
}

async function configureClaude(paths) {
  let config = {};
  try { config = JSON.parse(await fsp.readFile(paths.claudeConfig, 'utf8')); } catch (error) { if (error.code !== 'ENOENT') throw error; }
  config.mcpServers ||= {};
  config.mcpServers.saccade = expectedMcp();
  await writeJson(paths.claudeConfig, config);
}

async function removeClientConfiguration(paths) {
  try {
    const source = await fsp.readFile(paths.codexConfig, 'utf8');
    await fsp.writeFile(paths.codexConfig, replaceManagedBlock(source, ''), { mode: 0o600 });
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
  try {
    const config = JSON.parse(await fsp.readFile(paths.claudeConfig, 'utf8'));
    if (config.mcpServers?.saccade?.command === MCP_COMMAND) delete config.mcpServers.saccade;
    await writeJson(paths.claudeConfig, config);
  } catch (error) { if (error.code !== 'ENOENT') throw error; }
}

async function install(_options = {}, environment = process.env) {
  const paths = installPaths(environment);
  await fsp.mkdir(paths.root, { recursive: true });
  if (!fs.existsSync(paths.profile)) {
    const profile = JSON.parse(await fsp.readFile(path.join(__dirname, '..', 'default-profile.json'), 'utf8'));
    await writeJson(paths.profile, profile);
  }
  if (environment.SACCADE_SETUP_DISABLE_CLIENTS !== '1') {
    await Promise.all([configureCodex(paths), configureClaude(paths)]);
  }
  await writeJson(paths.state, {
    schema: 'saccade.node-setup/1', version: require('../package.json').version,
    installed_at: new Date().toISOString(), runtime: 'node', mcp: expectedMcp(),
  });
  console.log('Saccade Node Broker is configured. Install the Chrome/Edge Extension, then start a new Agent task.');
  return paths;
}

async function doctor(environment = process.env, print = false) {
  const paths = installPaths(environment);
  let broker;
  let brokerOk = environment.SACCADE_SETUP_SKIP_BROKER === '1';
  if (!brokerOk) {
    try {
      const client = require('./broker_client');
      await client.ensureBroker();
      broker = await client.request('/v1/doctor', { timeoutMs: 1_000 });
      brokerOk = broker.runtime === 'node';
    } catch (error) {
      broker = { error: String(error?.message || error).slice(0, 512) };
    }
  }
  const checks = [
    { name: 'Node.js 18+', ok: Number(process.versions.node.split('.')[0]) >= 18 },
    { name: 'Node-only setup state', ok: fs.existsSync(paths.state) },
    { name: 'Profile', ok: fs.existsSync(paths.profile) },
    { name: 'Loopback Node Broker', ok: brokerOk },
  ];
  const ok = checks.every((check) => check.ok);
  if (print) for (const check of checks) console.log(`${check.ok ? 'OK' : 'FAIL'} ${check.name}`);
  if (print && broker) console.log(`BROKER ${JSON.stringify(broker)}`);
  return { ok, checks, runtime: 'node', broker, native_host: false, platform_driver: false };
}

async function uninstall(options = {}, environment = process.env) {
  const paths = installPaths(environment);
  if (environment.SACCADE_SETUP_DISABLE_CLIENTS !== '1') await removeClientConfiguration(paths);
  await fsp.rm(paths.state, { force: true });
  await fsp.rm(paths.brokerState, { force: true });
  if (options.purge) await fsp.rm(paths.root, { recursive: true, force: true });
  console.log(options.purge ? 'Saccade configuration and Profile were removed.' : 'Saccade configuration was removed. The Profile was preserved.');
  return true;
}

function printHelp() { console.log('Usage: saccade [mcp|broker|install|doctor|update|uninstall] [--purge]'); }

async function main(argv, environment = process.env) {
  const options = parseArguments(argv);
  if (options.help) return printHelp();
  if (options.command === 'doctor') {
    const result = await doctor(environment, true);
    if (!result.ok) process.exitCode = 1;
  } else if (options.command === 'uninstall') await uninstall(options, environment);
  else await install(options, environment);
}

module.exports = { doctor, expectedMcp, install, installPaths, main, parseArguments, uninstall };
