(() => {
  const registry = globalThis.SaccadeControls.registry;
  const { compileChanges, compactTransport } = globalThis.SaccadeTruthDelta;
  const { OBSERVATION_SCHEMA, randomToken } = globalThis.SaccadeProtocol;
  const { isProtectedFieldType, redactProtectedText } = globalThis.SaccadeConsent;
  const { occurrence: reflexOccurrence } = globalThis.SaccadeControls.reflex_target;
  const MAX_OBJECTS = 10000;
  const MAX_STRUCTURAL_TEXT_BYTES = 256 * 1024;
  const MAX_UPLOAD_BYTES = 16 * 1024 * 1024;
  const CONTROL_SELECTOR = 'a[href],button,input,textarea,select,[role="button"],[role="checkbox"],[role="radio"],[role="switch"],[role="slider"],[role="tab"],[role="menuitem"],[role="textbox"],[role="listbox"],[role="combobox"],[contenteditable],[data-saccade-reflex-target],[data-saccade-label],[data-saccade-generic-control],[data-saccade-file-upload],[id*="upload" i][class*="button" i],[class*="upload" i][class*="button" i],.target';
  const IMAGE_SELECTOR = 'img[alt],img[aria-label],img[data-saccade-image-identity],svg[aria-label],svg[data-saccade-image-identity]';
  const STRUCTURAL_SELECTOR = 'h1,h2,h3,h4,h5,h6,p,ul,ol,li,table,tr,th,td,[role="text"],[role="heading"],[role="paragraph"],[role="list"],[role="listitem"],[role="table"],[role="row"],[role="cell"],[role="columnheader"],[role="rowheader"],[role="alert"],[role="status"],[aria-live]';
  const GENERIC_TEXT_SELECTOR = 'div,span,section,article,main,aside';
  const SURFACE_SELECTOR = 'canvas,video,embed[type="application/pdf"],object[type="application/pdf"],[data-saccade-restricted-document]';
  const DIALOG_SELECTOR = '[role="dialog"],[aria-modal="true"]';
  const OBSERVED_SELECTOR = `${CONTROL_SELECTOR},label,${IMAGE_SELECTOR},${STRUCTURAL_SELECTOR},${DIALOG_SELECTOR},${SURFACE_SELECTOR}`;
  // Typing is registered for the roles whose Truth exposes has_value, which is
  // the only evidence a typed value can produce: Truth never exposes the value
  // itself. Protected fields already carry no type affordance, so prepare()
  // rejects them before this set is consulted.
  const SOFTWARE_TYPE_ROLES = new Set([
    'text_field', 'search_field', 'text_area', 'content_editable', 'spin_button',
  ]);
  const SOFTWARE_CLICK_ROLES = new Set([
    'button', 'link', 'checkbox', 'radio', 'switch', 'select', 'option', 'tab', 'menu_item', 'reflex_target',
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
  const seenMouseAccuracyHits = new WeakSet();
  let mouseAccuracyHitOccurrence = 0;
  let objectSerial = 0;
  let revision = 0;
  // A pure hash/pushState change mutates no DOM, so the object fingerprint is
  // unchanged. The document URL is public browser truth and must still advance
  // the revision, or anchor navigation would be invisible to Truth.
  let lastUrlFingerprint = null;
  let viewportRevision = 0;
  // viewport_revision tracks geometry only. A DOM or URL change must not
  // advance it, or an Agent cannot tell "the page moved" from "the page changed".
  let lastGeometryFingerprint = null;
  let config = null;
  let scheduled = false;
  let scheduledFrame = null;
  let activeFileTrigger = null;
  let repeatedActionKeys = new Set();
  let frameSerial = 0;
  let observedRoots = new WeakSet();
  let observedDocuments = new WeakSet();
  let choiceHasValue = new WeakMap();
  let rememberedChoiceOwner = new WeakMap();
  let rememberedChoicePopup = new WeakMap();
  let compiledObjects = null;
  let workerPort = null;
  let geometryResizeObserver = null;
  let geometryObservedElements = new Set();

  function connectWorkerPort() {
    if (workerPort || !config) return;
    const port = chrome.runtime.connect({ name: 'saccade.collector' });
    workerPort = port;
    port.onDisconnect.addListener(() => {
      if (workerPort === port) workerPort = null;
      if (config) setTimeout(() => { connectWorkerPort(); schedule(); }, 100);
    });
  }

  function normalizedText(value, limit) {
    const text = redactProtectedText(value).replace(/\s+/g, ' ').trim();
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
    const contexts = [{ doc: document, frameId: config.frameId, documentId, parentFrameId: undefined, origin: location.origin, url: location.href }];
    const frames = [];
    const limitations = [];
    for (let index = 0; index < contexts.length; index += 1) {
      const parent = contexts[index];
      frames.push({
        frame_id: parent.frameId, ...(parent.parentFrameId ? { parent_frame_id: parent.parentFrameId } : {}),
        document_id: parent.documentId, document_url: parent.url, origin: parent.origin, status: 'observed',
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
        contexts.push({ doc: child, frameId, documentId: frameDocumentId(child), parentFrameId: parent.frameId, origin: parent.origin, url: child.location.href });
      }
    }
    return { contexts, frames, limitations };
  }

  function observeMutationRoot(root) {
    if (!root || observedRoots.has(root)) return;
    observedRoots.add(root);
    const observer = new MutationObserver((records) => {
      if (!records.some(mutationCanChangeObservation)) return;
      if (isMouseAccuracyGame(document)) scheduleVisual();
      else schedule();
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
    for (const event of ['transitionrun', 'transitionstart', 'transitionend', 'transitioncancel', 'animationstart', 'animationend', 'animationcancel']) {
      doc.addEventListener(event, scheduleVisual, true);
    }
    doc.fonts?.addEventListener?.('loadingdone', scheduleVisual);
    doc.addEventListener('change', (event) => {
      const changed = event.target;
      if (activeFileTrigger && changed?.tagName === 'INPUT'
        && String(changed.type).toLowerCase() === 'file' && changed.files?.length) {
        fileTriggerHasValue.add(activeFileTrigger);
        activeFileTrigger = null;
      }
      schedule();
    }, true);
    for (const event of ['hashchange', 'popstate', 'pageshow']) {
      doc.defaultView.addEventListener(event, schedule);
    }
    doc.defaultView.addEventListener('scroll', scheduleVisual, { passive: true });
    doc.defaultView.addEventListener('resize', scheduleVisual, { passive: true });
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
    if (!name) return undefined;
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
    if (placeholder && placeholder !== name) return `Placeholder: ${placeholder}`;
    const title = normalizedText(element.getAttribute('title'), 1024);
    return title && title !== name ? title : undefined;
  }

  function roleFor(element) {
    const tag = element.tagName;
    const type = String(element.type || '').toLowerCase();
    const ariaRole = String(element.getAttribute('role') || '').toLowerCase();
    const applicationBridge = element.hasAttribute('data-saccade-reflex-target');
    const mouseAccuracyBridge = isMouseAccuracyGame(element.ownerDocument)
      && element.classList.contains('target')
      && !element.classList.contains('hit');
    if (applicationBridge || mouseAccuracyBridge) return 'reflex_target';
    if (ariaRole === 'menuitem') return 'menu_item';
    if (tag === 'A' && element.hasAttribute('href')) return 'link';
    if (tag === 'INPUT' && type === 'file') return 'file_input';
    if ((tag === 'INPUT' && type === 'radio') || ariaRole === 'radio') return 'radio';
    if (ariaRole === 'switch') return 'switch';
    if ((tag === 'INPUT' && type === 'range') || ariaRole === 'slider') return 'slider';
    if (ariaRole === 'tab') return 'tab';
    if (ariaRole === 'listbox' && comboboxForListbox(element)) return null;
    if (ariaRole === 'listbox' || ariaRole === 'combobox') return 'select';
    const authoredUploadLike = element.hasAttribute('data-saccade-file-upload') || (
      /upload/i.test(`${element.id} ${element.className}`) && /\b(btn|button)\b/i.test(String(element.className).replace(/[_-]+/g, ' '))
    );
    const buttonLike = tag === 'BUTTON' || ariaRole === 'button'
      || (tag === 'INPUT' && ['button', 'submit', 'reset'].includes(type))
      || authoredUploadLike;
    if (buttonLike && isFileUploadTrigger(element, safeName(element, 'button'))) return 'file_input';
    if (buttonLike) return 'button';
    if (tag === 'INPUT' && type === 'checkbox' || ariaRole === 'checkbox') return 'checkbox';
    if (tag === 'SELECT') return 'select';
    if (tag === 'INPUT' && type === 'number') return 'spin_button';
    if (element.hasAttribute('data-saccade-label')) return 'label';
    if (element.hasAttribute('data-saccade-generic-control')) return 'generic_control';
    if (tag === 'INPUT' && type === 'search') return 'search_field';
    if (tag === 'TEXTAREA') return 'text_area';
    if (tag === 'INPUT' && ariaRole === 'searchbox') return 'search_field';
    if (tag === 'INPUT' && ['text', 'email', 'tel', 'url', 'password', 'date', 'time', 'month', 'week', 'datetime-local', 'color'].includes(type)) return 'text_field';
    if (ariaRole === 'textbox' && element.isContentEditable) return 'content_editable';
    if (element.isContentEditable) return 'content_editable';
    return null;
  }

  function isFileUploadTrigger(element, name) {
    if (/\b(upload|choose|select|browse|attach|replace|add)\b.*\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b/i.test(name || '')) return true;
    const bareUploadVerb = /^(upload|choose|browse|attach|select|add)$/i.test(name || '');
    const authoredUploadButton = element.hasAttribute('data-saccade-file-upload') || (
      /upload/i.test(`${element.id} ${element.className}`)
        && /\b(btn|button)\b/i.test(String(element.className).replace(/[_-]+/g, ' '))
    );
    const unnamedButton = !name && (
      element.tagName === 'BUTTON'
        || (element.tagName === 'INPUT' && ['button', 'submit', 'reset'].includes(String(element.type || '').toLowerCase()))
        || String(element.getAttribute('role') || '').toLowerCase() === 'button'
    );
    if (!bareUploadVerb && !authoredUploadButton && !unnamedButton) return false;
    let context = element.parentElement;
    for (let depth = 0; context && context !== element.ownerDocument.body && depth < 4; depth += 1, context = context.parentElement) {
      const text = normalizedText(context.innerText || context.textContent, 768) || '';
      if (/\b(drop|select|choose|upload|add)\b.{0,120}\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b/i.test(text)
        || /\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b.{0,120}\b(drop|select|choose|upload|add)\b/i.test(text)
        || (authoredUploadButton && /\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b/i.test(text))) return true;
    }
    return false;
  }

  function objectId(element) {
    // object_id is document-local authority. Every Agent request already has
    // to carry the exact document_id, so repeating that high-entropy value in
    // every object wastes model context without adding isolation.
    if (!identities.has(element)) identities.set(element, `object.${++objectSerial}`);
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

  function visibleRadioTrigger(input) {
    if (input.tagName !== 'INPUT' || String(input.type).toLowerCase() !== 'radio') return input;
    const inputVisibility = visibilityFor(input, boxFor(input));
    if (inputVisibility === 'visible') return input;
    const visible = (element) => visibilityFor(element, boxFor(element)) === 'visible';
    return Array.from(input.labels || []).find(visible) || input;
  }

  function ariaBoolean(element, name) {
    const value = element.getAttribute(`aria-${name}`);
    return value === null ? undefined : value === 'true';
  }

  function navigationTargetFor(element, role) {
    if (role !== 'link') return undefined;
    try {
      const target = new URL(element.getAttribute('href'), element.ownerDocument.baseURI);
      if (!['http:', 'https:'].includes(target.protocol)
        || target.username || target.password || target.href.length > 8192) return undefined;
      return target.href;
    } catch (_) {
      return undefined;
    }
  }

  function signalsFor(element, role) {
    const signals = { enabled: !element.disabled && element.getAttribute('aria-disabled') !== 'true' };
    if (role === 'reflex_target') {
      const page = element.ownerDocument;
      if (element === page.body && isMouseAccuracyGame(page)) signals.enabled = false;
      const authored = element.getAttribute('data-saccade-reflex-occurrence');
      signals.occurrence = authored || (isMouseAccuracyClassic(page)
        ? String(mouseAccuracyHitOccurrence)
        : reflexOccurrence(authored, isMouseAccuracyGame(page) ? page.body?.innerText : ''));
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
      const choiceOptions = optionsForChoice(element);
      const selectedOption = choiceOptions.some((option) => option.getAttribute('aria-selected') === 'true');
      if (choiceOptions.length) choiceHasValue.set(element, selectedOption);
      signals.hasValue = element.tagName === 'SELECT'
        ? element.selectedIndex >= 0
        : selectedOption
          || (choiceOptions.length === 0 && choiceHasValue.get(element) === true)
          || (element.getAttribute('role') === 'combobox'
            && ariaBoolean(element, 'expanded') === false
            && Boolean(element.getAttribute('aria-activedescendant')));
      signals.required = Boolean(element.required) || element.getAttribute('aria-required') === 'true';
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
      signals.expanded = ariaBoolean(element, 'expanded') ?? element.getAttribute('role') === 'listbox';
      signals.expandable = element.tagName !== 'SELECT'
        && element.getAttribute('role') === 'combobox'
        && signals.expanded === false;
    } else if (role === 'content_editable') {
      signals.hasValue = Boolean(normalizedText(element.textContent, 1));
      signals.readonly = element.getAttribute('aria-readonly') === 'true';
    } else {
      signals.hasValue = Boolean(element.value);
      signals.required = Boolean(element.required);
      signals.readonly = Boolean(element.readOnly);
      signals.invalid = element.getAttribute('aria-invalid') === 'true';
      const semanticHint = [
        element.name, element.id, element.getAttribute('aria-label'), element.getAttribute('placeholder'),
        ...Array.from(element.labels || []).map((label) => label.textContent),
      ].filter(Boolean).join(' ');
      signals.protected = isProtectedFieldType(element.type, element.autocomplete, semanticHint);
    }
    if (role === 'generic_control') signals.affordance = element.getAttribute('data-saccade-affordance');
    return signals;
  }

  function observationObject(element, role, frameId) {
    const descriptor = registry.observe(role, signalsFor(element, role));
    const interactionElement = role === 'file_input'
      ? visibleFileTrigger(element)
      : role === 'radio' ? visibleRadioTrigger(element) : element;
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
    const navigationTarget = navigationTargetFor(element, role);
    if (navigationTarget) object.navigation_target = navigationTarget;
    // A link that downloads, or that opens a new browsing context, does not
    // change this document's URL, so Saccade cannot verify it from Truth.
    // Declaring the disposition lets execution hand off explicitly instead of
    // dispatching and then reporting an unverifiable result.
    if (navigationTarget) {
      const disposition = element.hasAttribute('download')
        ? 'download'
        : (() => {
          const target = String(element.getAttribute('target') || '').trim().toLowerCase();
          return target && target !== '_self' ? 'new_context' : 'self';
        })();
      if (disposition !== 'self') object.navigation_disposition = disposition;
    }
    if (role === 'reflex_target') object.loop_class_token = reflexLoopClassToken;
    if (descriptor.affordances.length && interactionElement.ownerDocument.defaultView.getComputedStyle(interactionElement).pointerEvents !== 'none') {
      const token = randomToken('action', 16);
      object.action_token = token;
      tokenTargets.set(token, {
        element: interactionElement, controlElement: element, role, objectId: id, affordances: descriptor.affordances,
        authorityFingerprint: authorityFingerprint(object),
      });
    }
    return object;
  }

  function comboboxForListbox(listbox) {
    if (!listbox.id) return null;
    for (const candidate of composedQuery(listbox.ownerDocument, '[role="combobox"][aria-controls],[role="combobox"][aria-owns]')) {
      const ids = `${candidate.getAttribute('aria-controls') || ''} ${candidate.getAttribute('aria-owns') || ''}`.split(/\s+/);
      if (ids.includes(listbox.id)) {
        const priorOwner = rememberedChoiceOwner.get(listbox);
        if (priorOwner && priorOwner !== candidate && rememberedChoicePopup.get(priorOwner) === listbox) {
          rememberedChoicePopup.delete(priorOwner);
        }
        rememberedChoiceOwner.set(listbox, candidate);
        rememberedChoicePopup.set(candidate, listbox);
        return candidate;
      }
    }
    const remembered = rememberedChoiceOwner.get(listbox);
    if (remembered?.isConnected && remembered.ownerDocument === listbox.ownerDocument) return remembered;
    rememberedChoiceOwner.delete(listbox);
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
    const remembered = rememberedChoicePopup.get(owner);
    if (!roots.length && remembered?.isConnected && remembered.ownerDocument === owner.ownerDocument) roots.push(remembered);
    else if (remembered && (!remembered.isConnected || remembered.ownerDocument !== owner.ownerDocument)) rememberedChoicePopup.delete(owner);
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
    const enabled = optionEnabled(option, owner);
    const clickable = option.matches('[role="option"]');
    const selected = option.tagName === 'OPTION' ? option.selected : option.getAttribute('aria-selected') === 'true';
    const descriptor = { ...registry.option(name || '', selected, enabled, clickable) };
    if (!name) delete descriptor.name;
    // Native <option> geometry is browser-owned and often empty, so it remains
    // an alias used by the parent select operation. An ARIA option is a real
    // rendered target and can be clicked generically without a framework- or
    // site-specific selector.
    const interactionElement = clickable ? option : owner;
    const box = boxFor(interactionElement);
    const id = objectId(option);
    objectTargets.set(id, option);
    const object = {
      object_id: id, object_revision: revision + 1, frame_id: frameId, ...descriptor,
      document_bounds: { x: box.x + interactionElement.ownerDocument.defaultView.scrollX, y: box.y + interactionElement.ownerDocument.defaultView.scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility: visibilityFor(interactionElement, box), transition: 'none',
    };
    if (descriptor.affordances.length && interactionElement.ownerDocument.defaultView.getComputedStyle(interactionElement).pointerEvents !== 'none') {
      const token = randomToken('action', 16);
      object.action_token = token;
      tokenTargets.set(token, {
        element: interactionElement, role: 'option', objectId: id, affordances: descriptor.affordances,
        authorityFingerprint: authorityFingerprint(object),
      });
    }
    return object;
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
    if (role === 'text') return 'text';
    if (role === 'heading') return 'heading';
    if (role === 'paragraph') return 'paragraph';
    if (role === 'list') return 'list';
    if (role === 'listitem') return 'list_item';
    if (role === 'table') return 'table';
    if (role === 'row') return 'row';
    if (['cell', 'columnheader', 'rowheader'].includes(role)) return 'cell';
    if (role === 'alert') return 'alert';
    if (role === 'status') return 'status';
    if (element.hasAttribute('aria-live')) return 'status';
    if (/^H[1-6]$/.test(tag)) return 'heading';
    if (tag === 'P') return 'paragraph';
    if (tag === 'UL' || tag === 'OL') return 'list';
    if (tag === 'LI') return 'list_item';
    if (tag === 'TABLE') return 'table';
    if (tag === 'TR') return 'row';
    if (tag === 'TH' || tag === 'TD') return 'cell';
    return null;
  }

  function surfaceObject(element, frameId) {
    const box = boxFor(element);
    const visibility = visibilityFor(element, box);
    if (visibility === 'hidden') return null;
    const restricted = element.matches('embed[type="application/pdf"],object[type="application/pdf"],[data-saccade-restricted-document]');
    const id = objectId(element);
    const role = restricted ? 'restricted_document' : 'opaque_surface';
    const kind = role;
    const object = {
      object_id: id, object_revision: revision + 1, frame_id: frameId,
      kind, role, state: {}, affordances: [], protected: false,
      document_bounds: { x: box.x + element.ownerDocument.defaultView.scrollX, y: box.y + element.ownerDocument.defaultView.scrollY, width: box.width, height: box.height },
      viewport_bounds: box, visibility, transition: 'none',
    };
    const name = safeName(element, role);
    if (name) object.name = name;
    let limitation = 'opaque_canvas';
    if (element.tagName === 'VIDEO') limitation = 'opaque_video';
    else if (element.tagName === 'CANVAS' && element.getAttribute('data-saccade-context') === 'webgl') limitation = 'opaque_webgl';
    else if (restricted) limitation = element.hasAttribute('data-saccade-restricted-document') ? 'browser_restricted_page' : 'built_in_pdf';
    return { object, limitation: { kind: limitation, frame_id: frameId, object_id: id } };
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
    const text = forcedText || (role ? structuralText(element) : undefined)
      || (['list', 'table', 'row'].includes(role) ? safeName(element, role) : undefined);
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
      if (forcedRole === 'heading' && element.matches(DIALOG_SELECTOR)) {
        state.modal = String(element.getAttribute('aria-modal') === 'true');
      }
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

  function dialogTextCandidates(doc) {
    const candidates = [];
    for (const dialog of composedQuery(doc, DIALOG_SELECTOR)) {
      const dialogBox = boxFor(dialog);
      if (visibilityFor(dialog, dialogBox) === 'hidden') continue;
      for (const element of composedQuery(dialog, GENERIC_TEXT_SELECTOR)) {
        if (element.matches(`${CONTROL_SELECTOR},${IMAGE_SELECTOR},${STRUCTURAL_SELECTOR},${DIALOG_SELECTOR}`)) continue;
        if (element.parentElement?.closest(STRUCTURAL_SELECTOR)) continue;
        if (visibilityFor(element, boxFor(element)) === 'hidden') continue;
        const text = structuralText(element);
        if (text) candidates.push({ element, text });
      }
    }
    return candidates.filter(({ element }) => !candidates.some(({ element: descendant }) => (
      descendant !== element && element.contains(descendant)
    )));
  }

  function genericTextCandidates(doc) {
    const candidates = [];
    for (const element of composedQuery(doc, GENERIC_TEXT_SELECTOR)) {
      if (element.matches(`${CONTROL_SELECTOR},${IMAGE_SELECTOR},${STRUCTURAL_SELECTOR},${DIALOG_SELECTOR}`)) continue;
      if (element.closest(`${DIALOG_SELECTOR},${STRUCTURAL_SELECTOR}`)) continue;
      if (visibilityFor(element, boxFor(element)) === 'hidden') continue;
      const text = structuralText(element);
      if (text) candidates.push({ element, text });
    }
    return candidates.filter(({ element }) => !candidates.some(({ element: descendant }) => (
      descendant !== element && element.contains(descendant)
    )));
  }

  function isMouseAccuracyGame(doc) {
    return doc?.location?.hostname === 'mouseaccuracy.com'
      && (doc.location.pathname.startsWith('/game') || doc.location.pathname.startsWith('/classic'));
  }

  function isMouseAccuracyClassic(doc) {
    return doc?.location?.hostname === 'mouseaccuracy.com' && doc.location.pathname.startsWith('/classic');
  }

  function updateMouseAccuracyHitOccurrence(doc) {
    if (!isMouseAccuracyClassic(doc)) return;
    for (const hit of doc.querySelectorAll('.target.hit')) {
      recordMouseAccuracyOccurrence(hit);
    }
  }

  function recordMouseAccuracyOccurrence(element, softwareDispatchCompleted = false) {
    if (!isMouseAccuracyClassic(element?.ownerDocument) || seenMouseAccuracyHits.has(element)) return false;
    // Classic keeps its live score in page-private JavaScript and exposes it
    // only on the final result screen. Its dedicated bridge therefore counts
    // the completed exact-target click dispatch as the per-step occurrence;
    // a retained `.hit` class or synchronous disconnection remains a stronger
    // page-produced signal when either is available. This is never a reason to
    // replay an ambiguous dispatch.
    if (!softwareDispatchCompleted && element.isConnected && !element.classList.contains('hit')) return false;
    seenMouseAccuracyHits.add(element);
    mouseAccuracyHitOccurrence += 1;
    return true;
  }

  function observeCurrentGeometry() {
    if (!globalThis.ResizeObserver) return;
    if (!geometryResizeObserver) geometryResizeObserver = new ResizeObserver(scheduleVisual);
    const currentElements = new Set([
      ...[...objectTargets.values()].filter((element) => element?.isConnected),
      ...[...tokenTargets.values()].map((target) => target.element).filter((element) => element?.isConnected),
    ]);
    for (const element of geometryObservedElements) {
      if (!currentElements.has(element)) geometryResizeObserver.unobserve(element);
    }
    for (const element of currentElements) {
      if (!geometryObservedElements.has(element)) geometryResizeObserver.observe(element);
    }
    geometryObservedElements = currentElements;
  }

  function currentGeometryIsAnimating() {
    const visited = new Set();
    const currentElements = new Set([
      ...objectTargets.values(),
      ...[...tokenTargets.values()].map((target) => target.element),
    ]);
    for (const element of currentElements) {
      let current = element;
      while (current?.nodeType === Node.ELEMENT_NODE) {
        if (!visited.has(current)) {
          visited.add(current);
          if (current.getAnimations?.().some((animation) => animation.playState === 'running')) return true;
        }
        const root = current.getRootNode?.();
        current = current.parentElement || root?.host || null;
      }
    }
    return false;
  }

  function continueGeometryTracking() {
    observeCurrentGeometry();
    if (currentGeometryIsAnimating()) scheduleVisual();
  }

  function authorityFingerprint(object) {
    const contract = { ...object };
    delete contract.action_token;
    delete contract.object_revision;
    delete contract.document_bounds;
    delete contract.viewport_bounds;
    // Visibility and transition are local actionability inputs, not semantic
    // authority identity. Scrolling the same live element into view must not
    // rotate its token; prepare() rechecks both immediately before dispatch.
    delete contract.visibility;
    delete contract.transition;
    return JSON.stringify(contract);
  }

  function reuseStableAuthorities(objects, previousTokenTargets) {
    const previousByObject = new Map();
    for (const [token, target] of previousTokenTargets) {
      previousByObject.set(target.objectId, { token, target });
    }
    for (const object of objects) {
      if (!object.action_token) continue;
      const currentToken = object.action_token;
      const current = tokenTargets.get(currentToken);
      const previous = previousByObject.get(object.object_id);
      if (!current || !previous
        || previous.target.element !== current.element
        || previous.target.role !== current.role
        || previous.target.affordances.join('\u0000') !== current.affordances.join('\u0000')
        || previous.target.authorityFingerprint !== current.authorityFingerprint) continue;
      tokenTargets.delete(currentToken);
      object.action_token = previous.token;
      tokenTargets.set(previous.token, current);
    }
  }

  function collect({ forceSnapshot = false } = {}) {
    if (!config) return null;
    updateMouseAccuracyHitOccurrence(document);
    const hadCompiledObjects = compiledObjects !== null;
    const previousTokenTargets = new Map(tokenTargets);
    const previousObjectTargets = new Map(objectTargets);
    tokenTargets.clear();
    objectTargets.clear();
    const frameState = collectFrameContexts();
    for (const context of frameState.contexts) observeDocument(context.doc);
    const objects = [];
    const surfaceLimitations = [];
    const seenFileTriggers = new Set();
    let truncated = false;
    if (isMouseAccuracyGame(document) && document.body) {
      const loopStatus = observationObject(document.body, 'reflex_target', config.frameId);
      if (loopStatus) objects.push(loopStatus);
    }
    const candidates = frameState.contexts.flatMap((context) => (
      composedQuery(context.doc, CONTROL_SELECTOR).map((element) => ({ element, frameId: context.frameId }))
    ));
    const actionNameCounts = new Map();
    for (const { element } of candidates) {
      const role = roleFor(element);
      if (!role || !registry.observe(role, signalsFor(element, role)).affordances.length) continue;
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
        for (const element of composedQuery(context.doc, SURFACE_SELECTOR)) {
          const surface = surfaceObject(element, context.frameId);
          if (!surface) continue;
          objects.push(surface.object);
          surfaceLimitations.push(surface.limitation);
        }
      }
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
        const dialogTexts = dialogTextCandidates(context.doc)
          .map(({ element, text }) => ({ element, dialogText: true, text }));
        const genericTexts = genericTextCandidates(context.doc)
          .map(({ element, text }) => ({ element, genericText: true, text }));
        const seenStructural = new Set();
        for (const { element, dialogTitle, dialogText, genericText, text } of [...ordinary, ...dialogTitles, ...dialogTexts, ...genericTexts]) {
          if (seenStructural.has(element)) continue;
          seenStructural.add(element);
          const projected = structuralObject(
            element, context.frameId, dialogTitle ? 'heading' : (dialogText || genericText) ? 'text' : undefined, text,
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
    // Authority is bound to the stable object identity and exact current DOM
    // element, role, and affordances. Do not wait for document readiness: live
    // applications can intentionally keep the document in "loading" while
    // their controls are already usable. Per-action local preflight owns
    // connectedness, visibility, enabledness, geometry, and topmost checks.
    // Replacement, role/affordance changes, navigation, and disconnect still
    // invalidate authority immediately.
    reuseStableAuthorities(objects, previousTokenTargets);
    const changes = compileChanges(compiledObjects, objects);
    // Bounds come from getBoundingClientRect, so the layout viewport is the
    // space they are expressed in. device_pixel_ratio is descriptive only.
    const geometry = {
      unit: 'css_px',
      coordinate_space: 'content_viewport',
      viewport_width: document.documentElement.clientWidth,
      viewport_height: document.documentElement.clientHeight,
      scroll_x: Math.round(scrollX),
      scroll_y: Math.round(scrollY),
      device_pixel_ratio: devicePixelRatio,
    };
    const geometryFingerprint = [geometry.viewport_width, geometry.viewport_height,
      geometry.scroll_x, geometry.scroll_y, geometry.device_pixel_ratio].join('\u0000');
    const geometryChanged = geometryFingerprint !== lastGeometryFingerprint;
    const urlFingerprint = frameState.frames
      .map((frame) => `${frame.frame_id}\u0000${frame.document_url || ''}`).join('\u0001');
    const unchanged = hadCompiledObjects && changes.length === 0
      && urlFingerprint === lastUrlFingerprint && !geometryChanged;
    if (unchanged && !forceSnapshot) {
      tokenTargets.clear();
      objectTargets.clear();
      for (const [token, target] of previousTokenTargets) {
        if (target.element.isConnected) tokenTargets.set(token, target);
      }
      for (const [id, element] of previousObjectTargets) {
        if (element.isConnected) objectTargets.set(id, element);
      }
      continueGeometryTracking();
      return null;
    }
    if (!unchanged) revision += 1;
    if (!unchanged && geometryChanged) viewportRevision += 1;
    lastGeometryFingerprint = geometryFingerprint;
    lastUrlFingerprint = urlFingerprint;
    const snapshot = {
      schema: OBSERVATION_SCHEMA, browser_instance_id: config.browserInstanceId,
      tab_id: config.tabId, document_id: documentId, revision, viewport_revision: viewportRevision,
      frames: frameState.frames,
      geometry: { ...geometry, viewport_revision: viewportRevision },
      objects, changes: hadCompiledObjects && !forceSnapshot ? changes : [], coverage: {
        source: 'dom_extension',
        observed_frame_count: frameState.frames.filter((frame) => frame.status === 'observed').length,
        restricted_frame_count: frameState.frames.filter((frame) => frame.status !== 'observed').length,
        truncated,
      },
      limitations: [...frameState.limitations, ...surfaceLimitations, ...(truncated ? [{ kind: 'truncated', frame_id: config.frameId }] : [])], gap: false,
    };
    compiledObjects = objects;
    continueGeometryTracking();
    connectWorkerPort();
    if (!hadCompiledObjects || forceSnapshot) {
      workerPort?.postMessage({ kind: 'collector.observation', payload: snapshot });
    } else {
      const compact = compactTransport(objects, changes);
      workerPort?.postMessage({ kind: 'collector.observation_delta', payload: {
        browser_instance_id: config.browserInstanceId,
        tab_id: config.tabId,
        document_id: documentId,
        base_revision: revision - 1,
        revision,
        viewport_revision: viewportRevision,
        frames: frameState.frames,
        geometry: { ...geometry, viewport_revision: viewportRevision },
        objects: compact.objects,
        changes,
        authorities: compact.authorities,
        coverage: snapshot.coverage,
        limitations: snapshot.limitations,
      } });
    }
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

  function contentScreenOrigin() {
    // outerWidth includes both side borders. The remaining vertical chrome is
    // the toolbar plus the matching bottom border, so subtract one side border
    // to locate the content viewport instead of the outer window frame.
    const sideBorder = Math.max(0, (outerWidth - innerWidth) / 2);
    return {
      x: screenX + sideBorder,
      y: screenY + Math.max(0, outerHeight - innerHeight - sideBorder),
    };
  }

  function prepare(request) {
    if (!config || request.browser_instance_id !== config.browserInstanceId || request.tab_id !== config.tabId
      || request.document_id !== documentId || request.basis_revision !== revision) {
      throw actionFailure('prepare', 'stale_action_basis', true, 'stale action basis');
    }
    const target = tokenTargets.get(request.action_token);
    if (!target || !target.element.isConnected || !target.affordances.includes(request.operation)) {
      throw actionFailure('prepare', 'stale_action_token', true, 'action token is not current for operation');
    }
    // A preflight may defer scrolling. The dispatch pass scrolls and acts in
    // the same task while identity, token, document, revision, and affordance
    // checks remain mandatory.
    const deferred = request.defer_scroll === true;
    const focusElement = target.controlElement || target.element;
    if (!deferred) {
      target.element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'instant' });
      if (request.operation === 'type' || request.operation === 'select') {
        focusElement.focus({ preventScroll: true });
      }
    }
    const box = boxFor(target.element);
    const topBox = topViewportBox(target.element, box);
    const screenOrigin = contentScreenOrigin();
    const prepared = {
      browser_instance_id: config.browserInstanceId, tab_id: config.tabId, document_id: documentId,
      basis_revision: revision, viewport_revision: viewportRevision, object_id: target.objectId,
      action_token: request.action_token, operation: request.operation,
      screen_bounds: { x: screenOrigin.x + topBox.x, y: screenOrigin.y + topBox.y, width: topBox.width, height: topBox.height },
      visible: visibilityFor(target.element, box) === 'visible', topmost: isTopmost(target.element, box),
      focus_verified: target.role === 'reflex_target'
        || focusElement.ownerDocument.activeElement === focusElement,
    };
    if (request.operation === 'select') {
      const optionId = request.payload?.kind === 'select' ? request.payload.option_object_id : '';
      const option = objectTargets.get(optionId);
      const owner = option ? choiceOwner(option) : null;
      const choices = owner ? optionsForChoice(owner).filter((item) => optionEnabled(item, owner)) : [];
      if (!option || !option.matches('option,[role="option"]') || owner !== target.element || !optionEnabled(option, owner)) {
        throw actionFailure('prepare', 'select_option_not_current', false, 'select option is not bound and enabled for this control');
      }
      prepared.selection_index = choices.indexOf(option);
      if (prepared.selection_index < 0) {
        throw actionFailure('prepare', 'select_option_not_current', false, 'select option has no enabled option position');
      }
    }
    return prepared;
  }

  function actionFailure(stage, code, retrySafe, message) {
    const error = new Error(message);
    error.saccadeStage = stage;
    error.saccadeCode = code;
    error.saccadeRetrySafe = retrySafe;
    return error;
  }

  function targetEnabled(target) {
    const control = target.controlElement || target.element;
    return !control.disabled && control.getAttribute('aria-disabled') !== 'true';
  }

  function targetGeometryIsAnimating(element) {
    let current = element;
    while (current?.nodeType === Node.ELEMENT_NODE) {
      if (current.getAnimations?.().some((animation) => animation.playState === 'running')) return true;
      const root = current.getRootNode?.();
      current = current.parentElement || root?.host || null;
    }
    return false;
  }

  function softwarePreparationPolicy(request, target) {
    const reflexClick = request.operation === 'click' && target.role === 'reflex_target';
    const focusRequired = request.operation === 'type' || request.operation === 'select';
    return {
      // Software reflex input targets this exact authorized DOM object. It
      // does not aim a physical pointer, so continuous movement, browser
      // focus, and coordinate hit testing are not preparation gates.
      require_topmost: !reflexClick,
      require_focus: focusRequired,
      require_stable_geometry: !reflexClick,
    };
  }

  function preparationIssue(prepared, target, policy) {
    if (!prepared.visible) return 'not_visible';
    if (policy.require_topmost && !prepared.topmost) return 'not_topmost';
    if (policy.require_focus && !prepared.focus_verified) return 'focus_not_verified';
    if (!targetEnabled(target)) return 'not_enabled';
    return null;
  }

  function currentSoftwareRequest(request) {
    if (request.basis_revision === revision) return request;
    const target = tokenTargets.get(request.action_token);
    if (request.basis_revision < revision
      && request.document_id === documentId
      && target?.element.isConnected
      && target.objectId === request.object_id
      && target.affordances.includes(request.operation)) {
      return { ...request, basis_revision: revision };
    }
    return request;
  }

  function waitForPreparationFrame(deadline) {
    const remainingMs = Math.max(0, deadline - performance.now());
    return new Promise((resolve) => {
      let settled = false;
      let timerId;
      const finish = (frameObserved) => {
        if (settled) return;
        settled = true;
        clearTimeout(timerId);
        resolve(frameObserved);
      };
      const frameId = requestAnimationFrame(() => finish(true));
      timerId = setTimeout(() => {
        cancelAnimationFrame(frameId);
        finish(false);
      }, remainingMs);
    });
  }

  function sameBox(left, right) {
    return Boolean(left && right
      && left.x === right.x && left.y === right.y
      && left.width === right.width && left.height === right.height);
  }

  async function waitForSoftwarePreparation(request) {
    const startedAt = performance.now();
    let activeRequest = currentSoftwareRequest(request);
    let prepared = prepare(activeRequest);
    let target = tokenTargets.get(request.action_token);
    const policy = softwarePreparationPolicy(request, target);
    if (!preparationIssue(prepared, target, policy)
      && (!policy.require_stable_geometry || !targetGeometryIsAnimating(target.element))) {
      prepared.local_wait_ms = 0;
      return prepared;
    }
    const timeoutMs = Math.max(1, Math.min(30000, Number(request.timeout_ms) || 5000));
    const deadline = performance.now() + timeoutMs;
    let previousBox = null;
    let stableFrames = 0;
    let lastIssue = preparationIssue(prepared, target, policy)
      || (policy.require_stable_geometry ? 'geometry_unstable' : null);
    while (performance.now() < deadline) {
      if (!await waitForPreparationFrame(deadline)) break;
      // Recompile locally before revalidation. If identity, semantic authority,
      // document, or token changed, the old token is absent and prepare()
      // fails stale instead of rebinding. Geometry-only revisions may rebase
      // this private dispatch preparation; the public object identity and
      // action token never change.
      collect();
      activeRequest = currentSoftwareRequest(activeRequest);
      prepared = prepare(activeRequest);
      target = tokenTargets.get(request.action_token);
      lastIssue = preparationIssue(prepared, target, policy);
      const box = prepared.screen_bounds;
      stableFrames = sameBox(previousBox, box) ? stableFrames + 1 : 1;
      previousBox = box;
      if (!lastIssue && (!policy.require_stable_geometry || stableFrames >= 2)) {
        prepared.local_wait_ms = Math.max(0, performance.now() - startedAt);
        return prepared;
      }
    }
    throw actionFailure(
      'prepare',
      `actionability_timeout_${lastIssue || 'geometry_unstable'}`,
      true,
      `software actionability timed out: ${lastIssue || 'geometry_unstable'}`,
    );
  }

  async function softClick(request, preflight) {
    const prepared = preflight || await waitForSoftwarePreparation(request);
    const target = tokenTargets.get(request.action_token);
    if (!target || !target.element.isConnected || !SOFTWARE_CLICK_ROLES.has(target.role)) {
      throw actionFailure('dispatch', 'operation_not_registered', false, 'software click is not registered for the current control');
    }
    const box = boxFor(target.element);
    const clientX = box.x + box.width / 2;
    const clientY = box.y + box.height / 2;
    let clickDispatchCompleted = false;
    for (const [type, EventClass, buttons] of [
      ['pointermove', PointerEvent, 0], ['mousemove', MouseEvent, 0],
      ['pointerdown', PointerEvent, 1], ['mousedown', MouseEvent, 1],
      ['pointerup', PointerEvent, 0], ['mouseup', MouseEvent, 0], ['click', MouseEvent, 0],
    ]) {
      target.element.dispatchEvent(new EventClass(type, {
        bubbles: true, cancelable: true, composed: true, clientX, clientY,
        button: 0, buttons, pointerId: 1, pointerType: 'mouse', isPrimary: true,
      }));
      if (type === 'click') clickDispatchCompleted = true;
    }
    const recordedReflexOccurrence = target.role === 'reflex_target'
      && recordMouseAccuracyOccurrence(target.element, clickDispatchCompleted);
    if (recordedReflexOccurrence) collect();
    requestAnimationFrame(collect);
    return {
      accepted: true, local_wait_ms: prepared.local_wait_ms,
      dispatch_document_id: prepared.document_id,
      dispatch_basis_revision: prepared.basis_revision,
    };
  }

  async function softType(request, preflight) {
    const prepared = preflight || await waitForSoftwarePreparation(request);
    const target = tokenTargets.get(request.action_token);
    if (!target || !target.element.isConnected || !SOFTWARE_TYPE_ROLES.has(target.role)) {
      throw actionFailure('dispatch', 'operation_not_registered', false, 'software typing is not registered for the current control');
    }
    const element = target.element;
    const text = String(request.payload?.text ?? '');
    // The generic editing sequence a real edit produces, in order. No control is
    // special-cased and no framework is detected: a page listening for any of
    // these sees the same order it would see from a person. prepare() already
    // focused the element for the 'type' operation, so this does not repeat it.
    const proceed = element.dispatchEvent(new InputEvent('beforeinput', {
      bubbles: true, cancelable: true, composed: true, inputType: 'insertText', data: text,
    }));
    if (!proceed) throw actionFailure('dispatch', 'page_canceled_beforeinput', false, 'software type was canceled by the page');
    if (element.isContentEditable) {
      element.textContent = text;
    } else {
      // Assign through the prototype setter. A framework tracking a controlled
      // value installs its own accessor on the element, so assigning the
      // property directly is swallowed and the framework never re-renders.
      const prototype = element instanceof HTMLTextAreaElement
        ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
      const setter = Object.getOwnPropertyDescriptor(prototype, 'value')?.set;
      if (setter) setter.call(element, text); else element.value = text;
    }
    element.dispatchEvent(new InputEvent('input', {
      bubbles: true, composed: true, inputType: 'insertText', data: text,
    }));
    element.dispatchEvent(new Event('change', { bubbles: true }));
    requestAnimationFrame(collect);
    return {
      accepted: true, local_wait_ms: prepared.local_wait_ms,
      dispatch_document_id: prepared.document_id,
      dispatch_basis_revision: prepared.basis_revision,
    };
  }

  function linkedFileInput(target) {
    const direct = [target.controlElement, target.element].find((element) => (
      element?.tagName === 'INPUT' && String(element.type).toLowerCase() === 'file'
    ));
    if (direct) return direct;
    const trigger = target.element;
    const ids = String(trigger.getAttribute('aria-controls') || '').split(/\s+/).filter(Boolean);
    for (const id of ids) {
      const candidate = trigger.ownerDocument.getElementById(id);
      if (candidate?.tagName === 'INPUT' && String(candidate.type).toLowerCase() === 'file') return candidate;
    }
    return null;
  }

  function dispatchUploadTriggerClick(trigger) {
    const view = trigger.ownerDocument.defaultView;
    const box = boxFor(trigger);
    const clientX = box.x + box.width / 2;
    const clientY = box.y + box.height / 2;
    for (const [type, EventClass, buttons] of [
      ['pointermove', view.PointerEvent, 0], ['mousemove', view.MouseEvent, 0],
      ['pointerdown', view.PointerEvent, 1], ['mousedown', view.MouseEvent, 1],
      ['pointerup', view.PointerEvent, 0], ['mouseup', view.MouseEvent, 0], ['click', view.MouseEvent, 0],
    ]) {
      const event = new EventClass(type, {
        bubbles: true, cancelable: true, composed: true, clientX, clientY,
        button: 0, buttons, pointerId: 1, pointerType: 'mouse', isPrimary: true,
      });
      if (type !== 'click') {
        trigger.dispatchEvent(event);
        continue;
      }
      // Let the trigger's already-registered page listeners observe an
      // uncancelled click first. Cancel at the end of the target phase so the
      // synthetic bridge does not also submit an empty form or open a native
      // chooser. Pre-cancelling the event makes some generic upload widgets
      // abandon their own file-input setup.
      const preventNativeDefault = (clickEvent) => clickEvent.preventDefault();
      trigger.addEventListener('click', preventNativeDefault, { once: true });
      trigger.dispatchEvent(event);
      trigger.removeEventListener('click', preventNativeDefault);
    }
  }

  async function captureDynamicFileInput(trigger) {
    const document = trigger.ownerDocument;
    let captured = null;
    let resolveCapture;
    const capture = new Promise((resolve) => { resolveCapture = resolve; });
    const use = (candidate) => {
      if (captured || candidate?.tagName !== 'INPUT' || String(candidate.type).toLowerCase() !== 'file') return;
      captured = candidate;
      resolveCapture(candidate);
    };
    const onClick = (event) => {
      const candidate = event.target;
      if (candidate?.tagName !== 'INPUT' || String(candidate.type).toLowerCase() !== 'file') return;
      // The authorized upload supplies the FileList itself. Suppress only the
      // native chooser default; page click/change listeners still run.
      event.preventDefault();
      use(candidate);
    };
    const observer = new MutationObserver((records) => {
      for (const record of records) {
        for (const node of record.addedNodes) {
          if (node.nodeType !== Node.ELEMENT_NODE) continue;
          use(node.matches?.('input[type="file"]') ? node : node.querySelector?.('input[type="file"]'));
        }
      }
    });
    // Observe after the input's own target listeners. They must see an
    // uncancelled click, while this bubble listener still suppresses the
    // native chooser default action.
    document.addEventListener('click', onClick);
    observer.observe(document.documentElement, { subtree: true, childList: true });
    try {
      dispatchUploadTriggerClick(trigger);
      return await Promise.race([
        capture,
        new Promise((resolve) => setTimeout(() => resolve(null), 1000)),
      ]);
    } finally {
      document.removeEventListener('click', onClick);
      observer.disconnect();
    }
  }

  function uploadFileFromPayload(payload, view) {
    const source = payload?.kind === 'file' ? payload.file : null;
    if (!source || typeof source.name !== 'string' || !source.name
      || source.name.length > 255 || /[\\/\u0000-\u001f\u007f]/.test(source.name)
      || typeof source.mime_type !== 'string' || source.mime_type.length > 127
      || !Number.isSafeInteger(source.size_bytes) || source.size_bytes < 1
      || source.size_bytes > MAX_UPLOAD_BYTES || typeof source.content_base64 !== 'string'
      || source.content_base64.length > Math.ceil(MAX_UPLOAD_BYTES / 3) * 4 + 4) {
      throw actionFailure('prepare', 'upload_payload_invalid', true, 'upload payload is invalid');
    }
    let binary;
    try { binary = view.atob(source.content_base64); }
    catch (_error) {
      throw actionFailure('prepare', 'upload_payload_invalid', true, 'upload payload encoding is invalid');
    }
    if (binary.length !== source.size_bytes) {
      throw actionFailure('prepare', 'upload_payload_invalid', true, 'upload payload size does not match');
    }
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) bytes[index] = binary.charCodeAt(index);
    return new view.File([bytes], source.name, { type: source.mime_type, lastModified: Date.now() });
  }

  function fileAcceptedByInput(file, input) {
    const accepts = String(input.accept || '').split(',').map((value) => value.trim().toLowerCase()).filter(Boolean);
    if (!accepts.length) return true;
    const name = file.name.toLowerCase();
    const type = file.type.toLowerCase();
    return accepts.some((accept) => (
      accept.startsWith('.') ? name.endsWith(accept)
        : accept.endsWith('/*') ? type.startsWith(accept.slice(0, -1))
          : type === accept
    ));
  }

  function uploadDropTarget(trigger) {
    let context = trigger;
    for (let depth = 0; context && context !== trigger.ownerDocument.body && depth < 5; depth += 1, context = context.parentElement) {
      const text = normalizedText(context.innerText || context.textContent, 1024) || '';
      if (/\bdrag\s+(?:and\s+)?drop\b.{0,160}\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b/i.test(text)
        || /\b(files?|documents?|attachments?|images?|covers?|screenshots?)\b.{0,160}\bdrag\s+(?:and\s+)?drop\b/i.test(text)) return context;
    }
    return null;
  }

  function dispatchFileDrop(trigger, file) {
    const view = trigger.ownerDocument.defaultView;
    const transfer = new view.DataTransfer();
    transfer.items.add(file);
    for (const type of ['dragenter', 'dragover', 'drop']) {
      trigger.dispatchEvent(new view.DragEvent(type, {
        bubbles: true, cancelable: true, composed: true, dataTransfer: transfer,
      }));
    }
  }

  async function softUpload(request, preflight) {
    const prepared = preflight || await waitForSoftwarePreparation(request);
    const target = tokenTargets.get(request.action_token);
    if (!target || !target.element.isConnected || target.role !== 'file_input') {
      throw actionFailure('dispatch', 'operation_not_registered', false, 'upload is not registered for the current control');
    }
    const view = target.element.ownerDocument.defaultView;
    const file = uploadFileFromPayload(request.payload, view);
    const nativeChooserButton = target.element.tagName === 'BUTTON'
      || (target.element.tagName === 'INPUT'
        && ['button', 'submit', 'reset'].includes(String(target.element.type || '').toLowerCase()));
    const dropTarget = nativeChooserButton ? null : uploadDropTarget(target.element);
    if (dropTarget) {
      dispatchFileDrop(dropTarget, file);
      fileTriggerHasValue.add(target.element);
      collect();
      return {
        accepted: true, local_wait_ms: prepared.local_wait_ms,
        dispatch_document_id: prepared.document_id,
        dispatch_basis_revision: prepared.basis_revision,
        upload_dispatch: 'drop',
      };
    }
    let input = linkedFileInput(target);
    const triggerDispatched = !input;
    if (triggerDispatched) input = await captureDynamicFileInput(target.element);
    if (!input) {
      throw actionFailure('dispatch', 'file_input_not_captured', false, 'the upload trigger did not expose one file input');
    }
    if (input.disabled || input.getAttribute('aria-disabled') === 'true' || input.webkitdirectory) {
      throw actionFailure('prepare', 'file_input_unavailable', !triggerDispatched, 'the current file input cannot accept one file');
    }
    if (!fileAcceptedByInput(file, input)) {
      throw actionFailure('prepare', 'file_type_rejected', !triggerDispatched, 'the file does not match the input accept policy');
    }
    const transfer = new view.DataTransfer();
    transfer.items.add(file);
    try { input.files = transfer.files; }
    catch (_error) {
      throw actionFailure('dispatch', 'file_list_assignment_failed', false, 'the browser rejected the selected file');
    }
    activeFileTrigger = target.element;
    const expectedTrigger = activeFileTrigger;
    setTimeout(() => { if (activeFileTrigger === expectedTrigger) activeFileTrigger = null; }, 10000);
    input.dispatchEvent(new view.Event('input', { bubbles: true, composed: true }));
    input.dispatchEvent(new view.Event('change', { bubbles: true }));
    collect();
    return {
      accepted: true, local_wait_ms: prepared.local_wait_ms,
      dispatch_document_id: prepared.document_id,
      dispatch_basis_revision: prepared.basis_revision,
      upload_dispatch: 'file_input',
    };
  }

  async function waitForSelectOption(request, target) {
    const startedAt = performance.now();
    const original = objectTargets.get(request.payload.option_object_id);
    const timeoutMs = Math.max(1, Math.min(30000, Number(request.timeout_ms) || 5000));
    const deadline = performance.now() + timeoutMs;
    let lastIssue = 'not_visible';
    while (performance.now() < deadline) {
      const option = objectTargets.get(request.payload.option_object_id);
      if (!option || option !== original || !option.isConnected || choiceOwner(option) !== target) {
        throw actionFailure('prepare', 'select_option_stale', false, 'select option was replaced before dispatch');
      }
      if (!optionEnabled(option, target)) {
        throw actionFailure('prepare', 'select_option_disabled', true, 'select option is not enabled');
      }
      const box = boxFor(option);
      lastIssue = visibilityFor(option, box) !== 'visible' ? 'not_visible'
        : !isTopmost(option, box) ? 'not_topmost'
          : targetGeometryIsAnimating(option) ? 'geometry_unstable' : null;
      if (!lastIssue) {
        return { option, box, local_wait_ms: Math.max(0, performance.now() - startedAt) };
      }
      if (!await waitForPreparationFrame(deadline)) break;
      collect();
    }
    throw actionFailure(
      'prepare', `select_option_actionability_timeout_${lastIssue}`, true,
      `select option actionability timed out: ${lastIssue}`,
    );
  }

  async function softAction(request, preflight) {
    if (request.operation === 'click') return softClick(request, preflight);
    if (request.operation === 'type') return softType(request, preflight);
    if (request.operation === 'upload') return softUpload(request, preflight);
    const prepared = preflight || await waitForSoftwarePreparation(request);
    if (request.operation !== 'select' || request.payload?.kind !== 'select') {
      throw actionFailure('dispatch', 'operation_not_registered', false, 'software action is not registered for the current operation');
    }
    const target = tokenTargets.get(request.action_token)?.element;
    const option = objectTargets.get(request.payload.option_object_id);
    if (!target || !option || choiceOwner(option) !== target || !optionEnabled(option, target)) {
      throw actionFailure('dispatch', 'select_option_not_current', false, 'software select option is not bound and enabled for this control');
    }
    if (target.matches('select') && option.matches('option')) {
      option.selected = true;
      target.dispatchEvent(new Event('input', { bubbles: true }));
      target.dispatchEvent(new Event('change', { bubbles: true }));
    } else {
      const current = await waitForSelectOption(request, target);
      const clientX = current.box.x + current.box.width / 2;
      const clientY = current.box.y + current.box.height / 2;
      for (const [type, EventClass, buttons] of [
        ['pointermove', PointerEvent, 0], ['mousemove', MouseEvent, 0],
        ['pointerdown', PointerEvent, 1], ['mousedown', MouseEvent, 1],
        ['pointerup', PointerEvent, 0], ['mouseup', MouseEvent, 0], ['click', MouseEvent, 0],
      ]) {
        current.option.dispatchEvent(new EventClass(type, {
          bubbles: true, cancelable: true, composed: true, clientX, clientY,
          button: 0, buttons, pointerId: 1, pointerType: 'mouse', isPrimary: true,
        }));
      }
      prepared.local_wait_ms += current.local_wait_ms;
    }
    requestAnimationFrame(collect);
    return {
      accepted: true, local_wait_ms: prepared.local_wait_ms,
      dispatch_document_id: prepared.document_id,
      dispatch_basis_revision: prepared.basis_revision,
    };
  }

  async function softActionBatch(request) {
    if (!Array.isArray(request.steps) || request.steps.length < 1 || request.steps.length > 32) {
      throw actionFailure('preflight', 'invalid_batch', true, 'form batch must contain 1 to 32 steps');
    }
    const prepared = [];
    const seen = new Set();
    const deadline = performance.now() + Math.max(1, Math.min(30000, Number(request.timeout_ms) || 5000));
    for (const step of request.steps) {
      if (seen.has(step.object_id)) {
        throw actionFailure('preflight', 'invalid_batch', true, 'form batch objects must be independent');
      }
      seen.add(step.object_id);
      const target = tokenTargets.get(step.action_token);
      const safeToggle = step.operation === 'click' && ['checkbox', 'radio', 'switch'].includes(target?.role);
      if (!['type', 'select'].includes(step.operation) && !safeToggle) {
        throw actionFailure('preflight', 'batch_boundary', true, 'submit, navigation, and upload are not allowed in a form batch');
      }
      const remaining = Math.max(1, deadline - performance.now());
      prepared.push(await waitForSoftwarePreparation({ ...step, timeout_ms: remaining }));
    }
    // The first pass proves the whole batch is safe to begin. Revalidate each
    // exact token again immediately before dispatch because controlled inputs
    // may synchronously rerender another field after a preceding input event.
    // Replacement remains stale: this never rebinds an old token to a new DOM
    // object.
    const receipts = [];
    for (let index = 0; index < request.steps.length; index += 1) {
      const step = request.steps[index];
      try {
        if (index > 0) collect();
        const remaining = Math.max(1, deadline - performance.now());
        const result = await softAction({ ...step, timeout_ms: remaining });
        receipts.push({
          object_id: step.object_id, operation: step.operation,
          accepted: result.accepted === true,
          dispatch_document_id: result.dispatch_document_id,
          dispatch_basis_revision: result.dispatch_basis_revision,
        });
      } catch (error) {
        return {
          accepted: false,
          partial_dispatch: receipts.length > 0,
          steps: [...receipts, {
            object_id: step.object_id, operation: step.operation, accepted: false,
            code: error.saccadeCode || 'software_action_rejected',
          }],
          failure_stage: error.saccadeStage || 'dispatch',
          failure_code: error.saccadeCode || 'software_action_rejected',
          retry_safe: receipts.length === 0 && error.saccadeRetrySafe === true,
          dispatch_document_id: receipts.at(-1)?.dispatch_document_id || prepared[0]?.document_id,
          dispatch_basis_revision: Math.max(
            ...receipts.map((receipt) => receipt.dispatch_basis_revision),
            ...prepared.map((item) => item.basis_revision),
          ),
        };
      }
    }
    requestAnimationFrame(collect);
    return {
      accepted: receipts.every((receipt) => receipt.accepted),
      steps: receipts,
      dispatch_document_id: receipts.at(-1)?.dispatch_document_id || prepared[0]?.document_id,
      dispatch_basis_revision: Math.max(...receipts.map((receipt) => receipt.dispatch_basis_revision)),
    };
  }

  function schedule() {
    if (scheduled || !config) return;
    scheduled = true;
    queueMicrotask(() => { scheduled = false; collect(); });
  }

  function scheduleVisual() {
    if (scheduled || scheduledFrame !== null || !config) return;
    scheduledFrame = requestAnimationFrame(() => {
      scheduledFrame = null;
      collect();
    });
  }

  function mutationCanChangeObservation(record) {
    if (isMouseAccuracyGame(document)) return true;
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
    compiledObjects = null;
    connectWorkerPort();
    for (const observer of observers.splice(0)) observer.disconnect();
    observedRoots = new WeakSet();
    observedDocuments = new WeakSet();
    choiceHasValue = new WeakMap();
    rememberedChoiceOwner = new WeakMap();
    rememberedChoicePopup = new WeakMap();
    observeDocument(document);
    if (document.readyState === 'loading') {
      schedule();
      document.addEventListener('DOMContentLoaded', collect, { once: true });
      return null;
    }
    schedule();
    return null;
  }

  function deauthorize() {
    config = null;
    compiledObjects = null;
    if (workerPort) workerPort.disconnect();
    workerPort = null;
    if (scheduledFrame !== null) cancelAnimationFrame(scheduledFrame);
    scheduledFrame = null;
    scheduled = false;
    tokenTargets.clear();
    objectTargets.clear();
    geometryResizeObserver?.disconnect();
    geometryObservedElements = new Set();
    for (const observer of observers.splice(0)) observer.disconnect();
    observedRoots = new WeakSet();
    observedDocuments = new WeakSet();
    choiceHasValue = new WeakMap();
    rememberedChoiceOwner = new WeakMap();
    rememberedChoicePopup = new WeakMap();
  }

  chrome.runtime.onMessage.addListener((message, _sender, respond) => {
    if (message.kind === 'collector.soft_click' || message.kind === 'collector.soft_action'
      || message.kind === 'collector.soft_action_batch') {
      const action = message.kind === 'collector.soft_click'
        ? softClick : message.kind === 'collector.soft_action_batch' ? softActionBatch : softAction;
      action(message.request).then((result) => respond({ ok: true, result })).catch((error) => {
        respond({
          ok: false,
          error: String(error.message || error),
          failure_stage: error.saccadeStage || 'dispatch',
          failure_code: error.saccadeCode || 'software_action_rejected',
          retry_safe: error.saccadeRetrySafe === true,
        });
      });
      return true;
    }
    try {
      if (message.kind === 'collector.ping') respond({ ok: true, extension_candidate: globalThis.SaccadeCandidate });
      else if (message.kind === 'collector.configure') { configure(message.config); respond({ ok: true, document_id: documentId }); }
      else if (message.kind === 'collector.observe') { collect(); respond({ ok: true }); }
      else if (message.kind === 'collector.snapshot') { collect({ forceSnapshot: true }); respond({ ok: true }); }
      else if (message.kind === 'collector.deauthorize') { deauthorize(); respond({ ok: true }); }
      else if (message.kind === 'collector.recollect') { collect(); respond({ ok: true }); }
      else if (message.kind === 'collector.prepare_action') respond({ ok: true, prepared: prepare(message.request) });
      else return false;
    } catch (error) {
      const detail = String(error.message || error);
      if (message.kind === 'collector.prepare_action' && detail === 'stale action basis') collect();
      respond({
        ok: false,
        error: detail,
        failure_stage: error.saccadeStage,
        failure_code: error.saccadeCode,
        retry_safe: error.saccadeRetrySafe === true,
      });
    }
    return true;
  });
})();
