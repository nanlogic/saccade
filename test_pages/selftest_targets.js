(function () {
  const positions = [
    [220, 170],
    [900, 180],
    [640, 420],
    [340, 620],
    [1040, 590],
    [760, 300],
    [500, 520],
    [1120, 340],
    [180, 500],
  ];
  const radius = 7;

  function truth(data) {
    const text = Object.entries(data)
      .map(([k, v]) => `${k}=${v}`)
      .join(" ");
    document.getElementById("truth").textContent = text;
  }

  function isHit(event, target) {
    const dx = event.clientX - target.x;
    const dy = event.clientY - target.y;
    return Math.sqrt(dx * dx + dy * dy) <= radius + 1;
  }

  function domTargets(options) {
    const arena = document.getElementById("arena");
    const state = {hits: 0, misses: 0, spawned: 0, finished: false};
    let active = null;

    function spawn() {
      if (state.hits >= options.count) {
        state.finished = true;
        truth(state);
        return;
      }
      const [x, y] = positions[state.spawned % positions.length];
      const button = document.createElement("button");
      button.className = "target";
      button.style.left = `${x}px`;
      button.style.top = `${y}px`;
      button.setAttribute("aria-label", "target");
      active = {element: button, x, y};
      state.spawned += 1;
      truth(state);
      button.addEventListener("mousedown", event => {
        event.stopPropagation();
        if (!active) return;
        state.hits += 1;
        active.element.remove();
        active = null;
        truth(state);
        setTimeout(spawn, 80);
      });
      arena.appendChild(button);
    }

    arena.addEventListener("mousedown", event => {
      if (active && !event.target.classList.contains("target")) {
        state.misses += 1;
        truth(state);
      }
    });
    window.__saccadeStart = () => setTimeout(spawn, 80);
  }

  function domReusedTarget(options) {
    const arena = document.getElementById("arena");
    const state = {hits: 0, misses: 0, spawned: 0, finished: false};
    const button = document.createElement("button");
    button.id = "reused-target";
    button.className = "hit";
    button.style.display = "none";
    button.setAttribute("aria-label", "target");
    let active = false;

    function moveToNextPosition() {
      if (state.hits >= options.count) {
        active = false;
        state.finished = true;
        button.classList.add("hit");
        button.style.display = "none";
        truth(state);
        return;
      }
      const [x, y] = positions[state.spawned % positions.length];
      button.style.left = `${x}px`;
      button.style.top = `${y}px`;
      button.style.display = "block";
      button.className = "target";
      state.spawned += 1;
      active = true;
      truth(state);
    }

    button.addEventListener("mousedown", event => {
      event.stopPropagation();
      if (!active) return;
      state.hits += 1;
      moveToNextPosition();
    });
    arena.addEventListener("mousedown", event => {
      if (active && event.target !== button) {
        state.misses += 1;
        truth(state);
      }
    });
    arena.appendChild(button);
    window.__saccadeStart = () => setTimeout(moveToNextPosition, 80);
  }

  function svgTargets(options) {
    const arena = document.getElementById("arena");
    const state = {hits: 0, misses: 0, spawned: 0, finished: false};
    let active = null;

    function spawn() {
      if (state.hits >= options.count) {
        state.finished = true;
        truth(state);
        return;
      }
      const [x, y] = positions[state.spawned % positions.length];
      const circle = document.createElementNS("http://www.w3.org/2000/svg", "circle");
      circle.setAttribute("class", "target");
      circle.setAttribute("cx", x);
      circle.setAttribute("cy", y);
      circle.setAttribute("r", radius);
      circle.setAttribute("fill", "rgb(250, 40, 40)");
      active = {element: circle, x, y};
      state.spawned += 1;
      truth(state);
      circle.addEventListener("mousedown", event => {
        event.stopPropagation();
        state.hits += 1;
        active.element.remove();
        active = null;
        truth(state);
        setTimeout(spawn, 80);
      });
      arena.appendChild(circle);
    }

    document.addEventListener("mousedown", event => {
      if (!active || event.target.classList.contains("target")) return;
      if (isHit(event, active)) {
        state.hits += 1;
        active.element.remove();
        active = null;
        truth(state);
        setTimeout(spawn, 80);
      }
    });
    window.__saccadeStart = () => setTimeout(spawn, 80);
  }

  function canvasTargets(options) {
    const canvas = document.getElementById("arena");
    const ctx = canvas.getContext("2d");
    const sprite = document.createElement("canvas");
    sprite.width = 20;
    sprite.height = 20;
    const sctx = sprite.getContext("2d");
    sctx.fillStyle = "rgb(250, 40, 40)";
    sctx.beginPath();
    sctx.arc(10, 10, radius, 0, Math.PI * 2);
    sctx.fill();
    const state = {hits: 0, misses: 0, spawned: 0, finished: false};
    let active = null;
    let proxy = null;

    function updateProxy() {
      if (proxy) {
        proxy.remove();
        proxy = null;
      }
      if (active) {
        proxy = document.createElement("div");
        proxy.className = "target";
        proxy.style.left = `${active.x}px`;
        proxy.style.top = `${active.y}px`;
        proxy.style.pointerEvents = "none";
        document.body.appendChild(proxy);
      }
    }

    function draw() {
      ctx.fillStyle = "rgb(24, 24, 24)";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      if (active) {
        if (options.sprite) {
          ctx.drawImage(sprite, active.x - 10, active.y - 10);
        } else {
          ctx.fillStyle = "rgb(250, 40, 40)";
          ctx.beginPath();
          ctx.arc(active.x, active.y, radius, 0, Math.PI * 2);
          ctx.fill();
        }
      }
      updateProxy();
    }

    function spawn() {
      if (state.hits >= options.count) {
        state.finished = true;
        active = null;
        draw();
        truth(state);
        return;
      }
      const [x, y] = positions[state.spawned % positions.length];
      active = {x, y};
      state.spawned += 1;
      draw();
      truth(state);
    }

    document.addEventListener("mousedown", event => {
      if (!active) return;
      if (isHit(event, active)) {
        state.hits += 1;
        active = null;
        draw();
        truth(state);
        setTimeout(spawn, 80);
      } else {
        state.misses += 1;
        truth(state);
      }
    });

    draw();
    window.__saccadeStart = () => setTimeout(spawn, 80);
  }

  function overlayPage() {
    const canvas = document.getElementById("arena");
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "rgb(24, 24, 24)";
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.fillStyle = "rgb(250, 40, 40)";
    ctx.beginPath();
    ctx.arc(220, 170, radius, 0, Math.PI * 2);
    ctx.fill();
    let overlayHits = 0;
    document.getElementById("overlay").addEventListener("mousedown", () => {
      overlayHits += 1;
      truth({hits: 0, misses: 0, spawned: 1, overlay_hits: overlayHits, finished: true});
    });
  }

  function highDpiGrid() {
    const arena = document.getElementById("arena");
    const state = {hits: 0, misses: 0, spawned: 9, finished: false};
    const points = [
      [220, 160],
      [640, 160],
      [1060, 160],
      [220, 400],
      [640, 400],
      [1060, 400],
      [220, 640],
      [640, 640],
      [1060, 640],
    ];
    window.__saccadeStart = () => setTimeout(() => {
      points.forEach(([x, y], index) => {
        const dot = document.createElement("button");
        dot.className = "target";
        dot.style.left = `${x}px`;
        dot.style.top = `${y}px`;
        dot.dataset.index = index;
        dot.addEventListener("mousedown", event => {
          event.stopPropagation();
          if (!dot.classList.contains("hit")) {
            dot.classList.add("hit");
            state.hits += 1;
            state.finished = state.hits === state.spawned;
            truth(state);
          }
        });
        arena.appendChild(dot);
      });
      truth(state);
    }, 80);
    arena.addEventListener("mousedown", event => {
      if (!event.target.classList.contains("target")) {
        state.misses += 1;
        truth(state);
      }
    });
    truth(state);
  }

  window.SaccadeSelftest = {
    domTargets,
    domReusedTarget,
    svgTargets,
    canvasTargets,
    overlayPage,
    highDpiGrid,
  };
})();
