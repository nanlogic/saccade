(function () {
  const canvas = document.getElementById("arena");
  const ctx = canvas.getContext("2d");
  const truthNode = document.getElementById("truth");
  const params = new URLSearchParams(window.location.search);

  const spawnIntervals = {
    slow: 1600,
    normal: 1100,
    fast: 700,
    epic: 306,
  };
  const sizes = {
    tiny: 7,
    small: 11,
    normal: 15,
  };

  const speed = (params.get("speed") || "epic").toLowerCase();
  const size = (params.get("size") || "tiny").toLowerCase();
  const durationMs = Math.max(1, Number(params.get("duration") || 15)) * 1000;
  const seed = Number(params.get("seed") || 42) >>> 0;
  const spawnIntervalMs = spawnIntervals[speed] || spawnIntervals.epic;
  const maxRadius = sizes[size] || sizes.tiny;
  const lifetimeMs = 1400;
  const state = {
    hits: 0,
    misses: 0,
    spawned: 0,
    expired: 0,
    active: 0,
    finished: false,
    time_remaining_s: durationMs / 1000,
    last_event: "none",
  };
  const active = [];
  let rngState = seed || 1;
  let startedAt = null;
  let lastSpawnAt = 0;
  let running = false;
  let proxyLayer = [];

  function rand() {
    rngState ^= rngState << 13;
    rngState ^= rngState >>> 17;
    rngState ^= rngState << 5;
    return (rngState >>> 0) / 4294967296;
  }

  function truth() {
    state.active = active.length;
    truthNode.textContent = Object.entries(state)
      .map(([key, value]) => `${key}=${value}`)
      .join(" ");
  }

  function radiusFor(target, now) {
    const age = now - target.spawnedAt;
    const progress = Math.max(0, Math.min(1, age / lifetimeMs));
    return Math.max(2, maxRadius * Math.sin(progress * Math.PI));
  }

  function spawn(now) {
    const margin = maxRadius + 48;
    const x = margin + rand() * (canvas.width - margin * 2);
    const y = 110 + rand() * (canvas.height - 140);
    active.push({
      id: state.spawned + 1,
      x,
      y,
      spawnedAt: now,
      radius: maxRadius,
    });
    state.spawned += 1;
    lastSpawnAt = now;
    updateProxies(now);
    truth();
  }

  function updateProxies(now) {
    for (const node of proxyLayer) node.remove();
    proxyLayer = active.map(target => {
      const radius = radiusFor(target, now);
      const node = document.createElement("div");
      node.className = "target";
      node.dataset.targetId = target.id;
      node.style.left = `${target.x}px`;
      node.style.top = `${target.y}px`;
      node.style.width = `${radius * 2}px`;
      node.style.height = `${radius * 2}px`;
      node.style.marginLeft = `${-radius}px`;
      node.style.marginTop = `${-radius}px`;
      document.body.appendChild(node);
      return node;
    });
  }

  function draw(now) {
    ctx.fillStyle = "rgb(24, 24, 24)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "rgb(250, 40, 40)";
    for (const target of active) {
      const radius = radiusFor(target, now);
      target.radius = radius;
      ctx.beginPath();
      ctx.arc(target.x, target.y, radius, 0, Math.PI * 2);
      ctx.fill();
    }
  }

  function expire(now) {
    let changed = false;
    for (let index = active.length - 1; index >= 0; index -= 1) {
      if (now - active[index].spawnedAt > lifetimeMs) {
        active.splice(index, 1);
        state.expired += 1;
        changed = true;
      }
    }
    return changed;
  }

  function finishIfDone(now) {
    const elapsed = now - startedAt;
    state.time_remaining_s = Math.max(0, (durationMs - elapsed) / 1000).toFixed(3);
    if (elapsed >= durationMs && active.length === 0) {
      state.finished = true;
      running = false;
      updateProxies(now);
      truth();
      return true;
    }
    return false;
  }

  function tick(now) {
    if (!running) {
      draw(now);
      requestAnimationFrame(tick);
      return;
    }

    const elapsed = now - startedAt;
    if (elapsed < durationMs && now - lastSpawnAt >= spawnIntervalMs) {
      spawn(now);
    }
    const changed = expire(now);
    draw(now);
    updateProxies(now);
    if (changed) truth();
    finishIfDone(now);
    requestAnimationFrame(tick);
  }

  function hitAt(x, y, now) {
    let bestIndex = -1;
    let bestDistance = Infinity;
    for (let index = 0; index < active.length; index += 1) {
      const target = active[index];
      const dx = x - target.x;
      const dy = y - target.y;
      const distance = Math.sqrt(dx * dx + dy * dy);
      const radius = radiusFor(target, now) + 1;
      if (distance <= radius && distance < bestDistance) {
        bestDistance = distance;
        bestIndex = index;
      }
    }
    if (bestIndex >= 0) {
      active.splice(bestIndex, 1);
      state.hits += 1;
      updateProxies(now);
      truth();
      return true;
    }
    return false;
  }

  document.addEventListener("mousedown", event => {
    if (!running && state.finished) return;
    state.last_event = "mousedown";
    if (!hitAt(event.clientX, event.clientY, performance.now())) {
      state.misses += 1;
      truth();
    }
  });

  document.addEventListener("click", () => {
    state.last_event = "click";
    truth();
  });

  window.__saccadeStart = () => {
    if (running || state.finished) return;
    startedAt = performance.now();
    lastSpawnAt = startedAt - spawnIntervalMs;
    running = true;
    truth();
  };

  truth();
  requestAnimationFrame(tick);
})();
