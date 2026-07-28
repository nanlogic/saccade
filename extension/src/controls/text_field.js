(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    const state = {
      ...commonState(signals),
      has_value: bool(signals.hasValue),
      required: bool(signals.required),
      readonly: bool(signals.readonly),
      invalid: bool(signals.invalid),
    };
    const affordances = signals.protected || signals.enabled === false || signals.readonly ? [] : ['type'];
    return descriptor('text_field', affordances, state, Boolean(signals.protected));
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.text_field = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
