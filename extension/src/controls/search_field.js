(() => {
  const textField = globalThis.SaccadeControls?.text_field || require('./text_field.js');

  function observe(signals) {
    const observed = textField.observe(signals);
    return Object.freeze({ ...observed, role: 'search_field' });
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.search_field = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
