(() => {
  const registry = globalThis.SaccadeControls.registry;
  const { OBSERVATION_SCHEMA, randomToken } = globalThis.SaccadeProtocol;
  const { isProtectedFieldType } = globalThis.SaccadeConsent;
  const MAX_OBJECTS = 10000;
  const MAX_STRUCTURAL_TEXT_BYTES = 256 * 1024;
  const CONTROL_SELECTOR = 'a[href],button,input,textarea,select,[role="button"],[role="checkbox"],[role="radio"],[role="switch"],[role="tab"],[role="menuitem"],[role="textbox"],[role="listbox"],[role="combobox"],[contenteditable],[data-saccade-reflex-target],.target';
  const IMAGE_SELECTOR = 'img[alt],img[aria-label],img[data-saccade-image-identity],svg[aria-label],svg[data-saccade-image-identity]';
  const STRUCTURAL_SELECTOR = 'h1,h2,h3,h4,h5,h6,p,li,th,td,[role="heading"],[role="paragraph"],[role="listitem"],[role="cell"],[role="columnheader"],[role="rowheader"],[role="alert"],[role="status"]';
  const DIALOG_SELECTOR = '[role="dialog"],[aria-modal="true"]';
  const OBSERVED_SELECTOR = `${CONTROL_SELECTOR},${IMAGE_SELECTOR},${STRUCTURAL_SELECTOR},${DIALOG_SELECTOR}`;
  const SOFTWARE_CLICK_ROLES = new Set([
    'button', 'link', 'checkbox', 'radio', 'switch', 'tab', 'menu_item', 'reflex_target',
  ]);
  const identities = new WeakMap();
  const tokenTargets = new Map();
  const objectTargets = new Map();
  const fileTriggerHasValue = new WeakSet();
  const frameIdentities = new WeakMap();
  const frameDocumentIds = new WeakMap();
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
  let frameSerial = 0;
  let observedRoots = new WeakSet();
  let observedDocuments = new WeakSet();

  function normalizedText(value, limit) {
    const text = String(value || '').replace(/\s+/g, ' ').trim();
    return text ? text.slice(0, limit) : undefined;
  }

  function composedQuery(root, selector) {
    const matches = [];
    const roots = [root];
    const visited = new Set();
    while (roots.length) {
      const current = roots.pop();
      if (!current || visited.has(current)) continue;
      visited.add(current);
      for (const element of current.querySelectorAll(selector)) matches.push(element);
      for (const element of current.querySelectorAll('*')) {
        if (element.shadowRoot) roots.push(element.shadowRoot);
      }
    }
    return matches;
  }

  function frameIdentity(element) {
    if (!frameIdentities.has(element)) frameIdentities.set(element, `${config.frameId}.${++frameSerial}`);
    return frameIdentities.get(element);
  }

  function frameDocumentId(doc) {
    if (!frameDocumentIds.has(doc)) frameDocumentIds.set(doc, randomToken('document'));
    return frameDocumentIds.get(doc);
  }

  function collectFrameContexts() {
    const contexts = [{ doc: document, frameId: config.frameId, documentId, parentFrameId: undefined, origin: location.origin }];
    const frames = [];
    const limitations = [];
    for (let index = 0; index < contexts.length; index += 1) {
      const parent = contexts[index];
      frames.push({
        frame_id: parent.frameId, ...(parent.parentFrameId ? { parent_frame_id: parent.parentFrameId } : {}),
        document_id: parent.documentId, origin: parent.origin, status: 'observed',
      });
      for (const element of composedQuery(parent.doc, 'iframe,frame')) {
        const frameId = frameIdentity(element);
        let child;
        let sameOrigin = false;
        try {
          child = element.contentDocument;
          const childUrl = child?.location.href || '';
          const inheritedOrigin = childUrl === 'about:blank' || childUrl === 'about:srcdoc';
          sameOrigin = Boolean(child?.documentElement)
            && (inheritedOrigin || new URL(childUrl).origin === parent.origin);
        } catch (_error) { child = null; }
        if (!sameOrigin) {
          frames.push({
            frame_id: frameId, parent_frame_id: parent.frameId,
            document_id: `restricted.${frameId}`, origin: '', status: 'restricted_permission',
          });
          limitations.push({ kind: 'restricted_frame', frame_id: frameId });
          continue;
        }
        contexts.push({ doc: child, frameId, documentId: frameDocumentId(child), parentFrameId: parent.frameId, origin: parent.origin });
      }
    }
    return { contexts, frames, limitations };
  }

  function observeMutationRoot(root) {
    if (!root || observedRoots.has(root)) return;
    observedRoots.add(root);
    const observer = new MutationObserver((records) => {
      if (records.some(mutationCanChangeObservation)) schedule();
    });
    observer.observe(root, { subtree: true, childList: true, attributes: true, characterData: true });
    observers.push(observer);
  }

  function observeDocument(doc) {
    if (observedDocuments.has(doc)) return;
    observedDocuments.add(doc);
    observeMutationRoot(doc.documentElement);
    for (const element of doc.querySelectorAll('*')) {
      if (element.shadowRoot) observeMutationRoot(element.shadowRoot);
    }
    for (const event of ['input', 'focusin', 'focusout']) doc.addEventListener(event, schedule, true);
    for (const event of ['transitionend', 'animationend']) doc.addEventListener(event, schedule, true);
    doc.addEventListener('change', (event) => {
      const changed = event.target;
      if (activeFileTrigger && changed?.tagName === 'INPUT'
        && String(changed.type).toLowerCase() === 'file' && changed.files?.length) {
        fileTriggerHasValue.add(activeFileTrigger);
        activeFileTrigger = null;
      }
      schedule();
    }, true);
    doc.defaultView.addEventListener('scroll', schedule, { passive: true });
    doc.defaultView.addEventListener('resize', schedule, { passive: true });
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
        const style = node.ownerDocument.defaultView.getComputedStyle(node);
        if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return;
      }
      for (const child of node.childNodes) visit(child);
      if (node.shadowRoot) for (const child of node.shadowRoot.childNodes) visit(child);
    };
    visit(element);
    return normalizedText(chunks.join(' '), limit);
  }

  function referencedText(element, attribute, limit) {
    const ids = String(element.getAttribute(attribute) || '').split(/\s+/).filter(Boolean);
    return normalizedText(ids.map((id) => {
      const root = element.getRootNode();
      const node = root.getElementById?.(id) || root.querySelector?.(`#${CSS.escape(id)}`) || element.ownerDocument.getElementById(id);
      if (!node || node.getClientRects().length === 0) return '';
      const style = node.ownerDocument.defaultView.getComputedStyle(node);
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
    for (let depth = 0; group && group !== element.ownerDocument.body && depth < 6; depth += 1, group = group.parentElement) {
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
    const pageLocation = element.ownerDocument.location;
    const mouseAccuracyBridge = pageLocation.hostname === 'mouseaccuracy.com'
      && pageLocation.pathname.startsWith('/game')
      && element.classList.contains('target')
      && !element.classList.contains('hit');
    if (applicationBridge || mouseAccuracyBridge) return 'reflex_target';
    if (ariaRole === 'menuitem') return 'menu_item';
    if (tag === 'A' && element.hasAttribute('href')) return 'link';
    if (tag === 'INPUT' && type === 'file') return 'file_input';
    if ((tag === 'INPUT' && type === 'radio') || ariaRole === 'radio') return 'radio';
    if (ariaRole === 'switch') return 'switch';
    if (ariaRole === 'tab') return 'tab';
    if (ariaRole === 'listbox' && comboboxForListbox(element)) return null;
    if (ariaRole === 'listbox' || ariaRole === 'combobox') return 'select';
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
    const view = element.ownerDocument.defaultView;
    const style = view.getComputedStyle(element);
    if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0 || box.width <= 0 || box.height <= 0) return 'hidden';
    if (box.x + box.width <= 0 || box.y + box.height <= 0 || box.x >= view.innerWidth || box.y >= view.innerHeight) return 'offscreen';
    return 'visible';
  }

  function visibleFileTrigger(input) {
    const visible = (element) => visibilityFor(element, boxFor(element)) === 'visible';
    const labelled = Array.from(input.labels || []).find(visible);
    if (labelled) return labelled;
    if (input.id) {
      const controlled = Array.from(input.ownerDocument.querySelectorAll('[aria-controls]')).find((element) => (
        String(element.getAttribute('aria-controls') || '').split(/\s+/).includes(input.id)
          && element.matches('button,[role="button"],label') && visible(element)
      ));
      if (controlled) return controlled;
    }
    let ancestor = input.parentElement;
    for (let depth = 0; ancestor && ancestor !== input.ownerDocument.body && depth < 5; depth += 1, ancestor = ancestor.parentElement) {
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
      const page = element.ownerDocument;
      if (element === page.body && page.location.hostname === 'mouseaccuracy.com') signals.enabled = false;
      const authored = element.getAttribute('data-saccade-reflex-occurrence');
      const score = page.location.hostname === 'mouseaccuracy.com'
        ? (page.body?.innerText || '').match(/SCORE\s*(\d+)/i)?.[1]
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
      signals.hasValue = element.tagName === 'SELECT'
        ? element.selectedIndex >= 0
        : optionsForChoice(element).some((option) => option.getAttribute('aria-selected') === 'true');
      signals.required = Boolean(element.required) || element.getAttribute('aria-required') === 'true';
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
      signals.expanded = ariaBoolean(element, 'expanded') ?? element.getAttribute('role') === 'listbox';
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
    const deferred = role === 'button' && (
      element.type === 'submit'
      || element.getAttribute('aria-haspopup') === 'dialog'
      || element.hasAttribute('aria-controls')
    );
    const object = {
      object_id: id, object_revision: revision + 1, frame_id: frameId,
      ...descriptor,
      document_bounds: { x: box.x + element.ownerDocument.defaultView.scrollX, y: box.y + element.ownerDocument.defaultView.scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: role === 'link'
        ? 'navigation_possible' : deferred ? 'deferred_content_possible' : 'none',
    };
    if (name) object.name = name;
    if (description) object.description = description;
    if (role === 'reflex_target') object.loop_class_token = reflexLoopClassToken;
    if (descriptor.affordances.length && interactionElement.ownerDocument.defaultView.getComputedStyle(interactionElement).pointerEvents !== 'none') {
      const token = randomToken('action', 16);
      object.action_token = token;
      tokenTargets.set(token, { element: interactionElement, role, objectId: id, affordances: descriptor.affordances });
    }
    return object;
  }

  function comboboxForListbox(listbox) {
    if (!listbox.id) return null;
    for (const candidate of composedQuery(listbox.ownerDocument, '[role="combobox"][aria-controls],[role="combobox"][aria-owns]')) {
      const ids = `${candidate.getAttribute('aria-controls') || ''} ${candidate.getAttribute('aria-owns') || ''}`.split(/\s+/);
      if (ids.includes(listbox.id)) return candidate;
    }
    return null;
  }

  function choiceOwner(option) {
    const native = option.closest('select');
    if (native) return native;
    const listbox = option.closest('[role="listbox"]');
    if (!listbox) return null;
    return comboboxForListbox(listbox) || listbox;
  }

  function optionsForChoice(owner) {
    if (owner.tagName === 'SELECT') return Array.from(owner.options);
    const ids = `${owner.getAttribute('aria-controls') || ''} ${owner.getAttribute('aria-owns') || ''}`.split(/\s+/).filter(Boolean);
    const roots = ids.map((id) => owner.ownerDocument.getElementById(id)).filter(Boolean);
    if (!roots.length && owner.getAttribute('role') === 'listbox') roots.push(owner);
    return roots.flatMap((root) => Array.from(root.querySelectorAll('[role="option"]')));
  }

  function optionEnabled(option, owner) {
    return !option.disabled && option.getAttribute('aria-disabled') !== 'true'
      && !owner.disabled && owner.getAttribute('aria-disabled') !== 'true';
  }

  function optionObject(option, frameId) {
    const name = safeName(option, 'option');
    const owner = choiceOwner(option);
    if (!owner) return null;
    const selected = option.tagName === 'OPTION' ? option.selected : option.getAttribute('aria-selected') === 'true';
    const descriptor = { ...registry.option(name || '', selected, optionEnabled(option, owner)) };
    if (!name) delete descriptor.name;
    const box = boxFor(owner);
    const id = objectId(option);
    objectTargets.set(id, option);
    return {
      object_id: id, object_revision: revision + 1, frame_id: frameId, ...descriptor,
      document_bounds: { x: box.x + owner.ownerDocument.defaultView.scrollX, y: box.y + owner.ownerDocument.defaultView.scrollY, width: box.width, height: box.height },
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
      document_bounds: { x: box.x + element.ownerDocument.defaultView.scrollX, y: box.y + element.ownerDocument.defaultView.scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: 'none', name,
    };
    const identity = normalizedText(element.getAttribute('data-saccade-image-identity'), 256);
    if (identity) object.description = `Semantic identity: ${identity}`;
    return object;
  }

  function structuralRole(element) {
    const tag = element.tagName;
    const role = String(element.getAttribute('role') || '').toLowerCase();
    if (/^H[1-6]$/.test(tag) || role === 'heading') return 'heading';
    if (tag === 'P' || role === 'paragraph') return 'paragraph';
    if (tag === 'LI' || role === 'listitem') return 'list_item';
    if (tag === 'TH' || tag === 'TD' || ['cell', 'columnheader', 'rowheader'].includes(role)) return 'cell';
    if (role === 'alert') return 'alert';
    if (role === 'status') return 'status';
    return null;
  }

  function structuralText(element) {
    if (element.closest('[aria-hidden="true"],[hidden],template,script,style,noscript')) return undefined;
    if (element.closest(CONTROL_SELECTOR)) return undefined;
    const chunks = [];
    const visit = (node) => {
      if (node.nodeType === Node.TEXT_NODE) {
        chunks.push(node.nodeValue || '');
        return;
      }
      if (node.nodeType !== Node.ELEMENT_NODE) return;
      if (node !== element) {
        if (node.matches(`${CONTROL_SELECTOR},${IMAGE_SELECTOR},${STRUCTURAL_SELECTOR},script,style,template,noscript`)) return;
        if (node.getAttribute('aria-hidden') === 'true' || node.hidden) return;
        const style = node.ownerDocument.defaultView.getComputedStyle(node);
        if (style.display === 'none' || style.visibility === 'hidden' || Number(style.opacity) === 0) return;
      }
      for (const child of node.childNodes) visit(child);
      if (node.shadowRoot) for (const child of node.shadowRoot.childNodes) visit(child);
    };
    visit(element);
    return normalizedText(chunks.join(' '), 4096);
  }

  function structuralObject(element, frameId, forcedRole, forcedText) {
    const role = forcedRole || structuralRole(element);
    const text = forcedText || (role ? structuralText(element) : undefined);
    if (!role || !text) return null;
    const box = boxFor(element);
    const visibility = visibilityFor(element, box);
    if (visibility === 'hidden') return null;
    const state = {};
    if (role === 'heading') {
      const authored = Number.parseInt(element.getAttribute('aria-level') || '', 10);
      const native = /^H[1-6]$/.test(element.tagName) ? Number(element.tagName.slice(1)) : undefined;
      const level = Number.isInteger(authored) && authored > 0 ? authored : native;
      if (level) state.level = String(level);
    }
    if (['alert', 'status'].includes(role) && element.hasAttribute('aria-busy')) {
      state.busy = String(element.getAttribute('aria-busy') === 'true');
    }
    const id = objectId(element);
    objectTargets.set(id, element);
    return {
      object_id: id, object_revision: revision + 1, frame_id: frameId,
      kind: 'text', role, text, state, affordances: [], protected: false,
      document_bounds: { x: box.x + element.ownerDocument.defaultView.scrollX, y: box.y + element.ownerDocument.defaultView.scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: 'none',
    };
  }

  function dialogTitleCandidates(doc) {
    const titles = [];
    for (const dialog of composedQuery(doc, DIALOG_SELECTOR)) {
      const box = boxFor(dialog);
      if (visibilityFor(dialog, box) === 'hidden') continue;
      const text = safeName(dialog, 'dialog');
      if (text) titles.push({ element: dialog, text });
    }
    return titles;
  }

  function collect() {
    if (!config) return null;
    if (document.readyState === 'loading') return null;
    tokenTargets.clear();
    objectTargets.clear();
    const frameState = collectFrameContexts();
    for (const context of frameState.contexts) observeDocument(context.doc);
    const objects = [];
    const seenFileTriggers = new Set();
    let truncated = false;
    if (location.hostname === 'mouseaccuracy.com' && location.pathname.startsWith('/game') && document.body) {
      const loopStatus = observationObject(document.body, 'reflex_target', config.frameId);
      if (loopStatus) objects.push(loopStatus);
    }
    const candidates = frameState.contexts.flatMap((context) => (
      composedQuery(context.doc, CONTROL_SELECTOR).map((element) => ({ element, frameId: context.frameId }))
    ));
    const actionNameCounts = new Map();
    for (const { element } of candidates) {
      const role = roleFor(element);
      if (!['button', 'link'].includes(role)) continue;
      const name = safeName(element, role);
      if (!name) continue;
      const key = `${role}\0${name}`;
      actionNameCounts.set(key, (actionNameCounts.get(key) || 0) + 1);
    }
    repeatedActionKeys = new Set([...actionNameCounts].filter(([, count]) => count > 1).map(([key]) => key));

    for (const { element, frameId } of candidates) {
      const role = roleFor(element);
      if (!role) continue;
      if (role === 'file_input') {
        const trigger = visibleFileTrigger(element);
        if (seenFileTriggers.has(trigger)) continue;
        seenFileTriggers.add(trigger);
      }
      const object = observationObject(element, role, frameId);
      if (object) objects.push(object);
      if (role === 'select' && object) {
        for (const option of optionsForChoice(element)) {
          const choice = optionObject(option, frameId);
          if (choice) objects.push(choice);
        }
      }
      if (objects.length >= MAX_OBJECTS) { objects.length = MAX_OBJECTS; truncated = true; break; }
    }
    if (!truncated) {
      for (const context of frameState.contexts) {
        for (const element of composedQuery(context.doc, IMAGE_SELECTOR)) {
          const object = imageObject(element, context.frameId);
          if (object) objects.push(object);
          if (objects.length >= MAX_OBJECTS) { objects.length = MAX_OBJECTS; truncated = true; break; }
        }
        if (truncated) break;
      }
    }
    let structuralTextBytes = 0;
    if (!truncated) {
      const encoder = new TextEncoder();
      for (const context of frameState.contexts) {
        const ordinary = composedQuery(context.doc, STRUCTURAL_SELECTOR)
          .map((element) => ({ element, dialogTitle: false }));
        const dialogTitles = dialogTitleCandidates(context.doc)
          .map(({ element, text }) => ({ element, dialogTitle: true, text }));
        const seenStructural = new Set();
        for (const { element, dialogTitle, text } of [...ordinary, ...dialogTitles]) {
          if (seenStructural.has(element)) continue;
          seenStructural.add(element);
          const projected = structuralObject(
            element, context.frameId, dialogTitle ? 'heading' : undefined, text,
          );
          if (!projected) continue;
          const bytes = encoder.encode(projected.text).byteLength;
          if (structuralTextBytes + bytes > MAX_STRUCTURAL_TEXT_BYTES) { truncated = true; break; }
          structuralTextBytes += bytes;
          objects.push(projected);
          if (objects.length >= MAX_OBJECTS) { objects.length = MAX_OBJECTS; truncated = true; break; }
        }
        if (truncated) break;
      }
    }
    revision += 1;
    viewportRevision += 1;
    const snapshot = {
      schema: OBSERVATION_SCHEMA, browser_instance_id: config.browserInstanceId,
      tab_id: config.tabId, document_id: documentId, revision, viewport_revision: viewportRevision,
      frames: frameState.frames,
      objects, changes: [], coverage: {
        source: 'dom_extension',
        observed_frame_count: frameState.frames.filter((frame) => frame.status === 'observed').length,
        restricted_frame_count: frameState.frames.filter((frame) => frame.status !== 'observed').length,
        truncated,
      },
      limitations: [...frameState.limitations, ...(truncated ? [{ kind: 'truncated', frame_id: config.frameId }] : [])], gap: false,
    };
    chrome.runtime.sendMessage({ kind: 'collector.observation', payload: snapshot });
    return snapshot;
  }

  function isTopmost(element, box) {
    let view = element.ownerDocument.defaultView;
    let x = box.x + box.width / 2;
    let y = box.y + box.height / 2;
    const root = element.getRootNode();
    let hit = root.elementFromPoint?.(x, y) || element.ownerDocument.elementFromPoint(x, y);
    if (hit !== element && !element.contains(hit)) return false;
    while (view !== view.top) {
      const frame = view.frameElement;
      if (!frame) return false;
      const frameBox = frame.getBoundingClientRect();
      x += frameBox.x + frame.clientLeft;
      y += frameBox.y + frame.clientTop;
      view = view.parent;
      hit = view.document.elementFromPoint(x, y);
      if (hit !== frame && !frame.contains(hit)) return false;
    }
    return true;
  }

  function topViewportBox(element, box) {
    let view = element.ownerDocument.defaultView;
    let x = box.x;
    let y = box.y;
    while (view !== view.top) {
      const frame = view.frameElement;
      if (!frame) throw new Error('ambiguous frame transform');
      const frameBox = frame.getBoundingClientRect();
      x += frameBox.x + frame.clientLeft;
      y += frameBox.y + frame.clientTop;
      view = view.parent;
    }
    return { x, y, width: box.width, height: box.height };
  }

  function prepare(request) {
    if (!config || request.browser_instance_id !== config.browserInstanceId || request.tab_id !== config.tabId
      || request.document_id !== documentId || request.basis_revision !== revision) throw new Error('stale action basis');
    const target = tokenTargets.get(request.action_token);
    if (!target || !target.element.isConnected || !target.affordances.includes(request.operation)) throw new Error('action token is not current for operation');
    target.element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
    if (request.operation === 'type' || request.operation === 'select') {
      target.element.focus({ preventScroll: true });
    }
    const box = boxFor(target.element);
    const topBox = topViewportBox(target.element, box);
    const prepared = {
      browser_instance_id: config.browserInstanceId, tab_id: config.tabId, document_id: documentId,
      basis_revision: revision, viewport_revision: viewportRevision, object_id: target.objectId,
      action_token: request.action_token, operation: request.operation,
      screen_bounds: { x: screenX + topBox.x, y: screenY + Math.max(0, outerHeight - innerHeight) + topBox.y, width: topBox.width, height: topBox.height },
      visible: visibilityFor(target.element, box) === 'visible', topmost: isTopmost(target.element, box),
      focus_verified: (request.operation === 'type' || request.operation === 'select')
        ? target.element.ownerDocument.activeElement === target.element
        : top.document.hasFocus(),
    };
    if (request.operation === 'select') {
      const optionId = request.payload?.kind === 'select' ? request.payload.option_object_id : '';
      const option = objectTargets.get(optionId);
      const owner = option ? choiceOwner(option) : null;
      const choices = owner ? optionsForChoice(owner).filter((item) => optionEnabled(item, owner)) : [];
      if (!option || !option.matches('option,[role="option"]') || owner !== target.element || !optionEnabled(option, owner)) {
        throw new Error('select option is not bound and enabled for this control');
      }
      prepared.selection_index = choices.indexOf(option);
      if (prepared.selection_index < 0) throw new Error('select option has no enabled option position');
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
    if (!target || !SOFTWARE_CLICK_ROLES.has(target.role)) {
      throw new Error('software click is not registered for the current control');
    }
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

  function softAction(request) {
    if (request.operation === 'click') return softClick(request);
    const prepared = prepare(request);
    if (request.operation !== 'select' || request.payload?.kind !== 'select') {
      throw new Error('software action is not registered for the current operation');
    }
    const target = tokenTargets.get(request.action_token)?.element;
    const option = objectTargets.get(request.payload.option_object_id);
    if (!target || !option || choiceOwner(option) !== target || !optionEnabled(option, target)) {
      throw new Error('software select option is not bound and enabled for this control');
    }
    if (target.matches('select') && option.matches('option')) {
      option.selected = true;
      target.dispatchEvent(new Event('input', { bubbles: true }));
      target.dispatchEvent(new Event('change', { bubbles: true }));
    } else {
      for (const key of ['Home', ...Array(prepared.selection_index).fill('ArrowDown'), 'Enter']) {
        target.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
      }
    }
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
      ? record.target : record.target.host || record.target.parentElement;
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
    observedRoots = new WeakSet();
    observedDocuments = new WeakSet();
    observeDocument(document);
    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', collect, { once: true });
      return null;
    }
    return collect();
  }

  function deauthorize() {
    config = null;
    tokenTargets.clear();
    objectTargets.clear();
    for (const observer of observers.splice(0)) observer.disconnect();
    observedRoots = new WeakSet();
    observedDocuments = new WeakSet();
  }

  chrome.runtime.onMessage.addListener((message, _sender, respond) => {
    try {
      if (message.kind === 'collector.ping') respond({ ok: true });
      else if (message.kind === 'collector.configure') { configure(message.config); respond({ ok: true, document_id: documentId }); }
      else if (message.kind === 'collector.observe') { collect(); respond({ ok: true }); }
      else if (message.kind === 'collector.deauthorize') { deauthorize(); respond({ ok: true }); }
      else if (message.kind === 'collector.prepare_action') respond({ ok: true, prepared: prepare(message.request) });
      else if (message.kind === 'collector.soft_click') respond({ ok: true, result: softClick(message.request) });
      else if (message.kind === 'collector.soft_action') respond({ ok: true, result: softAction(message.request) });
      else return false;
    } catch (error) {
      const detail = String(error.message || error);
      if (message.kind === 'collector.prepare_action' && detail === 'stale action basis') collect();
      respond({ ok: false, error: detail });
    }
    return true;
  });
})();
