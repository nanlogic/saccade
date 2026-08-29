#!/usr/bin/env node
'use strict';

const crypto = require('node:crypto');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { spawn, spawnSync } = require('node:child_process');

const { closeSession, createSession, rpc } = require('../packages/setup/src/broker_client');
const {
  loadExpectedCandidate,
  optionValue,
  selectExactExtension,
} = require('./node_release_target');

const ROOT = path.resolve(__dirname, '..');
const MANIFEST_PATH = path.join(ROOT, 'conformance/release-0.2.0.json');

function parseBrowsers(value = 'chrome,edge') {
  const browsers = value.split(',').map((item) => item.trim()).filter(Boolean);
  if (browsers.length !== 2 || new Set(browsers).size !== 2
    || !browsers.includes('chrome') || !browsers.includes('edge')) {
    throw new Error('release gate requires exactly --browsers=chrome,edge');
  }
  return browsers;
}

function boundedOutput(value, limit = 2_000) {
  const text = String(value || '').replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, '');
  return text.length <= limit ? text : text.slice(text.length - limit);
}

function testFiles(directory) {
  return fs.readdirSync(path.join(ROOT, directory))
    .filter((name) => name.endsWith('.test.js'))
    .sort()
    .map((name) => path.join(directory, name));
}

function runCommand(id, command, args, { env, parseJson = false } = {}) {
  const started = performance.now();
  const result = spawnSync(command, args, {
    cwd: ROOT,
    encoding: 'utf8',
    env: { ...process.env, ...(env || {}) },
    maxBuffer: 8 * 1024 * 1024,
  });
  const value = {
    id,
    passed: result.status === 0,
    exit_code: result.status,
    elapsed_ms: Math.round((performance.now() - started) * 1000) / 1000,
  };
  if (result.error) value.error = boundedOutput(result.error.message);
  if (parseJson && result.status === 0) {
    try { value.result = JSON.parse(result.stdout); }
    catch (_error) {
      value.passed = false;
      value.error = 'command did not return valid JSON';
      value.stdout_tail = boundedOutput(result.stdout);
    }
  } else if (result.status !== 0) {
    value.stdout_tail = boundedOutput(result.stdout);
    value.stderr_tail = boundedOutput(result.stderr);
  }
  return value;
}

function sha256File(filePath) {
  return crypto.createHash('sha256').update(fs.readFileSync(filePath)).digest('hex');
}

async function fixtureReachable(baseUrl) {
  try {
    const response = await fetch(`${baseUrl}/fixtures/controls/file_input.html`, {
      signal: AbortSignal.timeout(500),
    });
    return response.ok;
  } catch (_error) {
    return false;
  }
}

async function ensureFixtureServer(baseUrl) {
  if (await fixtureReachable(baseUrl)) return null;
  const url = new URL(baseUrl);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1'
    || url.pathname !== '/' || !url.port) {
    throw new Error('fixture base URL is unavailable and is not a startable 127.0.0.1 origin');
  }
  const child = spawn('python3', [
    'scripts/fixture_server.py', '--bind', '127.0.0.1', '--port', url.port,
    '--directory', ROOT,
  ], { cwd: ROOT, stdio: 'ignore' });
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (child.exitCode !== null) break;
    await new Promise((resolve) => setTimeout(resolve, 50));
    if (await fixtureReachable(baseUrl)) return child;
  }
  child.kill('SIGTERM');
  throw new Error('fixture server did not become ready');
}

function staticGates() {
  const outputDirectory = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-release-gate-'));
  const npmCache = fs.mkdtempSync(path.join(os.tmpdir(), 'saccade-npm-cache-'));
  const checks = [
    runCommand('setup-tests', process.execPath, [
      '--test', ...testFiles('packages/setup/test'),
    ]),
    runCommand('extension-tests', process.execPath, [
      '--test', ...testFiles('extension/tests'),
    ]),
    runCommand('single-architecture', 'python3', ['scripts/check_single_architecture.py']),
    runCommand('diff-check', 'git', ['diff', '--check']),
    runCommand('extension-package', 'python3', [
      'scripts/package_extension_release.py', '--extension-root', 'extension', '--output', outputDirectory,
    ]),
    runCommand('npm-pack-dry-run', 'npm', ['pack', './packages/setup', '--dry-run', '--json'], {
      env: { npm_config_cache: npmCache }, parseJson: true,
    }),
  ];
  const packageCheck = checks.find((check) => check.id === 'extension-package');
  if (packageCheck?.passed) {
    const zip = fs.readdirSync(outputDirectory).find((name) => name.endsWith('.zip'));
    if (!zip) {
      packageCheck.passed = false;
      packageCheck.error = 'Extension packager produced no ZIP';
    } else {
      const zipPath = path.join(outputDirectory, zip);
      packageCheck.artifact = {
        filename: zip,
        size_bytes: fs.statSync(zipPath).size,
        sha256: sha256File(zipPath),
      };
    }
  }
  return checks;
}

