(() => {
  const { commonState, descriptor } = globalThis.SaccadeControls?.common || require('./common.js');
  function observe(signals) {
    return descriptor('slider', signals.enabled === false ? [] : ['drag'], commonState(signals));
  }
  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.slider = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
