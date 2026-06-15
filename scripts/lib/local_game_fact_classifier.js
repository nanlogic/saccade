const { FACT_SCHEMA, browserFactEnvelope } = require("./browser_fact_stream");

function clamp01(value) {
  return Math.max(0, Math.min(1, value));
}

function rgbFromVisualObject(object) {
  const rgba = object?.avg_rgba || [];
  return {
    r: Number(rgba[0] || 0),
    g: Number(rgba[1] || 0),
    b: Number(rgba[2] || 0),
  };
}

function colorDistance(a, b) {
  return Math.sqrt(
    (a.r - b.r) ** 2 +
      (a.g - b.g) ** 2 +
      (a.b - b.b) ** 2,
  );
}

const PALETTE = {
  red: { r: 255, g: 73, b: 111 },
  melon: { r: 255, g: 63, b: 98 },
  yellow: { r: 255, g: 215, b: 95 },
  blue: { r: 111, g: 107, b: 255 },
  green: { r: 54, g: 209, b: 124 },
  ink: { r: 24, g: 50, b: 56 },
  glass: { r: 225, g: 250, b: 255 },
};

function nearestPaletteColor(rgb) {
  let best = null;
  for (const [name, color] of Object.entries(PALETTE)) {
    const distance = colorDistance(rgb, color);
    if (!best || distance < best.distance) {
      best = { name, distance };
    }
  }
  const confidence = clamp01(1 - best.distance / 180);
  return { name: best.name, distance: Math.round(best.distance * 10) / 10, confidence };
}

function shapeFeatures(object) {
  const bbox = object?.bbox_css || object?.bbox_canvas_px || {};
  const w = Number(bbox.w || 0);
  const h = Number(bbox.h || 0);
  const area = Number(object?.area_px || 0);
  const maxDim = Math.max(w, h);
  const minDim = Math.max(1, Math.min(w, h));
  const aspect = maxDim / minDim;
  return { w, h, area, maxDim, minDim, aspect };
}

function classForVisualObject(object) {
  const rgb = rgbFromVisualObject(object);
  const palette = nearestPaletteColor(rgb);
  const shape = shapeFeatures(object);
  const reasons = [];
  let label = "unknown_visual_object";
  let confidence = Math.max(0.2, palette.confidence * 0.55);

  if (palette.name === "ink" && shape.area < 700) {
    label = "enemy_seed_or_face_detail";
    confidence = 0.45 + palette.confidence * 0.25;
    reasons.push("dark_small_component");
  } else if (palette.name === "glass" && shape.maxDim >= 24) {
    label = "player_cup_or_ui_glass";
    confidence = 0.45 + palette.confidence * 0.3;
    reasons.push("light_glass_component");
  } else if (shape.maxDim >= 70 || shape.area >= 2200) {
    label = palette.name === "green" ? "watermelon_boss_or_hazard" : "large_actor_or_effect";
    confidence = 0.55 + palette.confidence * 0.3;
    reasons.push("large_component");
  } else if (shape.maxDim >= 24 && shape.area >= 360) {
    if (palette.name === "yellow" && shape.aspect >= 1.8) {
      label = "banana_enemy_or_yellow_swirl";
      reasons.push("yellow_elongated_component");
    } else if (palette.name === "blue") {
      label = "blueberry_enemy_or_blue_drop";
      reasons.push("blue_medium_component");
    } else if (palette.name === "green") {
      label = "melon_or_leaf_component";
      reasons.push("green_medium_component");
    } else {
      label = "enemy_or_player_attack";
      reasons.push("medium_actor_component");
    }
    confidence = 0.5 + palette.confidence * 0.35;
  } else if (shape.area >= 55 && shape.maxDim <= 34) {
    if (["red", "yellow", "blue", "melon", "green"].includes(palette.name)) {
      label = "collectible_drop_or_projectile";
      confidence = 0.5 + palette.confidence * 0.35;
      reasons.push("small_palette_component");
    }
    if (shape.aspect >= 2.2) {
      label = "projectile_or_particle_streak";
      confidence = Math.max(confidence, 0.55);
      reasons.push("elongated_small_component");
    }
  } else if (shape.area < 55) {
    label = "particle_or_tiny_detail";
    confidence = 0.35 + palette.confidence * 0.2;
    reasons.push("tiny_component");
  }

  return {
    label,
    confidence: Math.round(clamp01(confidence) * 1000) / 1000,
    palette,
    shape,
    reasons,
  };
}

function semanticFactFromVisualFact(fact, options = {}) {
  if (fact?.fact_type !== "visual_object_seen" || !fact.visual_object) {
    return null;
  }
  const semantic = classForVisualObject(fact.visual_object);
  return browserFactEnvelope(
    "semantic_object_seen",
    {
      reason: options.reason || "local_game_visual_classifier",
      source_fact_seq: fact.seq ?? null,
      source_object_id: fact.visual_object.object_id || null,
      visual_object: fact.visual_object,
      semantic,
    },
    {
      seq: options.seq,
      t_ms: fact.t_ms ?? null,
      url: fact.url ?? null,
      title: fact.title || "Blend or Die - Prototype",
      privacy: "safe",
    },
  );
}

function classifyLocalGameFacts(facts, options = {}) {
  const semanticFacts = [];
  for (const fact of facts) {
    const semantic = semanticFactFromVisualFact(fact, {
      ...options,
      seq: semanticFacts.length + 1,
    });
    if (semantic) semanticFacts.push(semantic);
  }
  return semanticFacts;
}

function summarizeSemanticFacts(facts) {
  const byLabel = {};
  const byPalette = {};
  for (const fact of facts) {
    const label = fact.semantic?.label || "unknown";
    const palette = fact.semantic?.palette?.name || "unknown";
    byLabel[label] = (byLabel[label] || 0) + 1;
    byPalette[palette] = (byPalette[palette] || 0) + 1;
  }
  return {
    count: facts.length,
    schema: FACT_SCHEMA,
    by_label: byLabel,
    by_palette: byPalette,
  };
}

module.exports = {
  PALETTE,
  classForVisualObject,
  classifyLocalGameFacts,
  nearestPaletteColor,
  semanticFactFromVisualFact,
  summarizeSemanticFacts,
};
