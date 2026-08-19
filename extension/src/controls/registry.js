(() => {
  const nodeModules = typeof module !== 'undefined' && module.exports ? new Map([
    ['button', require('./button.js')],
    ['link', require('./link.js')],
    ['text_field', require('./text_field.js')],
    ['checkbox', require('./checkbox.js')],
    ['radio', require('./radio.js')],
    ['switch', require('./switch.js')],
    ['select', require('./select.js')],
    ['tab', require('./tab.js')],
    ['menu_item', require('./menu_item.js')],
    ['search_field', require('./search_field.js')],
    ['text_area', require('./text_area.js')],
    ['content_editable', require('./content_editable.js')],
    ['spin_button', require('./spin_button.js')],
    ['reflex_target', require('./reflex_target.js')],
    ['file_input', require('./file_input.js')],
    ['slider', require('./slider.js')],
    ['label', require('./label.js')],
    ['generic_control', require('./generic_control.js')],
  ]) : new Map();

  function moduleFor(role) {
    const controlModule = globalThis.SaccadeControls?.[role] || nodeModules.get(role);
    if (!controlModule) {
      const loaded = Object.keys(globalThis.SaccadeControls || {}).sort().join(',');
      throw new Error(`unregistered control role: ${role}; loaded: ${loaded}`);
    }
    return controlModule;
  }

  function observe(role, signals) {
    const controlModule = moduleFor(role);
    return Object.freeze(controlModule.observe(Object.freeze({ ...signals })));
  }

  function option(name, selected, enabled = true, clickable = false) {
    return Object.freeze(moduleFor('select').option(name, selected, enabled, clickable));
  }

  const api = Object.freeze({ observe, option });
  globalThis.SaccadeControls = globalThis.SaccadeControls || {};
  globalThis.SaccadeControls.registry = api;
  if (typeof module !== 'undefined' && module.exports) module.exports = api;
})();
