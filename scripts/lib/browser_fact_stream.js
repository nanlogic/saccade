const fs = require("node:fs/promises");

const FACT_STREAM_GLOBAL = "__saccadeFactStreamV0";
const FACT_SCHEMA = "saccade.browser_fact.v0";

function nsToMs(ns) {
  return Number.isFinite(ns) ? Math.round(ns / 1_000_000) : null;
}

function browserFactEnvelope(factType, payload = {}, options = {}) {
  return {
    kind: "browser_fact",
    schema: FACT_SCHEMA,
    seq: options.seq ?? null,
    t_ms: options.t_ms ?? null,
    url: options.url ?? null,
    title: options.title ?? null,
    fact_type: factType,
    privacy: options.privacy || "safe",
    ...payload,
  };
}

function browserFactFromMousemaxTarget(target, options = {}) {
  if (!target || !target.center_css || !target.bbox_css) {
    throw new Error("mousemax target is missing center_css or bbox_css");
  }
  const reason = options.reason || "mousemax_target";
  const targetId = target.id ?? options.targetId ?? null;
  return browserFactEnvelope(
    "visual_object_seen",
    {
      reason,
      node: null,
      visual_object: {
        object_id:
          options.object_id ||
          `mousemax:${targetId ?? "unknown"}:frame-${target.frame_id ?? "unknown"}`,
        source: "mousemax_replay_target",
        detector_source: target.source || null,
        target_id: targetId,
        frame_id: target.frame_id ?? null,
        first_seen_ns: target.first_seen_ns ?? null,
        last_seen_ns: target.last_seen_ns ?? null,
        center_css: target.center_css,
        bbox_css: target.bbox_css,
        radius_css: target.radius_css ?? null,
        confidence: target.confidence ?? null,
        clicked: target.clicked ?? false,
        game_area_css: options.game_area_css || null,
      },
    },
    {
      seq: options.seq,
      t_ms:
        options.t_ms ??
        nsToMs(target.last_seen_ns ?? target.first_seen_ns ?? options.t_obs_ns ?? null),
      url: options.url,
      title: options.title || "MOUSEMAX replay",
      privacy: "safe",
    },
  );
}

