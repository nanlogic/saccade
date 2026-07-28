(() => {
  function bool(value) { return value ? 'true' : 'false'; }

  function commonState(signals) {
    return { enabled: bool(signals.enabled !== false) };
  }

  function descriptor(role, affordances, state, protectedValue = false) {
    return { kind: 'control', role, affordances, state, protected: protectedValue };
  }

  const api = Object.freeze({ bool, commonState, descriptor });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.common = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
