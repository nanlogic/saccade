(() => {
  const controls = globalThis.SaccadeControls || {};
  const textField = controls.text_field || require('./text_field.js');

  const modules = new Map([
    ['button', controls.button || require('./button.js')],
    ['text_field', textField],
    ['checkbox', controls.checkbox || require('./checkbox.js')],
    ['select', controls.select || require('./select.js')],
    ['search_field', controls.search_field || require('./search_field.js')],
    ['text_area', controls.text_area || require('./text_area.js')],
    ['content_editable', controls.content_editable || require('./content_editable.js')],
    ['spin_button', controls.spin_button || require('./spin_button.js')],
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