function installScript(options = {}) {
  const opts = {
    queueLimit: 2048,
    textLimit: 160,
    allowCanvasDebugValues: false,
    allowCanvasPixelRead: false,
    canvasMaxSamplePixels: 160000,
    visualColorDistanceThreshold: 48,
    visualMinAreaPx: 20,
    visualMaxAreaFraction: 0.65,
    visualMaxObjectsPerCanvas: 32,
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
    const lastVisualSignatures = new Map();

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

    function colorDistance(a, b) {
      const dr = a[0] - b[0];
      const dg = a[1] - b[1];
      const db = a[2] - b[2];
      return Math.sqrt(dr * dr + dg * dg + db * db);
    }

    function sampledStep(width, height) {
      const maxPixels = Math.max(1, Number(OPTIONS.canvasMaxSamplePixels) || 160000);
      return Math.max(1, Math.ceil(Math.sqrt((width * height) / maxPixels)));
    }

    function backgroundColor(data, width, height, step) {
      const sum = [0, 0, 0];
      let count = 0;
      const add = (x, y) => {
        const i = (y * width + x) * 4;
        if (data[i + 3] === 0) return;
        sum[0] += data[i];
        sum[1] += data[i + 1];
        sum[2] += data[i + 2];
        count += 1;
      };
      for (let x = 0; x < width; x += step) {
        add(x, 0);
        add(x, height - 1);
      }
      for (let y = 0; y < height; y += step) {
        add(0, y);
        add(width - 1, y);
      }
      if (!count) return [0, 0, 0];
      return [sum[0] / count, sum[1] / count, sum[2] / count];
    }

    function canvasVisualObjects(el, summary) {
      if (!OPTIONS.allowCanvasPixelRead) {
        return { objects: [], skipped: "canvas_pixel_read_disabled" };
      }
      if (!(el instanceof HTMLCanvasElement)) {
        return { objects: [], skipped: "not_canvas" };
      }

      const width = Number(el.width || 0);
      const height = Number(el.height || 0);
      if (width <= 0 || height <= 0) {
        return { objects: [], skipped: "empty_canvas" };
      }

      let ctx;
      try {
        ctx = el.getContext("2d", { willReadFrequently: true });
      } catch (error) {
        return { objects: [], skipped: "context_error", error: String(error).slice(0, 120) };
      }
      if (!ctx) {
        return { objects: [], skipped: "no_2d_context" };
      }

      let image;
      try {
        image = ctx.getImageData(0, 0, width, height);
      } catch (error) {
        return { objects: [], skipped: "read_error", error: String(error).slice(0, 120) };
      }

      const step = sampledStep(width, height);
      const sampleW = Math.ceil(width / step);
      const sampleH = Math.ceil(height / step);
      const visited = new Uint8Array(sampleW * sampleH);
      const bg = backgroundColor(image.data, width, height, step);
      const threshold = Math.max(1, Number(OPTIONS.visualColorDistanceThreshold) || 48);
      const minAreaPx = Math.max(1, Number(OPTIONS.visualMinAreaPx) || 20);
      const maxAreaPx = width * height * Math.max(0.01, Number(OPTIONS.visualMaxAreaFraction) || 0.65);
      const rect = summary.rect;
      const objects = [];

      const isForeground = (sx, sy) => {
        const x = Math.min(width - 1, sx * step);
        const y = Math.min(height - 1, sy * step);
        const i = (y * width + x) * 4;
        if (image.data[i + 3] === 0) return false;
        return colorDistance([image.data[i], image.data[i + 1], image.data[i + 2]], bg) >= threshold;
      };

      for (let sy = 0; sy < sampleH; sy += 1) {
        for (let sx = 0; sx < sampleW; sx += 1) {
          const startIndex = sy * sampleW + sx;
          if (visited[startIndex] || !isForeground(sx, sy)) {
            visited[startIndex] = 1;
            continue;
          }

          const stack = [[sx, sy]];
          visited[startIndex] = 1;
          let count = 0;
          let minX = sx;
          let maxX = sx;
          let minY = sy;
          let maxY = sy;
          let sumX = 0;
          let sumY = 0;
          let sumR = 0;
          let sumG = 0;
          let sumB = 0;

          while (stack.length) {
            const [cx, cy] = stack.pop();
            const px = Math.min(width - 1, cx * step);
            const py = Math.min(height - 1, cy * step);
            const pi = (py * width + px) * 4;
            count += 1;
            minX = Math.min(minX, cx);
            maxX = Math.max(maxX, cx);
            minY = Math.min(minY, cy);
            maxY = Math.max(maxY, cy);
            sumX += px;
            sumY += py;
            sumR += image.data[pi];
            sumG += image.data[pi + 1];
            sumB += image.data[pi + 2];

            for (const [nx, ny] of [
              [cx - 1, cy],
              [cx + 1, cy],
              [cx, cy - 1],
              [cx, cy + 1],
            ]) {
              if (nx < 0 || ny < 0 || nx >= sampleW || ny >= sampleH) continue;
              const ni = ny * sampleW + nx;
              if (visited[ni]) continue;
              visited[ni] = 1;
              if (isForeground(nx, ny)) stack.push([nx, ny]);
            }
          }

          const areaPx = count * step * step;
          if (areaPx < minAreaPx || areaPx > maxAreaPx) continue;

          const x0 = minX * step;
          const y0 = minY * step;
          const x1 = Math.min(width, (maxX + 1) * step);
          const y1 = Math.min(height, (maxY + 1) * step);
          const centerCanvas = { x: sumX / count, y: sumY / count };
          const bboxCanvas = { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
          const centerCss = {
            x: Math.round((rect.x + (centerCanvas.x / width) * rect.w) * 100) / 100,
            y: Math.round((rect.y + (centerCanvas.y / height) * rect.h) * 100) / 100,
          };
          const bboxCss = {
            x: Math.round((rect.x + (bboxCanvas.x / width) * rect.w) * 100) / 100,
            y: Math.round((rect.y + (bboxCanvas.y / height) * rect.h) * 100) / 100,
            w: Math.round(((bboxCanvas.w / width) * rect.w) * 100) / 100,
            h: Math.round(((bboxCanvas.h / height) * rect.h) * 100) / 100,
          };

          objects.push({
            source: "canvas_pixel_probe",
            canvas_node_id: summary.node_id,
            center_canvas_px: {
              x: Math.round(centerCanvas.x * 100) / 100,
              y: Math.round(centerCanvas.y * 100) / 100,
            },
            bbox_canvas_px: bboxCanvas,
            center_css: centerCss,
            bbox_css: bboxCss,
            area_px: Math.round(areaPx),
            avg_rgba: [
              Math.round(sumR / count),
              Math.round(sumG / count),
              Math.round(sumB / count),
              255,
            ],
            confidence: Math.min(1, Math.max(0.2, areaPx / Math.max(minAreaPx, 1))),
          });
        }
      }

      objects.sort((a, b) => b.area_px - a.area_px || a.center_canvas_px.x - b.center_canvas_px.x);
      return {
        objects: objects.slice(0, Math.max(1, Number(OPTIONS.visualMaxObjectsPerCanvas) || 32)),
        sample: {
          width,
          height,
          step,
          background_rgb: bg.map((value) => Math.round(value)),
          threshold,
        },
      };
    }

    function sampleCanvasVisualObjects(reason = "visual_sample") {
      const enqueued = [];
      for (const el of document.querySelectorAll("canvas")) {
        const summary = elementSummary(el);
        if (!summary.visible) continue;
        const result = canvasVisualObjects(el, summary);
        const signature = JSON.stringify(result.objects.map((object) => ({
          c: object.center_canvas_px,
          b: object.bbox_canvas_px,
          a: object.area_px,
          rgb: object.avg_rgba,
        })));
        const previous = lastVisualSignatures.get(summary.node_id);
        if (previous === signature && reason !== "force") continue;
        lastVisualSignatures.set(summary.node_id, signature);
        result.objects.forEach((object, index) => {
          const fact = enqueue("visual_object_seen", {
            reason,
            node: summary,
            visual_object: {
              object_id: summary.node_id + ":visual-" + index,
              ...object,
            },
            visual_sample: result.sample || null,
          }, "safe");
          enqueued.push(fact);
        });
      }
      return {
        schema: SCHEMA,
        enqueued: enqueued.length,
        objects: enqueued.map((fact) => fact.visual_object),
      };
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
        if (OPTIONS.allowCanvasPixelRead) sampleCanvasVisualObjects("snapshot");
        return { schema: SCHEMA, seq, queued: queue.length };
      },
      sampleVisualObjects(reason = "visual_sample") {
        return sampleCanvasVisualObjects(reason);
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

async function sampleBrowserVisualObjects(bridge, reason = "visual_sample") {
  return await bridge.execute(
    `return window.${FACT_STREAM_GLOBAL} ? window.${FACT_STREAM_GLOBAL}.sampleVisualObjects(${JSON.stringify(reason)}) : null;`,
  );
}

function summarizeFacts(facts) {
  const byType = {};
  let actionable = 0;
  let sensitive = 0;
  let redacted = 0;
  let canvas = 0;
  let visualObjects = 0;
  let maxSeq = 0;
  for (const fact of facts) {
    byType[fact.fact_type] = (byType[fact.fact_type] || 0) + 1;
    if (fact.node?.actionable || fact.fact_type === "actionable_seen") actionable += 1;
    if (fact.node?.sensitivity || fact.fact_type === "sensitive_field_seen") sensitive += 1;
    if (fact.privacy === "redacted" || fact.node?.value_redacted) redacted += 1;
    if (fact.fact_type === "canvas_seen") canvas += 1;
    if (fact.fact_type === "visual_object_seen") visualObjects += 1;
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
    visual_objects: visualObjects,
  };
}

async function writeFactsJsonl(filePath, facts) {
  if (!facts.length) return;
  await fs.appendFile(filePath, facts.map((fact) => JSON.stringify(fact)).join("\n") + "\n");
}

function factTextCorpus(facts) {
  return JSON.stringify(facts);
}

module.exports = {
  FACT_SCHEMA,
  FACT_STREAM_GLOBAL,
  browserFactEnvelope,
  browserFactFromMousemaxTarget,
  drainBrowserFacts,
  factTextCorpus,
  installBrowserFactStream,
  installScript,
  sampleBrowserVisualObjects,
  snapshotBrowserFacts,
  summarizeFacts,
  writeFactsJsonl,
};
