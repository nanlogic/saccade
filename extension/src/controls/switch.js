(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    return descriptor('switch', signals.enabled === false ? [] : ['click'], {
      ...commonState(signals), checked: bool(signals.checked),
    });
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.switch = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
