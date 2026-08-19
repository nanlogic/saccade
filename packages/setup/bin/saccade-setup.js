#!/usr/bin/env node

const { main } = require('../src/setup');

main(process.argv.slice(2)).catch((error) => {
  console.error(`Saccade setup failed: ${error.message}`);
  process.exitCode = 1;
});
