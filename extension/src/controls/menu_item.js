(() => {
  const common = globalThis.SaccadeControls?.common || require('./common.js');
  const { bool, commonState, descriptor } = common;

  function observe(signals) {
    return descriptor('menu_item', signals.enabled === false ? [] : ['click'], {
      ...commonState(signals), expanded: bool(signals.expanded),
    });
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.menu_item = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
