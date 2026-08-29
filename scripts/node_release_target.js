'use strict';

const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');

function optionValue(argv, name) {
  const prefix = `--${name}=`;
  const inline = argv.find((value) => value.startsWith(prefix));
  if (inline) return inline.slice(prefix.length);
  const index = argv.indexOf(`--${name}`);
  return index >= 0 ? argv[index + 1] : undefined;
}

function loadExpectedCandidate(candidatePath = path.join(ROOT, 'extension/candidate.json')) {
  const candidate = JSON.parse(fs.readFileSync(path.resolve(candidatePath), 'utf8'));
  if (candidate?.schema !== 'saccade.extension-candidate/1'
    || typeof candidate.id !== 'string' || !/^[a-f0-9]{64}$/.test(candidate.id)
    || typeof candidate.version !== 'string' || !candidate.version) {
    throw new Error('expected Extension candidate is invalid');
  }
  return candidate;
}

function selectExactExtension(capabilities, { browser, expectedCandidate }) {
  if (!['chrome', 'edge'].includes(browser)) {
    throw new Error('--browser must be chrome or edge');
  }
  const attached = Array.isArray(capabilities?.connected_extensions)
    ? capabilities.connected_extensions : [];
  const matches = attached.filter((value) => (
    value.browser_family === browser
      && value.extension_candidate?.schema === expectedCandidate.schema
      && value.extension_candidate?.id === expectedCandidate.id
      && value.extension_candidate?.version === expectedCandidate.version
  ));
  if (matches.length !== 1) {
    const bounded = attached.slice(0, 8).map((value) => ({
      browser_family: value.browser_family,
      browser_instance_id: value.browser_instance_id,
      extension_candidate: value.extension_candidate,
    }));
    throw new Error(`expected exactly one current ${browser} Extension; found ${matches.length}; attached=${JSON.stringify(bounded)}`);
  }
  return matches[0];
}

function releaseTargetFromCapabilities(capabilities, argv = process.argv.slice(2)) {
  const browser = optionValue(argv, 'browser');
  const candidatePath = optionValue(argv, 'candidate');
  const expectedCandidate = loadExpectedCandidate(candidatePath);
  const extension = selectExactExtension(capabilities, { browser, expectedCandidate });
  return {
    browser,
    browser_instance_id: extension.browser_instance_id,
    extension_candidate: expectedCandidate,
  };
}

module.exports = {
  loadExpectedCandidate,
  optionValue,
  releaseTargetFromCapabilities,
  selectExactExtension,
};
