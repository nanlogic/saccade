const assert = require("node:assert/strict");
const test = require("node:test");

const {
  classForVisualObject,
  classifyLocalGameFacts,
  nearestPaletteColor,
  semanticFactFromVisualFact,
  summarizeSemanticFacts,
} = require("./local_game_fact_classifier");

test("nearestPaletteColor maps game colors to named palette entries", () => {
  assert.equal(nearestPaletteColor({ r: 255, g: 73, b: 111 }).name, "red");
  assert.equal(nearestPaletteColor({ r: 111, g: 107, b: 255 }).name, "blue");
  assert.equal(nearestPaletteColor({ r: 255, g: 215, b: 95 }).name, "yellow");
});

test("classForVisualObject labels small colorful objects as pickups or projectiles", () => {
  const semantic = classForVisualObject({
    avg_rgba: [255, 73, 111, 255],
    area_px: 120,
    bbox_css: { x: 10, y: 120, w: 12, h: 12 },
    center_css: { x: 16, y: 126 },
  });
  assert.equal(semantic.label, "collectible_drop_or_projectile");
  assert.equal(semantic.palette.name, "red");
  assert.equal(semantic.role.name, "drop");
  assert.equal(semantic.shape.area, 120);
});

test("classForVisualObject labels large green objects as boss or hazard candidates", () => {
  const semantic = classForVisualObject({
    avg_rgba: [66, 212, 118, 255],
    area_px: 2600,
    bbox_css: { x: 10, y: 90, w: 90, h: 70 },
    center_css: { x: 55, y: 125 },
  });
  assert.equal(semantic.label, "watermelon_boss_or_hazard");
  assert.equal(semantic.palette.name, "green");
  assert.equal(semantic.role.name, "hazard");
});

test("classForVisualObject labels fruit-sized red objects as enemies below the HUD", () => {
  const semantic = classForVisualObject({
    avg_rgba: [211, 75, 98, 255],
    area_px: 2600,
    bbox_css: { x: 84, y: 616, w: 24.5, h: 38.5 },
    center_css: { x: 96, y: 632 },
  });
  assert.equal(semantic.role.name, "enemy");
});

test("classForVisualObject labels the centered red straw as the player anchor", () => {
  const semantic = classForVisualObject({
    avg_rgba: [254, 117, 145, 255],
    area_px: 637,
    bbox_css: { x: 616, y: 287, w: 7, h: 38.5 },
    center_css: { x: 617.62, y: 304.77 },
  });
  assert.equal(semantic.role.name, "player");
});

test("semanticFactFromVisualFact preserves source object and emits semantic_object_seen", () => {
  const fact = semanticFactFromVisualFact({
    kind: "browser_fact",
    schema: "saccade.browser_fact.v0",
    seq: 9,
    t_ms: 1200,
    url: "http://127.0.0.1:4173/",
    title: "Blend or Die - Prototype",
    fact_type: "visual_object_seen",
    privacy: "safe",
    visual_object: {
      object_id: "node-3:visual-1",
      avg_rgba: [255, 215, 95, 255],
      area_px: 180,
      bbox_css: { x: 1, y: 2, w: 24, h: 8 },
      center_css: { x: 13, y: 6 },
    },
  });

  assert.equal(fact.fact_type, "semantic_object_seen");
  assert.equal(fact.source_fact_seq, 9);
  assert.equal(fact.source_object_id, "node-3:visual-1");
  assert.equal(fact.semantic.palette.name, "yellow");
});

test("classifyLocalGameFacts and summarizeSemanticFacts handle visual fact lists", () => {
  const facts = classifyLocalGameFacts([
    {
      fact_type: "visual_object_seen",
      visual_object: {
        avg_rgba: [111, 107, 255, 255],
        area_px: 500,
        bbox_css: { x: 1, y: 2, w: 26, h: 24 },
        center_css: { x: 14, y: 14 },
      },
    },
    { fact_type: "node_seen", node: { text: "ignored" } },
  ]);
  const summary = summarizeSemanticFacts(facts);
  assert.equal(facts.length, 1);
  assert.equal(summary.count, 1);
  assert.equal(summary.by_palette.blue, 1);
  assert.equal(summary.by_role.ui, 1);
});
