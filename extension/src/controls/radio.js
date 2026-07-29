(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    return descriptor('radio', signals.enabled === false ? [] : ['click'], {
      ...commonState(signals), checked: bool(signals.checked),
      required: bool(signals.required), invalid: bool(signals.invalid),
    });
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.radio = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
