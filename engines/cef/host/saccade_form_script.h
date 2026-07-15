// Copyright (c) 2026 Saccade contributors.
// Use of this source code is governed by a BSD-style license.

#ifndef SACCADE_CEF_HOST_SACCADE_FORM_SCRIPT_H_
#define SACCADE_CEF_HOST_SACCADE_FORM_SCRIPT_H_

// This is an allowlisted renderer command surface, not a general JavaScript
// evaluator. Input arrives as JSON and every result is deliberately shaped so
// sensitive values cannot cross the renderer boundary.
constexpr char kSaccadeFormCommandScript[] = R"SACCADE_FORM_JS(
(command, inputJson) => {
  const input = JSON.parse(String(inputJson || '{}'));
  const text = (value, limit = 160) => String(value || '')
      .replace(/\s+/g, ' ').trim().slice(0, limit);
  const controls = () => Array.from(document.querySelectorAll(
      "input,select,textarea,[contenteditable='true'],[role='textbox']"));
  const visible = (el) => {
    if (!el || !el.getBoundingClientRect) return false;
    for (let cur = el; cur && cur.nodeType === 1; cur = cur.parentElement) {
      const style = getComputedStyle(cur);
      if (cur.hidden || cur.getAttribute('aria-hidden') === 'true' ||
          style.display === 'none' || style.visibility === 'hidden' ||
          style.visibility === 'collapse') return false;
    }
    const rect = el.getBoundingClientRect();
    return rect.width > 0 && rect.height > 0;
  };
  const editorBacking = (el) => {
    if (!el || !el.isContentEditable) return null;
    let root = el.parentElement;
    for (let depth = 0; root && root !== document.body && depth < 16;
         depth += 1, root = root.parentElement) {
      const candidates = Array.from(root.querySelectorAll('textarea'))
          .filter(candidate => candidate !== el && !visible(candidate));
      const editors = Array.from(root.querySelectorAll(
          '[contenteditable="true"],[role="textbox"]'))
          .filter(candidate => visible(candidate));
      if (candidates.length === 1 && editors.length === 1 && editors[0] === el) {
        return candidates[0];
      }
    }
    return null;
  };
  const label = (el) => {
    if (el.id) {
      const exact = Array.from(document.querySelectorAll('label'))
          .find(candidate => candidate.htmlFor === el.id);
      if (exact && text(exact.innerText || exact.textContent)) {
        return {text: text(exact.innerText || exact.textContent),
                source: 'label_for', confidence: 1};
      }
    }
    const wrapping = el.closest('label');
    if (wrapping && text(wrapping.innerText || wrapping.textContent)) {
      return {text: text(wrapping.innerText || wrapping.textContent),
              source: 'label_wrap', confidence: 0.95};
    }
    if (el.isContentEditable && el.matches('.cm-content')) {
      const editor = el.closest('.cm-editor');
      const placeholder = editor && editor.querySelector('.cm-placeholder');
      const value = text(placeholder && placeholder.textContent);
      if (value) return {text: value, source: 'editor_placeholder', confidence: 0.85};
    }
    const backing = editorBacking(el);
    const backingPlaceholder = text(backing && backing.getAttribute('placeholder'));
    if (backingPlaceholder) {
      return {text: backingPlaceholder, source: 'editor_backing_placeholder',
              confidence: 0.85};
    }
    const labelledBy = text(el.getAttribute('aria-labelledby'));
    if (labelledBy) {
      const value = labelledBy.split(/\s+/).map(id => {
        const node = document.getElementById(id);
        return node ? text(node.innerText || node.textContent) : '';
      }).filter(Boolean).join(' ');
      if (value) return {text: text(value), source: 'aria_labelledby', confidence: 0.95};
    }
    const aria = text(el.getAttribute('aria-label'));
    if (aria) return {text: aria, source: 'aria_label', confidence: 0.9};
    const placeholder = text(el.getAttribute('placeholder'));
    if (placeholder) return {text: placeholder, source: 'placeholder', confidence: 0.65};
    const cell = el.closest('td,th');
    const row = cell && cell.parentElement;
    const table = row && row.closest('table');
    if (cell && row && table) {
      const cells = Array.from(row.children);
      const column = cells.indexOf(cell);
      const headers = Array.from(table.querySelectorAll('thead th'));
      const rowId = text(cells[0] && (cells[0].innerText || cells[0].textContent));
      const heading = column >= 0 && headers[column]
          ? text(headers[column].innerText || headers[column].textContent) : '';
      if (heading) return {text: text(`${rowId} ${heading}`),
                           source: 'table_header', confidence: 0.9};
    }
    const fallback = text(el.getAttribute('name') || el.id);
    if (fallback) return {text: fallback.replace(/[_-]+/g, ' '),
                          source: 'identifier', confidence: 0.35};
    return {text: '', source: 'missing', confidence: 0};
  };
  const sensitivity = (el, labelText) => {
    const explicit = text(el.getAttribute('data-sensitive')).toLowerCase();
    if (explicit && explicit !== 'none' && explicit !== 'false') return explicit;
    const token = [el.getAttribute('type'), el.getAttribute('autocomplete'),
      el.getAttribute('name'), el.id, el.getAttribute('aria-label'),
      el.getAttribute('placeholder'), labelText].filter(Boolean).join(' ').toLowerCase();
    if (/\b(password|passcode|pin)\b/.test(token)) return 'password';
    if (/otp|one[-_ ]?time|totp|2fa|mfa|verification[-_ ]?code/.test(token)) return 'otp';
    if (/ssn|social security|tax[-_ ]?id|taxpayer|government[-_ ]?id|passport|driver.?s license|\btin\b|\bein\b/.test(token)) return 'government_or_tax_id';
    if (/credit|card number|cc[-_]?number|cc[-_]?csc|cvv|cvc|bank account|routing number|payment/.test(token)) return 'payment';
    if (/signature|attestation|legal[-_ ]?attestation|initials|e[-_ ]?sign|consent/.test(token)) return 'legal_attestation';
    return 'none';
  };
  const typeOf = (el) => {
    const tag = el.tagName.toLowerCase();
    const role = (el.getAttribute('role') || '').toLowerCase();
    return (el.getAttribute('type') ||
      (tag === 'input' ? (el.type || 'text') :
       (el.isContentEditable ? 'contenteditable' :
        (role === 'textbox' ? 'role_textbox' : tag)))).toLowerCase();
  };
  const internalValue = (el, type) => {
    if (type === 'checkbox' || type === 'radio') return Boolean(el.checked);
    if (el.isContentEditable || type === 'role_textbox') {
      const backing = editorBacking(el);
      if (backing) return String(backing.value || '');
      if (el.matches('.cm-content')) {
        const lines = Array.from(el.querySelectorAll(':scope > .cm-line'));
        if (lines.length) {
          return lines.map(line => String(line.textContent || '')).join('\n');
        }
      }
      // innerText follows the rendered surface and ignores screen-reader
      // helpers that commonly live inside rich editors. textContent does not.
      return String(el.innerText || '');
    }
    return String(el.value || '');
  };
  const hasValue = (el, type) => type === 'checkbox' || type === 'radio'
      ? Boolean(el.checked) : String(internalValue(el, type)).trim().length > 0;
  const hash = (value) => {
    let result = 2166136261;
    for (let i = 0; i < value.length; i += 1) {
      result ^= value.charCodeAt(i);
      result = Math.imul(result, 16777619);
    }
    return (result >>> 0).toString(16).padStart(8, '0');
  };
  const inventory = () => {
    const elements = controls();
    const nameCounts = new Map();
    for (const el of elements) {
      const name = el.getAttribute('name');
      if (name) nameCounts.set(name, (nameCounts.get(name) || 0) + 1);
    }
    const fields = elements.map((el, index) => {
      const tag = el.tagName.toLowerCase();
      const type = typeOf(el);
      const fieldLabel = label(el);
      const fieldSensitivity = sensitivity(el, fieldLabel.text);
      const ownerRaw = text(el.getAttribute('data-owner')).toLowerCase();
      const owner = ownerRaw === 'agent' || ownerRaw === 'human' ? ownerRaw : 'unknown';
      const isVisible = visible(el);
      const enabled = !el.disabled && el.getAttribute('aria-disabled') !== 'true';
      const readonly = Boolean(el.readOnly) || el.getAttribute('aria-readonly') === 'true';
      const present = hasValue(el, type);
      const name = el.getAttribute('name') || '';
      const stable = Boolean(el.id) || Boolean(name && nameCounts.get(name) === 1);
      const selectorHint = el.id ? `#${el.id}` :
        (name ? `${tag}[name=${JSON.stringify(name)}]` : `${tag}:nth-field(${index})`);
      const fieldId = el.id ? `id:${el.id}` :
        (stable ? `name:${name}` : `unstable:${index}:${hash(selectorHint)}`);
      const supported = el.isContentEditable || tag === 'select' || tag === 'textarea' ||
        ['text','email','tel','url','number','date','month','week','time',
         'datetime-local','search','checkbox','radio'].includes(type);
      const nativeTypingRequired = Boolean(editorBacking(el));
      const blocked = [];
      if (!stable) blocked.push('unstable_identity');
      if (!isVisible) blocked.push('not_visible');
      if (!enabled) blocked.push('disabled');
      if (readonly) blocked.push('readonly');
      if (!supported) blocked.push('unsupported_type');
      if (fieldSensitivity !== 'none') blocked.push('sensitive_requires_human');
      if (owner === 'human') blocked.push('human_owned');
      if (present) blocked.push('preserve_existing_value');
      if (fieldLabel.confidence < 0.55) blocked.push('ambiguous_label');
      if (nativeTypingRequired && blocked.length === 0) {
        blocked.push('requires_native_typing');
      }
      return {
        element_index: index,
        field_id: fieldId,
        selector_hash: hash(selectorHint),
        tag, type,
        label: fieldSensitivity === 'none' ? fieldLabel.text :
          (fieldLabel.text || 'Sensitive field'),
        label_source: fieldLabel.source,
        label_confidence: fieldLabel.confidence,
        owner, sensitivity: fieldSensitivity,
        required: Boolean(el.required) || el.getAttribute('aria-required') === 'true',
        visible: isVisible, enabled, readonly, stable_identity: stable,
        native_typing_required: nativeTypingRequired,
        native_type_eligible: nativeTypingRequired &&
          blocked.every(reason => reason === 'requires_native_typing'),
        option_count: tag === 'select' ? el.options.length : null,
        value_state: fieldSensitivity === 'none'
          ? (present ? 'present_redacted' : 'empty')
          : (present ? 'completed_without_value' : 'requires_user_input'),
        eligible: blocked.length === 0,
        blocked_reasons: blocked
      };
    });
    const visibleFields = fields.filter(field => field.visible);
    return {
      dom_control_count: fields.length,
      hidden_control_count: fields.length - visibleFields.length,
      field_count: visibleFields.length,
      eligible_count: visibleFields.filter(field => field.eligible).length,
      sensitive_count: fields.filter(field => field.sensitivity !== 'none').length,
      existing_value_count: visibleFields.filter(field =>
        field.value_state === 'present_redacted' ||
        field.value_state === 'completed_without_value').length,
      fields: visibleFields
    };
  };
  const publicInventory = (snapshot) => ({...snapshot,
    fields: snapshot.fields.map(({element_index, ...field}) => field)});
  const focusNativeType = () => {
    const fieldId = String(input.field_id || '');
    const snapshot = inventory();
    const field = snapshot.fields.find(candidate => candidate.field_id === fieldId);
    if (!field || !field.native_type_eligible) {
      return {native_type_ready: false, field_id: fieldId,
        reason: !field ? 'not_found' :
          ((field.blocked_reasons || []).find(reason =>
            reason !== 'requires_native_typing') || 'not_native_type_eligible'),
        values_logged: false};
    }
    const el = controls()[field.element_index];
    el.focus();
    if (typeof document.execCommand === 'function') {
      document.execCommand('selectAll', false, null);
    }
    return {native_type_ready: document.activeElement === el,
      field_id: fieldId, type: field.type,
      before_length: String(internalValue(el, field.type)).length,
      visible_hash_before: hash(String(el.innerText || '').replace(/\u200b/g, '')),
      values_logged: false};
  };
  const verifyNativeType = () => {
    const fieldId = String(input.field_id || '');
    const snapshot = inventory();
    const field = snapshot.fields.find(candidate => candidate.field_id === fieldId);
    if (!field) return {field_id: fieldId, verified: false,
      reason: 'not_found', values_logged: false};
    const el = controls()[field.element_index];
    const value = String(internalValue(el, field.type));
    const visibleHash = hash(String(el.innerText || '').replace(/\u200b/g, ''));
    return {field_id: fieldId,
      verified: value.length === Number(input.expected_length) &&
        hash(value) === String(input.expected_hash || '') &&
        visibleHash !== String(input.visible_hash_before || ''),
      backing_match: value.length === Number(input.expected_length) &&
        hash(value) === String(input.expected_hash || ''),
      visible_changed: visibleHash !== String(input.visible_hash_before || ''),
      value_length: value.length, values_logged: false};
  };
  const compile = (assignments) => {
    const snapshot = inventory();
    const byId = new Map(snapshot.fields.map(field => [field.field_id, field]));
    const eligible = [];
    const rejected = [];
    for (const fieldId of Object.keys(assignments || {}).sort()) {
      const field = byId.get(fieldId);
      if (!field) {
        rejected.push({field_id: fieldId, reason: 'not_found'});
      } else if (!field.eligible) {
        rejected.push({field_id: fieldId,
          reason: field.blocked_reasons[0] || 'not_eligible',
          blocked_reasons: field.blocked_reasons, owner: field.owner,
          sensitivity: field.sensitivity, value_state: field.value_state});
      } else {
        eligible.push({field_id: fieldId, type: field.type,
          owner: field.owner === 'unknown' ? 'explicit_plan' : field.owner,
          required: field.required, option_count: field.option_count});
      }
    }
    return {plan_id: `form_plan_v1_${hash(eligible.map(field => field.field_id).join('|'))}`,
            eligible, rejected};
  };
  const inspect = () => {
    const requested = Array.isArray(input.field_ids) ? new Set(input.field_ids) : null;
    const snapshot = inventory();
    const elements = controls();
    const fields = [];
    snapshot.fields.forEach((field, index) => {
      if (requested && !requested.has(field.field_id)) return;
      const record = {field_id: field.field_id, owner: field.owner,
        sensitivity: field.sensitivity, value_state: field.value_state,
        status: 'ok'};
      const inspectable = field.sensitivity === 'none' && field.visible &&
        !(field.blocked_reasons || []).includes('unsupported_type');
      if (inspectable) {
        record.value = internalValue(elements[field.element_index], field.type);
        record.value_returned = true;
      } else {
        record.value_redacted = true;
      }
      fields.push(record);
    });
    if (requested) {
      for (const fieldId of requested) {
        if (!fields.some(field => field.field_id === fieldId)) {
          fields.push({field_id: fieldId, status: 'not_found'});
        }
      }
    }
    return {fields, sensitive_count: snapshot.sensitive_count,
            values_logged: false};
  };
  const execute = (assignments, expectedPlanId) => {
    const compiled = compile(assignments);
    if (!expectedPlanId || compiled.plan_id !== expectedPlanId) {
      throw new Error('form plan id mismatch; recompile before execution');
    }
    const elements = controls();
    const snapshot = inventory();
    const entries = new Map(snapshot.fields.map(field =>
      [field.field_id, {field, el: elements[field.element_index]}]));
    const filled = [];
    const failed = [];
    const preservedBefore = new Map();
    for (const rejected of compiled.rejected) {
      if (!(rejected.blocked_reasons || []).includes('preserve_existing_value')) continue;
      const entry = entries.get(rejected.field_id);
      if (entry) preservedBefore.set(rejected.field_id,
        internalValue(entry.el, entry.field.type));
    }
    const dispatch = el => {
      el.dispatchEvent(new Event('input', {bubbles: true}));
      el.dispatchEvent(new Event('change', {bubbles: true}));
    };
    let writes = 0;
    for (const planned of compiled.eligible) {
      const entry = entries.get(planned.field_id);
      if (!entry) { failed.push({field_id: planned.field_id, reason: 'field_disappeared'}); continue; }
      const value = assignments[planned.field_id];
      let verified = false;
      let method = 'value_property';
      try {
        if (planned.type === 'checkbox' || planned.type === 'radio') {
          if (typeof value !== 'boolean') {
            failed.push({field_id: planned.field_id, reason: 'boolean_required'}); continue;
          }
          writes += 1; entry.el.checked = value; dispatch(entry.el);
          verified = Boolean(entry.el.checked) === value; method = 'checked_property';
        } else if (planned.type === 'select') {
          const requested = String(value);
          const options = Array.from(entry.el.options || []);
          const optionIndex = options.findIndex(option =>
            String(option.value) === requested || text(option.textContent) === requested);
          if (optionIndex < 0) {
            failed.push({field_id: planned.field_id, reason: 'option_not_found'}); continue;
          }
          writes += 1; entry.el.selectedIndex = optionIndex; dispatch(entry.el);
          verified = entry.el.selectedIndex === optionIndex; method = 'select_option_match';
        } else if (planned.type === 'contenteditable' || planned.type === 'role_textbox') {
          writes += 1;
          entry.el.focus();
          if (typeof document.execCommand === 'function') {
            document.execCommand('selectAll', false, null);
          }
          const inserted = typeof document.execCommand === 'function' &&
            document.execCommand('insertText', false, String(value));
          if (!inserted) {
            entry.el.textContent = String(value);
            dispatch(entry.el);
          } else entry.el.dispatchEvent(new Event('change', {bubbles: true}));
          verified = String(internalValue(entry.el, planned.type)) === String(value);
          method = inserted ? 'contenteditable_insert_text' : 'contenteditable_text';
        } else {
          writes += 1; entry.el.value = String(value); dispatch(entry.el);
          verified = String(entry.el.value || '') === String(value);
        }
      } catch (_) {
        failed.push({field_id: planned.field_id, reason: 'write_exception'}); continue;
      }
      if (verified) filled.push({field_id: planned.field_id,
        status: 'filled_verified', method});
      else failed.push({field_id: planned.field_id, reason: 'postcondition_mismatch'});
    }
    if (writes && document.body && document.body.dataset) {
      const current = Number(document.body.dataset.sessionRevision || '0') || 0;
      document.body.dataset.sessionRevision = String(current + 1);
    }
    const preserved = [];
    for (const [fieldId, before] of preservedBefore.entries()) {
      const entry = entries.get(fieldId);
      if (entry && internalValue(entry.el, entry.field.type) === before) {
        preserved.push({field_id: fieldId, status: 'preserved_verified'});
      } else failed.push({field_id: fieldId, reason: 'existing_value_changed'});
    }
    return {plan_id: compiled.plan_id, filled, preserved,
      rejected: compiled.rejected, failed, write_attempted_count: writes,
      receipt_verified: failed.length === 0 &&
        filled.length === compiled.eligible.length,
      values_logged: false};
  };

  const articleText = () => {
    const root = document.querySelector(
        'article, main, [role="main"]') || document.body;
    let value = String(root ? (root.innerText || root.textContent || '') : '');
    for (const el of controls()) {
      const fieldLabel = label(el);
      if (sensitivity(el, fieldLabel.text) === 'none') continue;
      const protectedValue = String(internalValue(el, typeOf(el)) || '');
      if (protectedValue.length >= 3) {
        value = value.split(protectedValue).join('[redacted]');
      }
    }
    value = value
      .replace(/\b\d{3}-\d{2}-\d{4}\b/g, '[redacted]')
      .replace(/\b(?:\d[ -]*?){13,19}\b/g, '[redacted]')
      .replace(/[ \t]+\n/g, '\n')
      .replace(/\n{3,}/g, '\n\n')
      .trim()
      .slice(0, 250000);
    const headings = Array.from((root || document).querySelectorAll(
        'h1, h2, h3'))
      .map(node => text(node.innerText || node.textContent, 240))
      .filter(Boolean)
      .slice(0, 100);
    return {
      title: text(document.title, 500),
      text: value,
      article_text_length: value.length,
      headings,
      truncated: value.length === 250000,
      sensitive_values_exposed: false,
      values_logged: false
    };
  };

  let result;
  if (command === 'inventory') result = publicInventory(inventory());
  else if (command === 'inspect') result = inspect();
  else if (command === 'compile') result = compile(input.assignments || {});
  else if (command === 'execute') result = execute(
      input.assignments || {}, String(input.expected_plan_id || ''));
  else if (command === 'reveal_more') {
    let changed = 0;
    for (const element of Array.from(document.querySelectorAll('*'))) {
      if (element.scrollHeight <= element.clientHeight + 1) continue;
      const before = element.scrollTop;
      element.scrollTop = element.scrollHeight;
      element.dispatchEvent(new Event('scroll', {bubbles: true}));
      if (element.scrollTop !== before) changed += 1;
    }
    result = {changed_scrollers: changed, values_logged: false};
  }
  else if (command === 'screenshot_policy') {
    const snapshot = inventory();
    const localFixture = location.protocol === 'file:' ||
      (location.protocol === 'http:' &&
       ['127.0.0.1', 'localhost', '::1'].includes(location.hostname));
    result = {capture_allowed: Boolean(input.audit_requested) &&
      snapshot.sensitive_count === 0 && localFixture,
      reason: !input.audit_requested ? 'audit_not_requested' :
        (snapshot.sensitive_count ? 'sensitive_fields_present' :
         (!localFixture ? 'origin_not_allowlisted' : 'allowed')),
      scope: 'local_fixture_only',
      sensitive_count: snapshot.sensitive_count,
      values_logged: false};
  }
  else if (command === 'article_text') result = articleText();
  else if (command === 'focus_native_type') result = focusNativeType();
  else if (command === 'verify_native_type') result = verifyNativeType();
  else throw new Error('unsupported fixed form command');
  return JSON.stringify(result);
}
)SACCADE_FORM_JS";

#endif  // SACCADE_CEF_HOST_SACCADE_FORM_SCRIPT_H_
