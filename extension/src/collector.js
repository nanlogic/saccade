(() => {
  const registry = globalThis.SaccadeControls.registry;
  const { OBSERVATION_SCHEMA, randomToken } = globalThis.SaccadeProtocol;
  const { isProtectedFieldType } = globalThis.SaccadeConsent;
  const MAX_OBJECTS = 10000;
  const CONTROL_SELECTOR = 'a[href],button,input,textarea,select,[role="button"],[role="checkbox"],[role="radio"],[role="switch"],[role="tab"],[role="menuitem"],[role="textbox"],[contenteditable],[data-saccade-reflex-target],.target';
  const IMAGE_SELECTOR = 'img[alt],img[aria-label],img[data-saccade-image-identity],svg[aria-label],svg[data-saccade-image-identity]';
  const OBSERVED_SELECTOR = `${CONTROL_SELECTOR},${IMAGE_SELECTOR}`;
  const identities = new WeakMap();
  const tokenTargets = new Map();
  const objectTargets = new Map();
  const fileTriggerHasValue = new WeakSet();
  const observers = [];
  const documentId = randomToken('document');
  const reflexLoopClassToken = randomToken('loop');
  let objectSerial = 0;
  let revision = 0;
  let viewportRevision = 0;
  let config = null;
  let scheduled = false;
  let activeFileTrigger = null;
  let repeatedActionKeys = new Set();

  function normalizedText(value, limit) {
    const text = String(value || '').replace(/\s+/g, ' ').trim();
    return text ? text.slice(0, limit) : undefined;
  }

  function accessibleFallbackText(element, limit) {
    const chunks = [];
    const visit = (node) => {
      if (node.nodeType === Node.TEXT_NODE) {
        chunks.push(node.nodeValue || '');
        return;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) return;
      if (node !== element) {
        if (node.getAttribute('aria-hidden') === 'true') return;
        const style = getComputedStyle(node);
        if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return;
      }
      for (const child of node.childNodes) visit(child);
    };
    visit(element);
    return normalizedText(chunks.join(' '), limit);
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
    if (role === 'button' && location.hostname === 'mouseaccuracy.com' && location.pathname === '/') {
      let row = element.parentElement;
      for (let depth = 0; row && depth < 5; depth += 1, row = row.parentElement) {
        const buttons = Array.from(row.querySelectorAll(':scope button'));
        const rowText = normalizedText(row.innerText, 160);
        if (buttons.length === 2 && rowText && rowText.length <= 40
          && buttons.every((button) => button.getBoundingClientRect().width === 40)) {
          const index = buttons.indexOf(element);
          if (index === 0 || index === 1) return `${index === 0 ? 'Decrease' : 'Increase'} ${rowText}`;
        }
      }
    }
    if (role === 'content_editable') {
      const labelled = referencedText(element, 'aria-labelledby', 512);
      if (labelled) return labelled;
    }
    if (role === 'content_editable') return normalizedText(element.getAttribute('title'), 512);
    if (role === 'image') {
      const alt = normalizedText(element.getAttribute('alt'), 512);
      if (alt) return alt;
    }
    const labelled = referencedText(element, 'aria-labelledby', 512);
    if (labelled) return labelled;
    const labels = normalizedText(Array.from(element.labels || [], (label) => {
      if (!label.getClientRects().length) return '';
      const copy = label.cloneNode(true);
      for (const nested of copy.querySelectorAll('button,input,select,textarea,option')) nested.remove();
      return copy.innerText || copy.textContent || '';
    }).join(' '), 512);
    if (labels) return labels;
    if (['button', 'option', 'link', 'radio', 'switch', 'tab', 'menu_item'].includes(role)
      || (role === 'file_input' && element.tagName !== 'INPUT')) {
      const visible = accessibleFallbackText(element, 512);
      if (visible) return visible;
    }
    return normalizedText(element.getAttribute('title'), 512);
  }

  function repeatedActionContext(element, role, name) {
    if (!name || !['button', 'link'].includes(role)) return undefined;
    if (!repeatedActionKeys.has(`${role}\0${name}`)) return undefined;

    let group = element.parentElement;
    for (let depth = 0; group && group !== document.body && depth < 6; depth += 1, group = group.parentElement) {
      const copy = group.cloneNode(true);
      for (const nested of copy.querySelectorAll('button,input,select,textarea,[contenteditable]')) nested.remove();
      for (const link of copy.querySelectorAll('a[href]')) {
        if (/^(change display name|move up|move down)$/i.test(normalizedText(link.textContent, 64) || '')) link.remove();
      }
      const context = normalizedText(copy.textContent, 256);
      if (context && /[\p{L}\p{N}]/u.test(context)
        && !name.toLocaleLowerCase().includes(context.toLocaleLowerCase())) return context;
    }
    return undefined;
  }

  function safeDescription(element, name, protectedField) {
    if (protectedField || roleFor(element) === 'file_input') return undefined;
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
    const applicationBridge = element.hasAttribute('data-saccade-reflex-target');
    const mouseAccuracyBridge = location.hostname === 'mouseaccuracy.com'
      && location.pathname.startsWith('/game')
      && element.classList.contains('target')
      && !element.classList.contains('hit');
    if (applicationBridge || mouseAccuracyBridge) return 'reflex_target';
    if (ariaRole === 'menuitem') return 'menu_item';
    if (tag === 'A' && element.hasAttribute('href')) return 'link';
    if (tag === 'INPUT' && type === 'file') return 'file_input';
    if ((tag === 'INPUT' && type === 'radio') || ariaRole === 'radio') return 'radio';
    if (ariaRole === 'switch') return 'switch';
    if (ariaRole === 'tab') return 'tab';
    const buttonLike = tag === 'BUTTON' || ariaRole === 'button' || (tag === 'INPUT' && ['button', 'submit', 'reset'].includes(type));
    if (buttonLike && /\b(upload|choose|select|browse|attach|replace|add)\b.*\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b/i.test(safeName(element, 'button') || '')) return 'file_input';
    if (buttonLike) return 'button';
    if (tag === 'INPUT' && type === 'checkbox' || ariaRole === 'checkbox') return 'checkbox';
    if (tag === 'SELECT') return 'select';
    if (tag === 'INPUT' && type === 'number') return 'spin_button';
    if (tag === 'INPUT' && type === 'search') return 'search_field';
    if (tag === 'TEXTAREA') return 'text_area';
    if (tag === 'INPUT' && ariaRole === 'searchbox') return 'search_field';
    if (tag === 'INPUT' && ['text', 'email', 'tel', 'url', 'password'].includes(type)) return 'text_field';
    if (ariaRole === 'textbox' && element.isContentEditable) return 'content_editable';
    if (element.isContentEditable) return 'content_editable';
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

  function visibleFileTrigger(input) {
    const visible = (element) => visibilityFor(element, boxFor(element)) === 'visible';
    const labelled = Array.from(input.labels || []).find(visible);
    if (labelled) return labelled;
    if (input.id) {
      const controlled = Array.from(document.querySelectorAll('[aria-controls]')).find((element) => (
        String(element.getAttribute('aria-controls') || '').split(/\s+/).includes(input.id)
          && element.matches('button,[role="button"],label') && visible(element)
      ));
      if (controlled) return controlled;
    }
    let ancestor = input.parentElement;
    for (let depth = 0; ancestor && ancestor !== document.body && depth < 5; depth += 1, ancestor = ancestor.parentElement) {
      if (ancestor.querySelectorAll('input[type="file"]').length !== 1) continue;
      const candidates = Array.from(ancestor.querySelectorAll('button,[role="button"],label')).filter(visible);
      if (candidates.length === 1) return candidates[0];
    }
    return input;
  }

  function ariaBoolean(element, name) {
    const value = element.getAttribute(`aria-${name}`);
    return value === null ? undefined : value === 'true';
  }

  function signalsFor(element, role) {
    const signals = { enabled: !element.disabled && element.getAttribute('aria-disabled') !== 'true' };
    if (role === 'reflex_target') {
      if (element === document.body && location.hostname === 'mouseaccuracy.com') signals.enabled = false;
      const authored = element.getAttribute('data-saccade-reflex-occurrence');
      const score = location.hostname === 'mouseaccuracy.com'
        ? (document.body?.innerText || '').match(/SCORE\s*(\d+)/i)?.[1]
        : undefined;
      signals.occurrence = authored ?? score ?? '0';
    } else if (role === 'link') {
      signals.current = element.getAttribute('aria-current') || undefined;
      signals.expanded = ariaBoolean(element, 'expanded');
    } else if (role === 'file_input') {
      signals.hasValue = element.tagName === 'INPUT' ? Boolean(element.files?.length) : fileTriggerHasValue.has(element);
      signals.required = Boolean(element.required);
    } else if (role === 'button') {
      signals.pressed = ariaBoolean(element, 'pressed');
      signals.expanded = ariaBoolean(element, 'expanded');
    } else if (role === 'checkbox') {
      signals.checked = element.checked ?? ariaBoolean(element, 'checked');
      signals.required = Boolean(element.required);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
    } else if (role === 'radio') {
      signals.checked = element.checked ?? ariaBoolean(element, 'checked');
      signals.required = Boolean(element.required);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
    } else if (role === 'switch') {
      signals.checked = ariaBoolean(element, 'checked') ?? Boolean(element.checked);
    } else if (role === 'tab') {
      signals.selected = ariaBoolean(element, 'selected');
    } else if (role === 'menu_item') {
      signals.expanded = ariaBoolean(element, 'expanded');
    } else if (role === 'select') {
      signals.hasValue = element.selectedIndex >= 0;
      signals.required = Boolean(element.required);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
      signals.expanded = ariaBoolean(element, 'expanded') || false;
    } else if (role === 'content_editable') {
      signals.hasValue = Boolean(normalizedText(element.textContent, 1));
      signals.readonly = element.getAttribute('aria-readonly') === 'true';
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
    const interactionElement = role === 'file_input' ? visibleFileTrigger(element) : element;
    const box = boxFor(interactionElement);
    const visibility = visibilityFor(interactionElement, box);
    if (visibility === 'hidden') return null;
    const id = objectId(element);
    objectTargets.set(id, element);
    const name = safeName(element, role)
      || (role === 'file_input' && interactionElement !== element ? safeName(interactionElement, role) : undefined);
    const description = safeDescription(element, name, descriptor.protected)
      || repeatedActionContext(element, role, name);
    const object = {
      object_id: id, object_revision: revision + 1, frame_id: frameId,
      ...descriptor,
      document_bounds: { x: box.x + scrollX, y: box.y + scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: role === 'link' ? 'navigation_possible' : 'none',
    };
    if (name) object.name = name;
    if (description) object.description = description;
    if (role === 'reflex_target') object.loop_class_token = reflexLoopClassToken;
    if (descriptor.affordances.length && getComputedStyle(interactionElement).pointerEvents !== 'none') {
      const token = randomToken('action');
      object.action_token = token;
      tokenTargets.set(token, { element: interactionElement, role, objectId: id, affordances: descriptor.affordances });
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

  function imageObject(element, frameId) {
    const name = safeName(element, 'image');
    if (!name) return null;
    const box = boxFor(element);
    const visibility = visibilityFor(element, box);
    if (visibility === 'hidden') return null;
    const id = objectId(element);
    objectTargets.set(id, element);
    const object = {
      object_id: id, object_revision: revision + 1, frame_id: frameId,
      kind: 'image', role: 'image', state: {}, affordances: [], protected: false,
      document_bounds: { x: box.x + scrollX, y: box.y + scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: 'none', name,
    };
    const identity = normalizedText(element.getAttribute('data-saccade-image-identity'), 256);
    if (identity) object.description = `Semantic identity: ${identity}`;
    return object;
  }

  function collect() {
    if (!config) return null;
    tokenTargets.clear();
    objectTargets.clear();
    const objects = [];
    const seenFileTriggers = new Set();
    let truncated = false;
    if (location.hostname === 'mouseaccuracy.com' && location.pathname.startsWith('/game') && document.body) {
      const loopStatus = observationObject(document.body, 'reflex_target', config.frameId);
      if (loopStatus) objects.push(loopStatus);
    }
    const candidates = Array.from(document.querySelectorAll(CONTROL_SELECTOR));
    const actionNameCounts = new Map();
    for (const element of candidates) {
      const role = roleFor(element);
      if (!['button', 'link'].includes(role)) continue;
      const name = safeName(element, role);
      if (!name) continue;
      const key = `${role}\0${name}`;
      actionNameCounts.set(key, (actionNameCounts.get(key) || 0) + 1);
    }
    repeatedActionKeys = new Set([...actionNameCounts].filter(([, count]) => count > 1).map(([key]) => key));

    for (const element of candidates) {
      const role = roleFor(element);
      if (!role) continue;
      if (role === 'file_input') {
        const trigger = visibleFileTrigger(element);
        if (seenFileTriggers.has(trigger)) continue;
        seenFileTriggers.add(trigger);
      }
      const object = observationObject(element, role, config.frameId);
      if (object) objects.push(object);
      if (role === 'select' && object) {
        for (const option of element.options) objects.push(optionObject(option, config.frameId));
      }
      if (objects.length >= MAX_OBJECTS) { objects.length = MAX_OBJECTS; truncated = true; break; }
    }
    if (!truncated) {
      for (const element of document.querySelectorAll(IMAGE_SELECTOR)) {
        const object = imageObject(element, config.frameId);
        if (object) objects.push(object);
        if (objects.length >= MAX_OBJECTS) { objects.length = MAX_OBJECTS; truncated = true; break; }
      }
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
    if (request.operation === 'upload' && target.role === 'file_input') {
      activeFileTrigger = target.element;
      const expectedTrigger = activeFileTrigger;
      setTimeout(() => { if (activeFileTrigger === expectedTrigger) activeFileTrigger = null; }, 10000);
    }
    return prepared;
  }

  function softClick(request) {
    prepare(request);
    const target = tokenTargets.get(request.action_token);
    if (!target || target.role !== 'reflex_target') throw new Error('soft click requires a current reflex target');
    const box = boxFor(target.element);
    const clientX = box.x + box.width / 2;
    const clientY = box.y + box.height / 2;
    for (const [type, EventClass, buttons] of [
      ['pointermove', PointerEvent, 0], ['mousemove', MouseEvent, 0],
      ['pointerdown', PointerEvent, 1], ['mousedown', MouseEvent, 1],
      ['pointerup', PointerEvent, 0], ['mouseup', MouseEvent, 0], ['click', MouseEvent, 0],
    ]) {
      target.element.dispatchEvent(new EventClass(type, {
        bubbles: true, cancelable: true, composed: true, clientX, clientY,
        button: 0, buttons, pointerId: 1, pointerType: 'mouse', isPrimary: true,
      }));
    }
    requestAnimationFrame(collect);
    return { accepted: true };
  }

  function schedule() {
    if (scheduled || !config) return;
    scheduled = true;
    requestAnimationFrame(() => { scheduled = false; collect(); });
  }

  function mutationCanChangeObservation(record) {
    if (location.hostname === 'mouseaccuracy.com' && location.pathname.startsWith('/game')) return true;
    const element = record.target.nodeType === Node.ELEMENT_NODE
      ? record.target : record.target.parentElement;
    if (!element) return false;
    if (element.matches(OBSERVED_SELECTOR) || element.closest(OBSERVED_SELECTOR)) return true;
    if (record.type === 'attributes') return Boolean(element.querySelector(OBSERVED_SELECTOR));
    if (record.type !== 'childList') return false;
    return [...record.addedNodes, ...record.removedNodes].some((node) => {
      if (node.nodeType !== Node.ELEMENT_NODE) return false;
      return node.matches(OBSERVED_SELECTOR) || Boolean(node.querySelector(OBSERVED_SELECTOR));
    });
  }

  function configure(next) {
    config = next;
    for (const observer of observers.splice(0)) observer.disconnect();
    const observer = new MutationObserver((records) => {
      if (records.some(mutationCanChangeObservation)) schedule();
    });
    observer.observe(document.documentElement, { subtree: true, childList: true, attributes: true, characterData: true });
    observers.push(observer);
    for (const event of ['input', 'focusin', 'focusout']) document.addEventListener(event, schedule, true);
    document.addEventListener('change', (event) => {
      const changed = event.target;
      if (activeFileTrigger && changed instanceof HTMLInputElement
        && String(changed.type).toLowerCase() === 'file' && changed.files?.length) {
        fileTriggerHasValue.add(activeFileTrigger);
        activeFileTrigger = null;
      }
      schedule();
    }, true);
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
      else if (message.kind === 'collector.soft_click') respond({ ok: true, result: softClick(message.request) });
      else return false;
    } catch (error) {
      const detail = String(error.message || error);
      if (message.kind === 'collector.prepare_action' && detail === 'stale action basis') collect();
      respond({ ok: false, error: detail });
    }
    return true;
  });
})();
