(() => {
  const { commonState, descriptor } = globalThis.SaccadeControls?.common || require('./common.js');
  const ALLOWED = new Set(['click', 'hover', 'focus', 'scroll', 'drag']);
  function observe(signals) {
    const requested = String(signals.affordance || '');
    const affordances = signals.enabled === false || !ALLOWED.has(requested) ? [] : [requested];
    return descriptor('generic_control', affordances, commonState(signals));
  }
  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.generic_control = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
