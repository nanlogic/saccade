(() => {
  const { descriptor } = globalThis.SaccadeControls?.common || require('./common.js');
  function observe() { return descriptor('label', [], {}); }
  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.label = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
