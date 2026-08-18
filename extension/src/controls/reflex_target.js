(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { commonState, descriptor } = common;

  function occurrence(authored, pageText = '') {
    const explicit = String(authored || '').trim();
    if (explicit) return explicit.slice(0, 64);
    return String(pageText || '').match(/(?:^|\s)SCORE\s*[:\-–]?\s*(\d+)(?:\s|$)/i)?.[1] || '0';
  }

  function observe(signals) {
    return descriptor(
      'reflex_target',
      signals.enabled === false ? [] : ['click'],
      { ...commonState(signals), reflex_occurrence: String(signals.occurrence ?? '0') },
    );
  }

  const api = Object.freeze({ observe, occurrence });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.reflex_target = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