async function connectedTargets(browsers, expectedCandidate) {
  const session = await createSession();
  try {
    const capabilities = await rpc(session, 'system.capabilities', {}, 5_000, 'release-capabilities');
    return browsers.map((browser) => {
      const extension = selectExactExtension(capabilities, { browser, expectedCandidate });
      return {
        browser_family: browser,
        browser_instance_id: extension.browser_instance_id,
        extension_candidate: extension.extension_candidate,
      };
    });
  } finally {
    await closeSession(session);
  }
}

function liveGate(browser, baseUrl, uploadFile, includePublic) {
  const checks = [
    runCommand(`${browser}-deterministic`, process.execPath, [
      'scripts/probe_node_release_smoke.js', `--base-url=${baseUrl}`, `--browser=${browser}`,
    ], { parseJson: true }),
    runCommand(`${browser}-standard-upload`, process.execPath, [
      'scripts/probe_node_upload.js', `${baseUrl}/fixtures/controls/file_input.html`, uploadFile, browser,
    ], { parseJson: true }),
    runCommand(`${browser}-session-isolation`, process.execPath, [
      'scripts/probe_node_session_isolation.js', `--base-url=${baseUrl}`, `--browser=${browser}`,
    ], { parseJson: true }),
  ];
  for (const check of checks) check.blocking = true;
  if (includePublic) {
    const publicForms = runCommand(`${browser}-public-forms`, process.execPath, [
      'scripts/probe_node_public_forms.js', `--browser=${browser}`,
    ], { parseJson: true });
    publicForms.blocking = false;
    checks.push(publicForms);
    const publicTruth = runCommand(`${browser}-public-truth`, process.execPath, [
      'scripts/probe_node_public_truth.js', `--browser=${browser}`,
    ], { parseJson: true });
    publicTruth.blocking = false;
    checks.push(publicTruth);
  }
  return checks;
}

function releasePassed(staticChecks, liveChecks) {
  return staticChecks.every((check) => check.passed)
    && liveChecks.every((check) => check.blocking === false || check.passed);
}

async function main(argv = process.argv.slice(2)) {
  const expectedCandidate = loadExpectedCandidate(optionValue(argv, 'candidate'));
  const manifest = JSON.parse(fs.readFileSync(MANIFEST_PATH, 'utf8'));
  const browsers = parseBrowsers(optionValue(argv, 'browsers'));
  const baseUrl = (optionValue(argv, 'base-url') || 'http://127.0.0.1:8765').replace(/\/$/, '');
  const uploadFile = path.resolve(optionValue(argv, 'upload-file') || 'extension/icons/icon-128.png');
  const includePublic = argv.includes('--include-public');
  const outputPath = optionValue(argv, 'output');
  const report = {
    schema: 'saccade.release-candidate-gate/1',
    release: manifest.release,
    generated_at: new Date().toISOString(),
    node: process.version,
    extension_candidate: expectedCandidate,
    browsers,
    include_public: includePublic,
    static: staticGates(),
    connected_targets: [],
    live: [],
  };
  let fixtureProcess = null;
  try {
    if (!report.static.every((check) => check.passed)) {
      throw new Error('static release gates failed; live browser gates were not dispatched');
    }
    fixtureProcess = await ensureFixtureServer(baseUrl);
    report.fixture_server = fixtureProcess ? 'started_by_gate' : 'already_running';
    report.connected_targets = await connectedTargets(browsers, expectedCandidate);
    for (const browser of browsers) {
      report.live.push(...liveGate(browser, baseUrl, uploadFile, includePublic));
    }
    report.passed = releasePassed(report.static, report.live);
  } catch (error) {
    report.passed = false;
    report.error = boundedOutput(error.message);
  } finally {
    if (fixtureProcess && fixtureProcess.exitCode === null) fixtureProcess.kill('SIGTERM');
  }
  const serialized = `${JSON.stringify(report, null, 2)}\n`;
  if (outputPath) fs.writeFileSync(path.resolve(outputPath), serialized, { mode: 0o600 });
  process.stdout.write(serialized);
  if (!report.passed) process.exitCode = 1;
  return report;
}

if (require.main === module) main().catch((error) => {
  process.stderr.write(`${JSON.stringify({ passed: false, error: boundedOutput(error.message) })}\n`);
  process.exitCode = 1;
});

module.exports = {
  boundedOutput, ensureFixtureServer, parseBrowsers, releasePassed, runCommand,
};
