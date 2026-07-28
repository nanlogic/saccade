(() => {
  const HOST_PROTOCOL = 'saccade-extension-host/1';
  const OBSERVATION_SCHEMA = 'saccade.observation/1';

  function envelope(kind, payload = {}, requestId) {
    const message = { protocol: HOST_PROTOCOL, kind, payload };
    if (Number.isSafeInteger(requestId) && requestId >= 0) message.request_id = requestId;
    return message;
  }

  function parseHostMessage(message) {
    if (!message || typeof message !== 'object' || Array.isArray(message)) return null;
    if (Object.keys(message).some((key) => !['protocol', 'kind', 'request_id', 'payload'].includes(key))) return null;
    if (message.protocol !== HOST_PROTOCOL || typeof message.kind !== 'string' || !message.kind) return null;
    if (message.request_id !== undefined && (!Number.isSafeInteger(message.request_id) || message.request_id < 0)) return null;
    if (message.payload !== undefined && (!message.payload || typeof message.payload !== 'object' || Array.isArray(message.payload))) return null;
    return { kind: message.kind, requestId: message.request_id, payload: message.payload || {} };
  }

  function randomToken(prefix = 'token') {
    const bytes = new Uint8Array(24);
    crypto.getRandomValues(bytes);
    return `${prefix}.${Array.from(bytes, (byte) => byte.toString(16).padStart(2, '0')).join('')}`;
  }

  const api = Object.freeze({ HOST_PROTOCOL, OBSERVATION_SCHEMA, envelope, parseHostMessage, randomToken });
  globalThis.SaccadeProtocol = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
