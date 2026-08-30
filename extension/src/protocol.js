(() => {
  const BROKER_PROTOCOL = 'saccade.node-broker/1';
  const OBSERVATION_SCHEMA = 'saccade.observation/1';

  function randomToken(prefix = 'token', byteLength = 24) {
    if (!Number.isSafeInteger(byteLength) || byteLength < 16 || byteLength > 32) throw new Error('token entropy length is out of range');
    const bytes = new Uint8Array(byteLength);
    crypto.getRandomValues(bytes);
    return `${prefix}.${Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')}`;
  }

  const api = Object.freeze({ BROKER_PROTOCOL, OBSERVATION_SCHEMA, randomToken });
  globalThis.SaccadeProtocol = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
