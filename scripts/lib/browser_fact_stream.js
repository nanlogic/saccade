const fs = require("node:fs/promises");

const FACT_STREAM_GLOBAL = "__saccadeFactStreamV0";
const FACT_SCHEMA = "saccade.browser_fact.v0";

function installScript(options = {}) {
  const opts = {
    queueLimit: 2048,
    textLimit: 160,
    allowCanvasDebugValues: false,
    ...options,
  };

  return `(() => {
    const GLOBAL = ${JSON.stringify(FACT_STREAM_GLOBAL)};
    const SCHEMA = ${JSON.stringify(FACT_SCHEMA)};
    const OPTIONS = ${JSON.stringify(opts)};
    if (window[GLOBAL]) {
      return { installed: false, schema: SCHEMA, seq: window[GLOBAL].seq() };
    }

    const queue = [];
    let seq = 0;
    let nodeSeq = 0;
    const nodeIds = new WeakMap();
    const lastSignatures = new Map();

    function nowMs() {
      return Math.round(performance.now());
    }

    function enqueue(factType, payload, privacy = "safe") {
      const fact = {
        kind: "browser_fact",
        schema: SCHEMA,
        seq: ++seq,
        t_ms: nowMs(),
        url: location.href,
        title: document.title || null,
        fact_type: factType,
        privacy,
        ...payload,
      };
      queue.push(fact);
      while (queue.length > OPTIONS.queueLimit) queue.shift();
      return fact;
    }

    function nodeId(el) {
      if (!nodeIds.has(el)) nodeIds.set(el, "node-" + (++nodeSeq));
      return nodeIds.get(el);
    }

    function compactText(text, limit = OPTIONS.textLimit) {
      return String(text || "").replace(/\\s+/g, " ").trim().slice(0, limit);
    }

    function ownText(el) {
      let text = "";
      for (const child of el.childNodes || []) {
        if (child.nodeType === Node.TEXT_NODE) text += " " + child.textContent;
      }
      return compactText(text);
    }

    function labelText(el) {
      if (el.getAttribute("aria-label")) return compactText(el.getAttribute("aria-label"));
      if (el.labels && el.labels.length) {
        return compactText([...el.labels].map((label) => label.textContent).join(" "));
      }
      if (el.id) {
        const label = document.querySelector('label[for="' + CSS.escape(el.id) + '"]');
        if (label) return compactText(label.textContent);
      }
      if (el.getAttribute("placeholder")) return compactText(el.getAttribute("placeholder"));
      if (el.getAttribute("title")) return compactText(el.getAttribute("title"));
      return ownText(el);
    }

    function sensitivity(el) {
      const explicit =
        el.getAttribute("data-sensitive") ||
        el.getAttribute("data-sensitivity") ||
        el.getAttribute("autocomplete");
      const joined = [
        explicit,
        el.type,
        el.name,
        el.id,
        el.getAttribute("aria-label"),
        labelText(el),
      ]
        .filter(Boolean)
        .join(" ")
        .toLowerCase();

      if (el.type === "password" || /password|passcode/.test(joined)) return "password";
      if (/credit|card|cc-number|cardnumber|cvv|cvc/.test(joined)) return "credit_card";
      if (/ssn|social security/.test(joined)) return "ssn";
      if (/tax.?id|ein|government|passport|license|id number/.test(joined)) return "government_id";
      if (/token|api.?key|secret|recovery/.test(joined)) return "api_token";
      if (/otp|one.?time|2fa|mfa|verification code/.test(joined)) return "otp";
      if (/signature|attestation/.test(joined)) return "signature";
      return null;
    }

    function valueState(el, kind) {
      if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement)) {
        return null;
      }
      if (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio")) {
        return el.checked ? "checked" : "unchecked";
      }
      if (kind) {
        return el.value ? "completed_without_value" : "requires_user_input";
      }
      if (el instanceof HTMLSelectElement) {
        return el.value ? "selected_without_value" : "empty";
      }
      return el.value ? "completed_without_value" : "empty";
    }

    function visibleRect(el) {
      const rect = el.getBoundingClientRect();
      const style = getComputedStyle(el);
      const visible =
        rect.width > 0 &&
        rect.height > 0 &&
        style.display !== "none" &&
        style.visibility !== "hidden" &&
        Number(style.opacity || "1") !== 0;
      return {
        visible,
        rect: {
          x: Math.round(rect.x * 100) / 100,
          y: Math.round(rect.y * 100) / 100,
          w: Math.round(rect.width * 100) / 100,
          h: Math.round(rect.height * 100) / 100,
        },
      };
    }

    function actionable(el) {
      if (el instanceof HTMLButtonElement) return true;
      if (el instanceof HTMLAnchorElement && el.href) return true;
      if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) return true;
      const role = (el.getAttribute("role") || "").toLowerCase();
      if (["button", "link", "checkbox", "radio", "switch", "textbox", "combobox", "menuitem"].includes(role)) return true;
      if (el.tabIndex >= 0) return true;
      if (typeof el.onclick === "function") return true;
      return getComputedStyle(el).cursor === "pointer";
    }

    function descriptor(el) {
      const bits = [el.tagName.toLowerCase()];
      if (el.id) bits.push("#" + el.id);
      if (el.getAttribute("name")) bits.push("[name=" + el.getAttribute("name") + "]");
      if (el.getAttribute("role")) bits.push("[role=" + el.getAttribute("role") + "]");
      return bits.join("");
    }

    function elementSummary(el) {
      const kind = sensitivity(el);
      const { visible, rect } = visibleRect(el);
      const text = kind ? null : compactText(labelText(el) || ownText(el));
      return {
        node_id: nodeId(el),
        descriptor: descriptor(el),
        tag: el.tagName.toLowerCase(),
        id: el.id || null,
        name: el.getAttribute("name") || null,
        role: el.getAttribute("role") || null,
        type: el.getAttribute("type") || null,
        text: text || null,
        visible,
        rect,
        actionable: actionable(el),
        contenteditable: el.isContentEditable || false,
        sensitivity: kind,
        value_state: valueState(el, kind),
        value_redacted: el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement,
      };
    }

    function signature(summary) {
      return JSON.stringify({
        descriptor: summary.descriptor,
        text: summary.text,
        visible: summary.visible,
        rect: summary.rect,
        actionable: summary.actionable,
        sensitivity: summary.sensitivity,
        value_state: summary.value_state,
      });
    }

    function canvasPayload(el, summary) {
      const payload = {
        node: summary,
        canvas: {
          width: el.width || null,
          height: el.height || null,
          client_width: el.clientWidth || null,
          client_height: el.clientHeight || null,
          has_debug_dataset: Boolean(el.dataset && el.dataset.debug),
        },
      };
      if (el.dataset && el.dataset.debug) {
        try {
          const parsed = JSON.parse(el.dataset.debug);
          payload.canvas.debug_keys = Object.keys(parsed).sort();
          if (OPTIONS.allowCanvasDebugValues) payload.canvas.debug = parsed;
        } catch {
          payload.canvas.debug_parse_error = true;
        }
      }
      return payload;
    }

    function scanElement(el, reason = "scan") {
      if (!(el instanceof Element)) return;
      const summary = elementSummary(el);
      const interesting =
        summary.visible &&
        (summary.actionable ||
          summary.sensitivity ||
          summary.contenteditable ||
          summary.tag === "canvas" ||
          summary.text);
      if (!interesting) return;

      const sig = signature(summary);
      const previous = lastSignatures.get(summary.node_id);
      if (previous === sig && reason !== "force") return;
      lastSignatures.set(summary.node_id, sig);

      enqueue("node_seen", { reason, node: summary }, summary.sensitivity ? "redacted" : "safe");
      if (summary.actionable) {
        enqueue("actionable_seen", { reason, node: summary }, summary.sensitivity ? "redacted" : "safe");
      }
      if (summary.sensitivity) {
        enqueue("sensitive_field_seen", { reason, node: summary }, "redacted");
      }
      if (summary.tag === "canvas") {
        enqueue("canvas_seen", { reason, ...canvasPayload(el, summary) }, "safe");
      }
    }

    function scanTree(root, reason) {
      if (!(root instanceof Element)) return;
      scanElement(root, reason);
      for (const el of root.querySelectorAll("*")) scanElement(el, reason);
    }

    const observer = new MutationObserver((mutations) => {
      for (const mutation of mutations) {
        if (mutation.type === "childList") {
          for (const node of mutation.addedNodes) {
            if (node instanceof Element) scanTree(node, "child_added");
          }
        } else if (mutation.type === "attributes") {
          scanElement(mutation.target, "attribute_changed");
        } else if (mutation.type === "characterData" && mutation.target.parentElement) {
          scanElement(mutation.target.parentElement, "text_changed");
        }
      }
    });

    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      characterData: true,
      attributeFilter: [
        "aria-label",
        "class",
        "data-sensitive",
        "data-sensitivity",
        "data-debug",
        "disabled",
        "hidden",
        "id",
        "name",
        "placeholder",
        "role",
        "style",
        "type",
        "value",
      ],
    });

    scanTree(document.body || document.documentElement, "initial_scan");

    window[GLOBAL] = {
      schema: SCHEMA,
      seq: () => seq,
      drain(limit = 200) {
        const count = Math.max(0, Math.min(Number(limit) || 200, queue.length));
        return queue.splice(0, count);
      },
      snapshot() {
        scanTree(document.body || document.documentElement, "snapshot");
        return { schema: SCHEMA, seq, queued: queue.length };
      },
      stop() {
        observer.disconnect();
        return { schema: SCHEMA, seq, queued: queue.length };
      },
    };

    return { installed: true, schema: SCHEMA, seq, queued: queue.length };
  })();`;
}

