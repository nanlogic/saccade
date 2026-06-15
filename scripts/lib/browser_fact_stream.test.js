const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const {
  FACT_SCHEMA,
  browserFactFromMousemaxTarget,
  factTextCorpus,
  installScript,
  summarizeFacts,
  writeFactsJsonl,
} = require("./browser_fact_stream");

test("installScript includes the fact schema and options", () => {
  const script = installScript({
    allowCanvasDebugValues: false,
    allowCanvasPixelRead: true,
    textLimit: 80,
  });
  assert.match(script, new RegExp(FACT_SCHEMA.replaceAll(".", "\\.")));
  assert.match(script, /allowCanvasDebugValues/);
  assert.match(script, /allowCanvasPixelRead/);
});

test("summarizeFacts counts generic browser fact categories", () => {
  const summary = summarizeFacts([
    {
      kind: "browser_fact",
      seq: 1,
      fact_type: "node_seen",
      privacy: "safe",
      node: { actionable: false },
    },
    {
      kind: "browser_fact",
      seq: 2,
      fact_type: "actionable_seen",
      privacy: "safe",
      node: { actionable: true },
    },
    {
      kind: "browser_fact",
      seq: 3,
      fact_type: "sensitive_field_seen",
      privacy: "redacted",
      node: { sensitivity: "ssn", value_redacted: true },
    },
    {
      kind: "browser_fact",
      seq: 4,
      fact_type: "canvas_seen",
      privacy: "safe",
      node: {},
    },
    {
      kind: "browser_fact",
      seq: 5,
      fact_type: "visual_object_seen",
      privacy: "safe",
      node: {},
      visual_object: { source: "canvas_pixel_probe" },
    },
  ]);

  assert.equal(summary.count, 5);
  assert.equal(summary.max_seq, 5);
  assert.equal(summary.by_type.node_seen, 1);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.sensitive, 1);
  assert.equal(summary.redacted, 1);
  assert.equal(summary.canvas, 1);
  assert.equal(summary.visual_objects, 1);
});

test("factTextCorpus exposes accidental raw-value leaks to checks", () => {
  const corpus = factTextCorpus([{ node: { text: "SSN", value_state: "completed_without_value" } }]);
  assert.equal(corpus.includes("123-45-6789"), false);
  assert.equal(corpus.includes("completed_without_value"), true);
});

test("writeFactsJsonl writes real newline-delimited JSON", async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "saccade-facts-"));
  const file = path.join(dir, "facts.jsonl");
  await writeFactsJsonl(file, [{ seq: 1 }, { seq: 2 }]);
  const text = await fs.readFile(file, "utf8");
  assert.equal(text, '{"seq":1}\n{"seq":2}\n');
  assert.deepEqual(
    text
      .trim()
      .split("\n")
      .map((line) => JSON.parse(line)),
    [{ seq: 1 }, { seq: 2 }],
  );
});

test("browserFactFromMousemaxTarget maps replay targets to visual facts", () => {
  const fact = browserFactFromMousemaxTarget(
    {
      id: 7,
      frame_id: 42,
      first_seen_ns: 1_000_000,
      last_seen_ns: 2_000_000,
      center_css: { x: 12, y: 34 },
      bbox_css: { x: 10, y: 30, w: 4, h: 8 },
      radius_css: 4,
      confidence: 0.95,
      source: "PixelDetector",
      clicked: false,
    },
    {
      seq: 3,
      url: "https://mouseaccuracy.com/classic/",
      reason: "tracker_appeared",
      game_area_css: { x: 0, y: 100, w: 1280, h: 700 },
    },
  );

  assert.equal(fact.schema, FACT_SCHEMA);
  assert.equal(fact.fact_type, "visual_object_seen");
  assert.equal(fact.privacy, "safe");
  assert.equal(fact.t_ms, 2);
  assert.equal(fact.visual_object.source, "mousemax_replay_target");
  assert.equal(fact.visual_object.detector_source, "PixelDetector");
  assert.equal(fact.visual_object.target_id, 7);
  assert.deepEqual(fact.visual_object.center_css, { x: 12, y: 34 });
  assert.deepEqual(fact.visual_object.game_area_css, { x: 0, y: 100, w: 1280, h: 700 });
});
