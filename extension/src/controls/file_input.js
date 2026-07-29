(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;
  function observe(signals) {
    const state = { ...commonState(signals), has_value: bool(signals.hasValue), required: bool(signals.required) };
    return descriptor('file_input', signals.enabled === false ? [] : ['upload'], state);
  }
  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.file_input = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
