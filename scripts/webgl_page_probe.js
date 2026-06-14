(() => {
  const viewport = {
    width: window.innerWidth || 0,
    height: window.innerHeight || 0,
    devicePixelRatio: window.devicePixelRatio || 1,
  };

  function cleanText(value) {
    return String(value || "").trim().replace(/\s+/g, " ").slice(0, 140);
  }

  function rectOf(el) {
    const rect = el.getBoundingClientRect();
    return {
      left: Number(rect.left || 0),
      top: Number(rect.top || 0),
      right: Number(rect.right || 0),
      bottom: Number(rect.bottom || 0),
      width: Number(rect.width || 0),
      height: Number(rect.height || 0),
    };
  }

  function styleOf(el) {
    const style = getComputedStyle(el);
    return {
      display: style.display || "",
      position: style.position || "",
      zIndex: style.zIndex || "",
      opacity: style.opacity || "",
      visibility: style.visibility || "",
      pointerEvents: style.pointerEvents || "",
      transform: style.transform || "",
      filter: style.filter || "",
      mixBlendMode: style.mixBlendMode || "",
      contain: style.contain || "",
      overflowX: style.overflowX || "",
      overflowY: style.overflowY || "",
      width: style.width || "",
      height: style.height || "",
    };
  }

  function labelOf(el) {
    if (!el || !el.tagName) return "";
    const id = el.id ? `#${cleanText(el.id)}` : "";
    const className = typeof el.className === "string"
      ? cleanText(el.className).split(/\s+/).filter(Boolean).slice(0, 4).map((name) => `.${name}`).join("")
      : "";
    return `${el.tagName.toLowerCase()}${id}${className}`;
  }

  function visibleEnough(rect, style) {
    return rect.width > 0 && rect.height > 0 &&
      rect.right > 0 && rect.bottom > 0 &&
      rect.left < viewport.width && rect.top < viewport.height &&
      style.display !== "none" && style.visibility !== "hidden" && style.opacity !== "0";
  }

  function ancestorChain(el) {
    const chain = [];
    let current = el.parentElement;
    while (current && chain.length < 6 && current !== document.documentElement) {
      const style = styleOf(current);
      chain.push({
        label: labelOf(current),
        rect: rectOf(current),
        style: {
          display: style.display,
          position: style.position,
          transform: style.transform,
          overflowX: style.overflowX,
          overflowY: style.overflowY,
          opacity: style.opacity,
        },
      });
      current = current.parentElement;
    }
    return chain;
  }

  function contextInfo(canvas) {
    const attempts = [];
    for (const name of ["webgl2", "webgl", "experimental-webgl"]) {
      try {
        const gl = canvas.getContext(name);
        attempts.push({ name, ok: Boolean(gl) });
        if (!gl) continue;
        let debug = null;
        try {
          debug = gl.getExtension && gl.getExtension("WEBGL_debug_renderer_info");
        } catch (_) {}
        const getParameter = (param) => {
          try {
            return cleanText(gl.getParameter(param));
          } catch (error) {
            return `error:${cleanText(error && error.message ? error.message : error)}`;
          }
        };
        let attrs = null;
        try {
          attrs = gl.getContextAttributes ? gl.getContextAttributes() : null;
        } catch (error) {
          attrs = { error: cleanText(error && error.message ? error.message : error) };
        }
        return {
          type: name,
          attempts,
          drawingBuffer: {
            width: gl.drawingBufferWidth || 0,
            height: gl.drawingBufferHeight || 0,
          },
          attributes: attrs,
          vendor: debug ? getParameter(debug.UNMASKED_VENDOR_WEBGL) : getParameter(gl.VENDOR),
          renderer: debug ? getParameter(debug.UNMASKED_RENDERER_WEBGL) : getParameter(gl.RENDERER),
          version: getParameter(gl.VERSION),
        };
      } catch (error) {
        attempts.push({ name, ok: false, error: cleanText(error && error.message ? error.message : error) });
      }
    }
    return { type: "none_or_2d", attempts };
  }

  function canvasInfo(canvas, index) {
    const rect = rectOf(canvas);
    const style = styleOf(canvas);
    const center = {
      x: rect.left + rect.width / 2,
      y: rect.top + rect.height / 2,
    };
    const elementsFromCenter =
      document.elementsFromPoint && Number.isFinite(center.x) && Number.isFinite(center.y)
        ? document.elementsFromPoint(center.x, center.y).slice(0, 8).map(labelOf)
        : [];
    return {
      index,
      label: labelOf(canvas),
      rect,
      visible: visibleEnough(rect, style),
      backing: {
        width: Number(canvas.width || 0),
        height: Number(canvas.height || 0),
      },
      style,
      context: contextInfo(canvas),
      elementsFromCenter,
      ancestors: ancestorChain(canvas),
    };
  }

  const body = document.body;
  const html = document.documentElement;
  const canvases = Array.from(document.querySelectorAll("canvas")).map(canvasInfo);
  const visibleLayers = Array.from(document.querySelectorAll("body, body > *, canvas, svg, [style], [class*='game' i], [id*='game' i], [class*='canvas' i], [id*='canvas' i]"))
    .map((el, index) => {
      const rect = rectOf(el);
      const style = styleOf(el);
      return {
        index,
        label: labelOf(el),
        rect,
        area: rect.width * rect.height,
        visible: visibleEnough(rect, style),
        style: {
          display: style.display,
          position: style.position,
          zIndex: style.zIndex,
          opacity: style.opacity,
          transform: style.transform,
          filter: style.filter,
          overflowX: style.overflowX,
          overflowY: style.overflowY,
        },
      };
    })
    .filter((item) => item.visible && item.area > 1000)
    .sort((a, b) => b.area - a.area)
    .slice(0, 40);

  return JSON.stringify({
    ok: true,
    engine: "saccade-webgl-page-probe-v0",
    url: location.href,
    title: document.title || "",
    viewport,
    scroll: {
      x: window.scrollX || 0,
      y: window.scrollY || 0,
      width: Math.max(html ? html.scrollWidth || 0 : 0, body ? body.scrollWidth || 0 : 0),
      height: Math.max(html ? html.scrollHeight || 0 : 0, body ? body.scrollHeight || 0 : 0),
    },
    body: {
      childCount: body ? body.children.length : 0,
      className: body ? cleanText(body.className || "") : "",
    },
    canvases,
    visibleLayers,
    notes: [
      "This probe records page structure and rendering metadata only; it does not read form values.",
      "Calling canvas.getContext can create a context on an unused canvas, so use this only for local dogfood diagnostics.",
    ],
  });
})()
