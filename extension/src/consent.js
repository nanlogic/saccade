(() => {
  function normalizeOrigin(value) {
    try { return new URL(String(value || '')).origin; }
    catch (_error) { return null; }
  }

  function isSupportedUrl(value) {
    try { return ['http:', 'https:'].includes(new URL(String(value || '')).protocol); }
    catch (_error) { return false; }
  }

  function isProtectedFieldType(type, autocomplete = '') {
    return String(type || '').toLowerCase() === 'password'
      || /(?:^|\s)(?:cc-[^\s]+|one-time-code|current-password|new-password)(?:\s|$)/i.test(String(autocomplete));
  }

  const api = Object.freeze({ normalizeOrigin, isSupportedUrl, isProtectedFieldType });
  globalThis.SaccadeConsent = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
