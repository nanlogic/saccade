const assert = require("node:assert/strict");
const test = require("node:test");

const {
  FACT_SCHEMA,
  factTextCorpus,
  installScript,
  summarizeFacts,
} = require("./browser_fact_stream");

test("installScript includes the fact schema and options", () => {
  const script = installScript({ allowCanvasDebugValues: false, textLimit: 80 });
  assert.match(script, new RegExp(FACT_SCHEMA.replaceAll(".", "\\.")));
  assert.match(script, /allowCanvasDebugValues/);
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
  ]);

  assert.equal(summary.count, 4);
  assert.equal(summary.max_seq, 4);
  assert.equal(summary.by_type.node_seen, 1);
  assert.equal(summary.actionable, 1);
  assert.equal(summary.sensitive, 1);
  assert.equal(summary.redacted, 1);
  assert.equal(summary.canvas, 1);
});

test("factTextCorpus exposes accidental raw-value leaks to checks", () => {
  const corpus = factTextCorpus([{ node: { text: "SSN", value_state: "completed_without_value" } }]);
  assert.equal(corpus.includes("123-45-6789"), false);
  assert.equal(corpus.includes("completed_without_value"), true);
});
