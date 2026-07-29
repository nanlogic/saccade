(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { commonState, descriptor } = common;

  function observe(signals) {
    return descriptor(
      'reflex_target',
      signals.enabled === false ? [] : ['click'],
      { ...commonState(signals), reflex_occurrence: String(signals.occurrence ?? '0') },
    );
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.reflex_target = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
