const assert = require('node:assert/strict');
const test = require('node:test');

const {
  optionValue,
  selectExactExtension,
} = require('../../../scripts/node_release_target');

const candidate = {
  schema: 'saccade.extension-candidate/1',
  id: 'a'.repeat(64),
  version: '0.4.0',
};

test('release target options accept explicit browser forms', () => {
  assert.equal(optionValue(['--browser=edge'], 'browser'), 'edge');
  assert.equal(optionValue(['--browser', 'chrome'], 'browser'), 'chrome');
});

test('release target requires one exact browser and candidate occurrence', () => {
  const chrome = {
    browser_family: 'chrome', browser_instance_id: 'browser.chrome', extension_candidate: candidate,
  };
  const edge = {
    browser_family: 'edge', browser_instance_id: 'browser.edge', extension_candidate: candidate,
  };
  assert.equal(selectExactExtension({ connected_extensions: [chrome, edge] }, {
    browser: 'edge', expectedCandidate: candidate,
  }).browser_instance_id, 'browser.edge');
  assert.throws(() => selectExactExtension({ connected_extensions: [chrome, chrome] }, {
    browser: 'chrome', expectedCandidate: candidate,
  }), /exactly one current chrome Extension/);
  assert.throws(() => selectExactExtension({ connected_extensions: [chrome] }, {
    browser: 'edge', expectedCandidate: candidate,
  }), /found 0/);
});
