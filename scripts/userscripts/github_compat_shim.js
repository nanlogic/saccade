(() => {
  const installed = {};

  if (typeof window.IntersectionObserver === "undefined") {
    class SaccadeIntersectionObserver {
      constructor(callback, options) {
        this.callback = callback;
        this.options = options || {};
        this.targets = new Set();
      }

      observe(target) {
        if (!target) return;
        this.targets.add(target);
        const rect = target.getBoundingClientRect ? target.getBoundingClientRect() : null;
        const entry = {
          time: Date.now(),
          target,
          rootBounds: null,
          boundingClientRect: rect,
          intersectionRect: rect,
          isIntersecting: true,
          intersectionRatio: 1
        };
        setTimeout(() => {
          try {
            this.callback([entry], this);
          } catch (_) {}
        }, 0);
      }

      unobserve(target) {
        this.targets.delete(target);
      }

      disconnect() {
        this.targets.clear();
      }

      takeRecords() {
        return [];
      }
    }

    window.IntersectionObserver = SaccadeIntersectionObserver;
    installed.intersectionObserver = true;
  } else {
    installed.intersectionObserver = false;
  }

  if (typeof window.CSSStyleSheet === "undefined") {
    window.CSSStyleSheet = class SaccadeCSSStyleSheet {
      constructor() {
        this.cssText = "";
      }

      replaceSync(text) {
        this.cssText = String(text || "");
      }

      replace(text) {
        this.replaceSync(text);
        return Promise.resolve(this);
      }
    };
    installed.cssStyleSheet = true;
    installed.cssStyleSheetReplaceSync = true;
    installed.cssStyleSheetReplace = true;
  } else {
    installed.cssStyleSheet = false;

    if (typeof window.CSSStyleSheet.prototype.replaceSync === "undefined") {
      window.CSSStyleSheet.prototype.replaceSync = function replaceSync(text) {
        this.__saccadeCssText = String(text || "");
      };
      installed.cssStyleSheetReplaceSync = true;
    } else {
      installed.cssStyleSheetReplaceSync = false;
    }

    if (typeof window.CSSStyleSheet.prototype.replace === "undefined") {
      window.CSSStyleSheet.prototype.replace = function replace(text) {
        this.replaceSync(text);
        return Promise.resolve(this);
      };
      installed.cssStyleSheetReplace = true;
    } else {
      installed.cssStyleSheetReplace = false;
    }
  }

  function installAdoptedStyleSheets(proto, label) {
    if (!proto) return false;
    if (Object.prototype.hasOwnProperty.call(proto, "adoptedStyleSheets")) return false;
    const existing = Object.getOwnPropertyDescriptor(proto, "adoptedStyleSheets");
    if (existing) return false;
    const store = new WeakMap();
    Object.defineProperty(proto, "adoptedStyleSheets", {
      configurable: true,
      enumerable: true,
      get() {
        return store.get(this) || [];
      },
      set(value) {
        store.set(this, Array.from(value || []));
      }
    });
    installed[label] = true;
    return true;
  }

  installed.documentAdoptedStyleSheets =
    typeof Document !== "undefined" && installAdoptedStyleSheets(Document.prototype, "documentAdoptedStyleSheets");
  installed.shadowRootAdoptedStyleSheets =
    typeof ShadowRoot !== "undefined" && installAdoptedStyleSheets(ShadowRoot.prototype, "shadowRootAdoptedStyleSheets");

  function installCodeMirrorInputShim() {
    const host = String(location.hostname || "");
    if (!/(^|\.)github\.com$/.test(host) && !/(^|\.)gist\.github\.com$/.test(host)) {
      return { enabled: false, reason: "not_github" };
    }
    if (window.__saccadeGithubCodeMirrorInputShim) {
      return window.__saccadeGithubCodeMirrorInputShim;
    }

    const state = {
      enabled: true,
      kind: "saccade_github_codemirror_input_shim_v1",
      focusCalls: 0,
      keyEventsHandled: 0,
      commandsHandled: 0,
      caretUpdates: 0,
      textValuesLogged: false
    };
    let lastRoot = null;

    function ensureCaretStyle() {
      if (document.getElementById("saccade-github-codemirror-caret-style")) return;
      const style = document.createElement("style");
      style.id = "saccade-github-codemirror-caret-style";
      style.textContent = `
        @keyframes saccadeGithubCodeMirrorCaretBlink {
          0%, 49% { opacity: 1; }
          50%, 100% { opacity: 0; }
        }
        .CodeMirror.saccade-cm-human-focused {
          outline: 2px solid #0969da !important;
          outline-offset: 2px !important;
          box-shadow: 0 0 0 4px rgba(9, 105, 218, 0.16) !important;
        }
        .CodeMirror .saccade-cm-human-caret {
          position: absolute;
          width: 2px;
          min-height: 16px;
          background: #0969da;
          z-index: 50;
          pointer-events: none;
          animation: saccadeGithubCodeMirrorCaretBlink 1s steps(1, end) infinite;
        }
      `;
      document.head.appendChild(style);
    }

    function codeMirrorFromRoot(root) {
      if (!root) return null;
      if (root.CodeMirror && typeof root.CodeMirror.getValue === "function") return root.CodeMirror;
      return null;
    }

    function rootFromEvent(event) {
      const target = event && event.target;
      return target && target.closest ? target.closest(".CodeMirror") : null;
    }

    function rootFromActiveElement() {
      const active = document.activeElement;
      if (active && active.closest) {
        const root = active.closest(".CodeMirror");
        if (root) return root;
      }
      if (lastRoot && document.contains(lastRoot)) return lastRoot;
      return null;
    }

    function focusedCodeMirror(event) {
      const root = rootFromEvent(event) || rootFromActiveElement();
      const cm = codeMirrorFromRoot(root);
      if (!cm) return null;
      lastRoot = root;
      updateCaret(cm, root);
      return cm;
    }

    function syncCodeMirror(cm) {
      if (cm && typeof cm.save === "function") cm.save();
      const input = cm && typeof cm.getInputField === "function" ? cm.getInputField() : null;
      if (input) {
        input.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: "" }));
        input.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }

    function caretForRoot(root) {
      let caret = root.querySelector(":scope > .saccade-cm-human-caret");
      if (!caret) {
        caret = document.createElement("div");
        caret.className = "saccade-cm-human-caret";
        root.appendChild(caret);
      }
      return caret;
    }

    function updateCaret(cm, root) {
      if (!cm || !root) return;
      ensureCaretStyle();
      root.classList.add("saccade-cm-human-focused");
      try {
        const rootRect = root.getBoundingClientRect();
        const pageCoords = typeof cm.cursorCoords === "function"
          ? cm.cursorCoords(null, "page")
          : null;
        const localCoords = typeof cm.cursorCoords === "function"
          ? cm.cursorCoords(null, "local")
          : null;
        const caret = caretForRoot(root);
        const left = pageCoords && Number.isFinite(pageCoords.left)
          ? pageCoords.left - window.scrollX - rootRect.left
          : (localCoords && Number.isFinite(localCoords.left) ? localCoords.left : 52);
        const top = pageCoords && Number.isFinite(pageCoords.top)
          ? pageCoords.top - window.scrollY - rootRect.top
          : (localCoords && Number.isFinite(localCoords.top) ? localCoords.top : 12);
        const heightSource = pageCoords || localCoords;
        const height = heightSource && Number.isFinite(heightSource.bottom - heightSource.top)
          ? Math.max(16, heightSource.bottom - heightSource.top)
          : 18;
        caret.style.left = `${Math.max(0, Math.round(left))}px`;
        caret.style.top = `${Math.max(0, Math.round(top))}px`;
        caret.style.height = `${Math.round(height)}px`;
        caret.style.display = "block";
        state.caretUpdates++;
      } catch (_) {}
    }

    function hideCarets() {
      for (const root of Array.from(document.querySelectorAll(".CodeMirror"))) {
        root.classList.remove("saccade-cm-human-focused");
        const caret = root.querySelector(":scope > .saccade-cm-human-caret");
        if (caret) caret.style.display = "none";
      }
    }

    function focusFromPointer(event) {
      const root = rootFromEvent(event);
      const cm = codeMirrorFromRoot(root);
      if (!cm || typeof cm.focus !== "function") return;
      lastRoot = root;
      setTimeout(() => {
        try {
          cm.focus();
          updateCaret(cm, root);
          state.focusCalls++;
        } catch (_) {}
      }, 0);
    }

    function runCommand(cm, command) {
      if (!cm || typeof cm.execCommand !== "function") return false;
      try {
        cm.execCommand(command);
        syncCodeMirror(cm);
        updateCaret(cm, lastRoot || rootFromActiveElement());
        state.commandsHandled++;
        return true;
      } catch (_) {
        return false;
      }
    }

    function insertText(cm, text) {
      if (!cm || typeof cm.replaceSelection !== "function") return false;
      try {
        cm.replaceSelection(text, "end");
        syncCodeMirror(cm);
        updateCaret(cm, lastRoot || rootFromActiveElement());
        state.keyEventsHandled++;
        return true;
      } catch (_) {
        return false;
      }
    }

    function handleKeydown(event) {
      if (!event || event.defaultPrevented || event.isComposing) return;
      if (event.metaKey || event.ctrlKey || event.altKey) return;
      const cm = focusedCodeMirror(event);
      if (!cm) return;

      let handled = false;
      if (event.key && event.key.length === 1) {
        handled = insertText(cm, event.key);
      } else if (event.key === "Enter") {
        handled = runCommand(cm, "newlineAndIndent") || insertText(cm, "\n");
      } else if (event.key === "Backspace") {
        handled = runCommand(cm, "delCharBefore");
      } else if (event.key === "Delete") {
        handled = runCommand(cm, "delCharAfter");
      } else if (event.key === "Tab") {
        handled = insertText(cm, "  ");
      }

      if (handled) {
        event.preventDefault();
        event.stopPropagation();
      }
    }

    document.addEventListener("pointerdown", focusFromPointer, true);
    document.addEventListener("mousedown", focusFromPointer, true);
    document.addEventListener("click", focusFromPointer, true);
    document.addEventListener("keydown", handleKeydown, true);
    document.addEventListener("selectionchange", () => {
      const root = rootFromActiveElement();
      const cm = codeMirrorFromRoot(root);
      if (cm) updateCaret(cm, root);
    });
    window.addEventListener("blur", hideCarets);
    setTimeout(() => {
      const root = rootFromActiveElement();
      const cm = codeMirrorFromRoot(root);
      if (cm) updateCaret(cm, root);
    }, 0);
    window.__saccadeGithubCodeMirrorInputShim = state;
    return state;
  }

  installed.githubCodeMirrorInputShim = installCodeMirrorInputShim();

  const report = {
    kind: "saccade_github_compat_shim_v0",
    timing: "servoshell_userscript_head_bind_or_webdriver_execute",
    href: String(location.href || ""),
    installed,
    features: {
      intersectionObserver: typeof window.IntersectionObserver,
      cssStyleSheet: typeof window.CSSStyleSheet,
      cssStyleSheetReplaceSync: typeof (window.CSSStyleSheet && window.CSSStyleSheet.prototype && window.CSSStyleSheet.prototype.replaceSync),
      documentAdoptedStyleSheets: typeof document.adoptedStyleSheets,
      documentPrototypeAdoptedStyleSheets: typeof Document !== "undefined" && "adoptedStyleSheets" in Document.prototype,
      shadowRootPrototypeAdoptedStyleSheets: typeof ShadowRoot !== "undefined" && "adoptedStyleSheets" in ShadowRoot.prototype
    }
  };

  window.__saccadeCompatShim = report;
  return report;
})()
