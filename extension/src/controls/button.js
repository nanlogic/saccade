(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    const state = commonState(signals);
    if (signals.pressed !== undefined) state.pressed = bool(signals.pressed);
    if (signals.expanded !== undefined) state.expanded = bool(signals.expanded);
    return descriptor('button', signals.enabled === false ? [] : ['click'], state);
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.button = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
