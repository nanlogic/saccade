const assert = require("node:assert/strict");
const fs = require("node:fs/promises");
const os = require("node:os");
const path = require("node:path");
const test = require("node:test");

const {
  appendJsonl,
  readJsonl,
  summarizeGameSamples,
  summarizeReceipts,
  summarizeReflexEvents,
} = require("./reflex_live_bridge");

test("appendJsonl and readJsonl preserve command order", async () => {
  const dir = await fs.mkdtemp(path.join(os.tmpdir(), "saccade-reflex-live-"));
  const file = path.join(dir, "commands.jsonl");

  await appendJsonl(file, { id: "ping-1", type: "ping" });
  await appendJsonl(file, {
    id: "drag-1",
    type: "drag",
    start: { x: 1, y: 2 },
    end: { x: 3, y: 4 },
    frames: 5,
  });

  assert.deepEqual(await readJsonl(file), [
    { id: "ping-1", type: "ping" },
    {
      id: "drag-1",
      type: "drag",
      start: { x: 1, y: 2 },
      end: { x: 3, y: 4 },
      frames: 5,
    },
  ]);
});

test("summaries count receipts and reflex frame events", () => {
  const receipts = summarizeReceipts([
    { type: "ping", status: "ok" },
    { type: "drag", status: "scheduled" },
    { type: "drag_phase", status: "dispatched", dispatch_ns: 20_000 },
    { type: "drag_phase", status: "dispatched", dispatch_ns: 40_000 },
  ]);
  assert.equal(receipts.count, 4);
  assert.equal(receipts.by_type_status["ping:ok"], 1);
  assert.equal(receipts.by_type_status["drag_phase:dispatched"], 2);
  assert.deepEqual(receipts.dispatch_ms, {
    count: 2,
    min: 0.02,
    p50: 0.02,
    p95: 0.02,
    max: 0.04,
  });

  const frames = summarizeReflexEvents([
    {
      kind: "saccade_reflex_frame",
      readback_ok: true,
      readback_ns: 2_000_000,
      sample_count: 1000,
      sample_saturated: 4,
      sample_max_channel_range: 120,
      sample_luma_range: 70,
      dropped_logs: 0,
    },
    { kind: "saccade_reflex_test_drag", dispatch_ns: 20_000 },
    {
      kind: "saccade_reflex_frame",
      readback_ok: true,
      readback_ns: 6_000_000,
      sample_count: 1000,
      sample_saturated: 1,
      sample_max_channel_range: 90,
      sample_luma_range: 60,
      dropped_logs: 1,
    },
  ]);
  assert.equal(frames.event_count, 3);
  assert.equal(frames.count, 2);
  assert.equal(frames.readback_ok, 2);
  assert.equal(frames.bridge_input_events, 1);
  assert.equal(frames.dropped_logs_max, 1);
  assert.equal(frames.foreground_present, true);
  assert.equal(frames.foreground_route, "readback_foreground_present");
  assert.equal(frames.max_channel_range, 120);
  assert.equal(frames.max_luma_range, 70);
  assert.equal(frames.max_saturated_ratio, 0.004);
  assert.deepEqual(frames.readback_ms, {
    count: 2,
    min: 2,
    p50: 2,
    p95: 2,
    max: 6,
  });
});

test("summarizeGameSamples reports time scale and camera delta", () => {
  const summary = summarizeGameSamples(
    [
      { wall_ms: 100, debug: { time: 1.0, camera: { x: 10, y: 20 } } },
      { wall_ms: 1100, debug: { time: 2.0, camera: { x: 30, y: 25 } } },
    ],
    0,
    1500,
  );

  assert.equal(summary.debug_sample_count, 2);
  assert.equal(summary.time_scale, 1);
  assert.deepEqual(summary.camera_delta, { x: 20, y: 5 });
});
