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