async function installBrowserFactStream(bridge, options = {}) {
  return await bridge.execute(installScript(options));
}

async function drainBrowserFacts(bridge, limit = 200) {
  return await bridge.execute(
    `return window.${FACT_STREAM_GLOBAL} ? window.${FACT_STREAM_GLOBAL}.drain(${Number(limit) || 200}) : [];`,
  );
}

async function snapshotBrowserFacts(bridge) {
  return await bridge.execute(
    `return window.${FACT_STREAM_GLOBAL} ? window.${FACT_STREAM_GLOBAL}.snapshot() : null;`,
  );
}

function summarizeFacts(facts) {
  const byType = {};
  let actionable = 0;
  let sensitive = 0;
  let redacted = 0;
  let canvas = 0;
  let maxSeq = 0;
  for (const fact of facts) {
    byType[fact.fact_type] = (byType[fact.fact_type] || 0) + 1;
    if (fact.node?.actionable || fact.fact_type === "actionable_seen") actionable += 1;
    if (fact.node?.sensitivity || fact.fact_type === "sensitive_field_seen") sensitive += 1;
    if (fact.privacy === "redacted" || fact.node?.value_redacted) redacted += 1;
    if (fact.fact_type === "canvas_seen") canvas += 1;
    if (Number.isFinite(fact.seq)) maxSeq = Math.max(maxSeq, fact.seq);
  }
  return {
    count: facts.length,
    max_seq: maxSeq,
    by_type: byType,
    actionable,
    sensitive,
    redacted,
    canvas,
  };
}

async function writeFactsJsonl(filePath, facts) {
  if (!facts.length) return;
  await fs.appendFile(filePath, facts.map((fact) => JSON.stringify(fact)).join("\\n") + "\\n");
}

function factTextCorpus(facts) {
  return JSON.stringify(facts);
}

module.exports = {
  FACT_SCHEMA,
  FACT_STREAM_GLOBAL,
  drainBrowserFacts,
  factTextCorpus,
  installBrowserFactStream,
  installScript,
  snapshotBrowserFacts,
  summarizeFacts,
  writeFactsJsonl,
};
