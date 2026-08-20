(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    const affordances = signals.enabled === false ? [] : [
      ...(signals.expandable ? ['click'] : []), 'select',
    ];
    return descriptor('select', affordances, {
      ...commonState(signals), has_value: bool(signals.hasValue),
      required: bool(signals.required), invalid: bool(signals.invalid), expanded: bool(signals.expanded),
    });
  }

  function option(name, selected, enabled = true, clickable = false) {
    return {
      kind: 'control', role: 'option', name: String(name),
      state: { selected: bool(selected), enabled: bool(enabled) },
      affordances: enabled && clickable ? ['click'] : [], protected: false,
    };
  }

  const api = Object.freeze({ observe, option });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.select = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
