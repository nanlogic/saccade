'use strict';

const test = require('node:test');
const assert = require('node:assert/strict');
const { execFileSync } = require('node:child_process');
const path = require('node:path');

test('CLI version follows the installed package manifest', () => {
  const result = execFileSync(process.execPath, [path.join(__dirname, '../bin/saccade.js'), '--version'], { encoding: 'utf8' });
  assert.equal(result.trim(), `saccade ${require('../package.json').version}`);
});
