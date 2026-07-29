(() => {
  const textField = globalThis.SaccadeControls?.text_field || require('./text_field.js');

  function observe(signals) {
    const observed = textField.observe(signals);
    return Object.freeze({ ...observed, role: 'spin_button' });
  }

  const api = Object.freeze({ observe });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.spin_button = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
