(() => {
  const controls = globalThis.SaccadeControls || {};
  const modules = new Map([
    ['button', controls.button || require('./button.js')],
    ['text_field', controls.text_field || require('./text_field.js')],
    ['checkbox', controls.checkbox || require('./checkbox.js')],
    ['select', controls.select || require('./select.js')],
  ]);

  function observe(role, signals) {
    const controlModule = modules.get(role);
    if (!controlModule) throw new Error(`unregistered control role: ${role}`);
    return Object.freeze(controlModule.observe(Object.freeze({ ...signals })));
  }

  function option(name, selected, enabled = true) {
    return Object.freeze(modules.get('select').option(name, selected, enabled));
  }

  const api = Object.freeze({ observe, option });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.registry = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
