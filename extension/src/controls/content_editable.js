(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, descriptor } = common;

  function observe(signals) {
    const state = {
      has_value: bool(signals.hasValue),
      readonly: bool(signals.readonly),
    };
    const affordances = signals.enabled === false || signals.readonly ? [] : ['type'];
    return descriptor('content_editable', affordances, state);
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.content_editable = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
