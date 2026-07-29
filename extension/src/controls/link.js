(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState } = common;
  function observe(signals) {
    const state = commonState(signals);
    if (signals.current !== undefined) state.current = String(signals.current);
    if (signals.expanded !== undefined) state.expanded = bool(signals.expanded);
    return { kind: 'link', role: 'link', affordances: signals.enabled === false ? [] : ['click'], state, protected: false };
  }
  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.link = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
