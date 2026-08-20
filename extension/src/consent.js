(() => {
  function normalizeOrigin(value) {
    try { return new URL(String(value || '')).origin; }
    catch (_error) { return null; }
  }

  function isSupportedUrl(value) {
    try { return ['http:', 'https:'].includes(new URL(String(value || '')).protocol); }
    catch (_error) { return false; }
  }

  function isProtectedFieldType(type, autocomplete = '', semanticHint = '') {
    const password = String(type || '').toLowerCase() === 'password'
      || /(?:^|\s)(?:current-password|new-password)(?:\s|$)/i.test(String(autocomplete));
    const identity = `${autocomplete} ${semanticHint}`;
    return password
      || /\b(?:ssn|social security(?: number)?)\b/i.test(identity)
      || /\b(?:ein|employer identification(?: number)?|federal tax id(?:entification)?(?: number)?)\b/i.test(identity);
  }

  function redactProtectedText(value) {
    return String(value || '')
      .replace(/\b\d{3}-\d{2}-\d{4}\b/g, '[REDACTED SSN]')
      .replace(/\b\d{2}-\d{7}\b/g, '[REDACTED EIN]');
  }

  const api = Object.freeze({ normalizeOrigin, isSupportedUrl, isProtectedFieldType, redactProtectedText });
  globalThis.SaccadeConsent = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
