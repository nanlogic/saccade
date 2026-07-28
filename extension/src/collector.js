(() => {
  const registry = globalThis.SaccadeControls.registry;
  const { OBSERVATION_SCHEMA, randomToken } = globalThis.SaccadeProtocol;
  const { isProtectedFieldType } = globalThis.SaccadeConsent;
  const MAX_OBJECTS = 10000;
  const identities = new WeakMap();
  const tokenTargets = new Map();
  const objectTargets = new Map();
  const observers = [];
  const documentId = randomToken('document');
  let objectSerial = 0;
  let revision = 0;
  let viewportRevision = 0;
  let config = null;
  let scheduled = false;

  function normalizedText(value, limit) {
    const text = String(value || '').replace(/\s+/g, ' ').trim();
    return text ? text.slice(0, limit) : undefined;
  }

  function referencedText(element, attribute, limit) {
    const ids = String(element.getAttribute(attribute) || '').split(/\s+/).filter(Boolean);
    return normalizedText(ids.map((id) => {
      const node = element.ownerDocument.getElementById(id);
      if (!node || node.getClientRects().length === 0) return '';
      const style = getComputedStyle(node);
      return style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 ? '' : node.innerText || '';
    }).join(' '), limit);
  }

  function safeName(element, role) {
    const aria = normalizedText(element.getAttribute('aria-label'), 512);
    if (aria) return aria;
    const labelled = referencedText(element, 'aria-labelledby', 512);
    if (labelled) return labelled;
    const labels = normalizedText(Array.from(element.labels || [], (label) => {
      if (!label.getClientRects().length) return '';
      const copy = label.cloneNode(true);
      for (const nested of copy.querySelectorAll('button,input,select,textarea,option')) nested.remove();
      return copy.innerText || copy.textContent || '';
    }).join(' '), 512);
    if (labels) return labels;
    if (role === 'button' || role === 'option') {
      const visible = normalizedText(element.innerText || element.textContent, 512);
      if (visible) return visible;
    }
    return normalizedText(element.getAttribute('title'), 512);
  }

  function safeDescription(element, name, protectedField) {
    if (protectedField) return undefined;
    const described = referencedText(element, 'aria-describedby', 1024);
    if (described) return described;
    const placeholder = normalizedText(element.getAttribute('placeholder'), 1024);
    if (placeholder && placeholder !== name) return placeholder;
    const title = normalizedText(element.getAttribute('title'), 1024);
    return title && title !== name ? title : undefined;
  }

  function roleFor(element) {
    const tag = element.tagName;
    const type = String(element.type || '').toLowerCase();
    const ariaRole = String(element.getAttribute('role') || '').toLowerCase();
    if (tag === 'BUTTON' || ariaRole === 'button' || (tag === 'INPUT' && ['button', 'submit', 'reset'].includes(type))) return 'button';
    if (tag === 'INPUT' && type === 'checkbox' || ariaRole === 'checkbox') return 'checkbox';
    if (tag === 'SELECT') return 'select';
    if (tag === 'INPUT' && ['text', 'email', 'tel', 'url', 'password'].includes(type)) return 'text_field';
    return null;
  }

  function objectId(element) {
    if (!identities.has(element)) identities.set(element, `object.${documentId}.${++objectSerial}`);
    return identities.get(element);
  }

  function boxFor(element) {
    const box = element.getBoundingClientRect();
    return { x: box.x, y: box.y, width: Math.max(0, box.width), height: Math.max(0, box.height) };
  }

  function visibilityFor(element, box) {
    const style = getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || box.width <= 0 || box.height <= 0) return 'hidden';
    if (box.x + box.width <= 0 || box.y + box.height <= 0 || box.x >= innerWidth || box.y >= innerHeight) return 'offscreen';
    return 'visible';
  }

  function ariaBoolean(element, name) {
    const value = element.getAttribute(`aria-${name}`);
    return value === null ? undefined : value === 'true';
  }

  function signalsFor(element, role) {
    const signals = { enabled: !element.disabled && element.getAttribute('aria-disabled') !== 'true' };
    if (role === 'button') {
      signals.pressed = ariaBoolean(element, 'pressed');
      signals.expanded = ariaBoolean(element, 'expanded');
    } else if (role === 'checkbox') {
      signals.checked = element.checked ?? ariaBoolean(element, 'checked');
      signals.required = Boolean(element.required);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
    } else if (role === 'select') {
      signals.hasValue = element.selectedIndex >= 0;
      signals.required = Boolean(element.required);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
      signals.expanded = ariaBoolean(element, 'expanded') || false;
    } else {
      signals.hasValue = Boolean(element.value);
      signals.required = Boolean(element.required);
      signals.readonly = Boolean(element.readOnly);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
      signals.protected = isProtectedFieldType(element.type, element.autocomplete);
    }
    return signals;
  }

  function observationObject(element, role, frameId) {
    const descriptor = registry.observe(role, signalsFor(element, role));
    const box = boxFor(element);
    const visibility = visibilityFor(element, box);
    if (visibility === 'hidden') return null;
    const id = objectId(element);
    objectTargets.set(id, element);
    const name = safeName(element, role);
    const description = safeDescription(element, name, descriptor.protected);
    const object = {
      object_id: id, object_revision: revision + 1, frame_id: frameId,
      ...descriptor,
      document_bounds: { x: box.x + scrollX, y: box.y + scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: 'none',
    };
    if (name) object.name = name;
    if (description) object.description = description;
    if (descriptor.affordances.length && getComputedStyle(element).pointerEvents !== 'none') {
      const token = randomToken('action');
      object.action_token = token;
      tokenTargets.set(token, { element, role, objectId: id, affordances: descriptor.affordances });
    }
    return object;
  }

  function optionObject(option, frameId) {
    const name = safeName(option, 'option');
    let owner = option.parentElement;
    while (owner && owner.tagName !== 'SELECT') owner = owner.parentElement;
    const descriptor = { ...registry.option(name || '', option.selected, !option.disabled && !owner.disabled) };
    if (!name) delete descriptor.name;
    const box = boxFor(owner);
    const id = objectId(option);
    objectTargets.set(id, option);
    return {
      object_id: id, object_revision: revision + 1, frame_id: frameId, ...descriptor,
      document_bounds: { x: box.x + scrollX, y: box.y + scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility: visibilityFor(owner, box), transition: 'none',
    };
  }

  function collect() {
    if (!config) return null;
    tokenTargets.clear();
    objectTargets.clear();
    const objects = [];
    let truncated = false;
    for (const element of document.querySelectorAll('button,input,select,[role="button"],[role="checkbox"]')) {
      const role = roleFor(element);
      if (!role) continue;
      const object = observationObject(element, role, config.frameId);
      if (object) objects.push(object);
      if (role === 'select' && object) {
        for (const option of element.options) objects.push(optionObject(option, config.frameId));
      }
      if (objects.length >= MAX_OBJECTS) { objects.length = MAX_OBJECTS; truncated = true; break; }
    }
    revision += 1;
    viewportRevision += 1;
    const snapshot = {
      schema: OBSERVATION_SCHEMA, browser_instance_id: config.browserInstanceId,
      tab_id: config.tabId, document_id: documentId, revision, viewport_revision: viewportRevision,
      frames: [{ frame_id: config.frameId, document_id: documentId, origin: location.origin, status: 'observed' }],
      objects, changes: [], coverage: { source: 'dom_extension', observed_frame_count: 1, restricted_frame_count: 0, truncated },
      limitations: truncated ? [{ kind: 'truncated', frame_id: config.frameId }] : [], gap: false,
    };
    chrome.runtime.sendMessage({ kind: 'collector.observation', payload: snapshot });
    return snapshot;
  }

  function isTopmost(element, box) {
    const hit = document.elementFromPoint(box.x + box.width / 2, box.y + box.height / 2);
    return hit === element || element.contains(hit);
  }

  function prepare(request) {
    if (!config || request.browser_instance_id !== config.browserInstanceId || request.tab_id !== config.tabId
      || request.document_id !== documentId || request.basis_revision !== revision) throw new Error('stale action basis');
    const target = tokenTargets.get(request.action_token);
    if (!target || !target.element.isConnected || !target.affordances.includes(request.operation)) throw new Error('action token is not current for operation');
    target.element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    const box = boxFor(target.element);
    const prepared = {
      browser_instance_id: config.browserInstanceId, tab_id: config.tabId, document_id: documentId,
      basis_revision: revision, viewport_revision: viewportRevision, object_id: target.objectId,
      action_token: request.action_token, operation: request.operation,
      screen_bounds: { x: screenX + box.x, y: screenY + Math.max(0, outerHeight - innerHeight) + box.y, width: box.width, height: box.height },
      visible: visibilityFor(target.element, box) === 'visible', topmost: isTopmost(target.element, box),
      focus_verified: document.hasFocus() && window.top === window.self,
    };
    if (request.operation === 'select') {
      const optionId = request.payload?.kind === 'select' ? request.payload.option_object_id : '';
      const option = objectTargets.get(optionId);
      let owner = option?.parentElement;
      while (owner && owner.tagName !== 'SELECT') owner = owner.parentElement;
      if (!option || option.tagName !== 'OPTION' || owner !== target.element) throw new Error('select option is not bound to this control');
      prepared.selection_index = Array.from(target.element.options).indexOf(option);
    }
    return prepared;
  }

  function schedule() {
    if (scheduled || !config) return;
    scheduled = true;
    requestAnimationFrame(() => { scheduled = false; collect(); });
  }

  function configure(next) {
    config = next;
    for (const observer of observers.splice(0)) observer.disconnect();
    const observer = new MutationObserver(schedule);
    observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
    observers.push(observer);
    for (const event of ['input', 'change', 'focusin', 'focusout']) document.addEventListener(event, schedule, true);
    addEventListener('scroll', schedule, { passive: true });
    addEventListener('resize', schedule, { passive: true });
    return collect();
  }

  chrome.runtime.onMessage.addListener((message, _sender, respond) => {
    try {
      if (message.kind === 'collector.ping') respond({ ok: true });
      else if (message.kind === 'collector.configure') { configure(message.config); respond({ ok: true, document_id: documentId }); }
      else if (message.kind === 'collector.observe') { collect(); respond({ ok: true }); }
      else if (message.kind === 'collector.prepare_action') respond({ ok: true, prepared: prepare(message.request) });
      else return false;
    } catch (error) { respond({ ok: false, error: String(error.message || error) }); }
    return true;
  });
})();
