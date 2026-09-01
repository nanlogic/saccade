#!/usr/bin/env node
'use strict';

const { createBrokerServer, DEFAULT_PORT } = require('../src/broker');
const { serveMcp } = require('../src/mcp');
const setup = require('../src/setup');

async function main(argv = process.argv.slice(2)) {
  const command = argv[0] || 'install';
  if (command === 'broker') {
    const port = Number(process.env.SACCADE_BROKER_PORT || DEFAULT_PORT);
    const broker = createBrokerServer(undefined, { port });
    await broker.listen();
    if (!argv.includes('--child')) console.error(`Saccade Node Broker listening on 127.0.0.1:${port}`);
    return;
  }
  if (command === 'mcp') return serveMcp();
  if (command === '--version' || command === '-V') {
    console.log('saccade 0.2.1');
    return;
  }
  return setup.main(argv);
}

main().catch((error) => {
  console.error(`Saccade failed: ${error.message}`);
  process.exitCode = 1;
});

module.exports = { main };
