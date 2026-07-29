(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    return descriptor('tab', signals.enabled === false ? [] : ['click'], {
      ...commonState(signals), selected: bool(signals.selected),
    });
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.tab = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
