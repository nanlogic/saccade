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

  function canvasPixelProbe(canvas) {
    const width = Number(canvas.width || 0);
    const height = Number(canvas.height || 0);
    const totalPixels = width * height;
    if (!width || !height) {
      return { status: "empty_backing", width, height };
    }
    if (totalPixels > 2500000) {
      return { status: "too_large", width, height, pixels: totalPixels };
    }
    let ctx = null;
    try {
      ctx = canvas.getContext("2d", { willReadFrequently: true });
    } catch (error) {
      return { status: "context_error", width, height, error: cleanText(error && error.message ? error.message : error) };
    }
    if (!ctx) {
      return { status: "no_2d_context", width, height };
    }

    let data = null;
    try {
      data = ctx.getImageData(0, 0, width, height).data;
    } catch (error) {
      return { status: "read_error", width, height, error: cleanText(error && error.message ? error.message : error) };
    }

    const stride = Math.max(1, Math.floor(Math.sqrt(totalPixels / 200000)));
    let samples = 0;
    let saturated = 0;
    let alphaNonZero = 0;
    let edge = 0;
    let edgeSamples = 0;
    let minR = 255;
    let minG = 255;
    let minB = 255;
    let maxR = 0;
    let maxG = 0;
    let maxB = 0;
    let minLuma = 255;
    let maxLuma = 0;
    let sumLuma = 0;
    let sumLumaSq = 0;
    let checksum = 2166136261;

    for (let y = 0; y < height; y += stride) {
      for (let x = 0; x < width; x += stride) {
        const i = (y * width + x) * 4;
        const r = data[i] || 0;
        const g = data[i + 1] || 0;
        const b = data[i + 2] || 0;
        const a = data[i + 3] || 0;
        minR = Math.min(minR, r);
        minG = Math.min(minG, g);
        minB = Math.min(minB, b);
        maxR = Math.max(maxR, r);
        maxG = Math.max(maxG, g);
        maxB = Math.max(maxB, b);
        const luma = (r + g + b) / 3;
        minLuma = Math.min(minLuma, luma);
        maxLuma = Math.max(maxLuma, luma);
        sumLuma += luma;
        sumLumaSq += luma * luma;
        if (Math.max(r, g, b) - Math.min(r, g, b) >= 45) saturated += 1;
        if (a > 0) alphaNonZero += 1;
        checksum ^= r;
        checksum = Math.imul(checksum, 16777619);
        checksum ^= g;
        checksum = Math.imul(checksum, 16777619);
        checksum ^= b;
        checksum = Math.imul(checksum, 16777619);
        checksum ^= a;
        checksum = Math.imul(checksum, 16777619);

        if (x + stride < width) {
          const j = (y * width + x + stride) * 4;
          const delta = Math.abs(r - (data[j] || 0)) +
            Math.abs(g - (data[j + 1] || 0)) +
            Math.abs(b - (data[j + 2] || 0));
          if (delta >= 18) edge += 1;
          edgeSamples += 1;
        }
        samples += 1;
      }
    }

    const lumaMean = sumLuma / Math.max(1, samples);
    const lumaVariance = Math.max(0, (sumLumaSq / Math.max(1, samples)) - (lumaMean * lumaMean));
    return {
      status: "ok",
      width,
      height,
      pixels: totalPixels,
      sampleStride: stride,
      samples,
      edgeRatio: Number((edge / Math.max(1, edgeSamples)).toFixed(6)),
      saturatedRatio: Number((saturated / Math.max(1, samples)).toFixed(6)),
      alphaNonZeroRatio: Number((alphaNonZero / Math.max(1, samples)).toFixed(6)),
      maxChannelRange: Math.max(maxR - minR, maxG - minG, maxB - minB),
      lumaRange: Number((maxLuma - minLuma).toFixed(6)),
      lumaStdev: Number(Math.sqrt(lumaVariance).toFixed(6)),
      checksum: (checksum >>> 0).toString(16).padStart(8, "0"),
    };
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
      pixelProbe: canvasPixelProbe(canvas),
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
