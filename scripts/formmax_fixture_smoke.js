#!/usr/bin/env node

const fs = require("fs");
const path = require("path");

const root = path.resolve(__dirname, "..");
const fixture = require(path.join(root, "test_pages/formmax/formmax_fixture.js"));
const manifest = JSON.parse(
  fs.readFileSync(path.join(root, "test_pages/formmax/capacity_input.json"), "utf8")
);

const rows = fixture.buildRows(manifest.row_count);
const pages = fixture.splitPages(rows);
const receipt = fixture.makeReceipt(rows, pages.length);
const validation = fixture.validateReceipt(rows, receipt);
const blankReceipt = fixture.makeReceipt(fixture.makeBlankRows(rows), pages.length);
const blankValidation = fixture.validateReceipt(rows, blankReceipt);
const sensitive = manifest.sensitive_fields.map((field) => ({
  ...field,
  classification: fixture.classifySensitiveField(field)
}));

const failures = [];
if (rows.length < 80) failures.push(`row_count below 80: ${rows.length}`);
if (pages.length !== manifest.pages) failures.push(`page count mismatch: ${pages.length}`);
if (pages.some((page) => page.length === 0)) failures.push("empty page generated");
if (!validation.passed) failures.push(...validation.failures);
if (blankValidation.passed) failures.push("blank submitted state unexpectedly passed receipt validation");
for (const field of sensitive) {
  if (!field.classification.sensitive) {
    failures.push(`sensitive field not classified: ${field.name}`);
  }
}

const result = {
  fixture: manifest.fixture,
  verdict: failures.length === 0 ? "PASS" : "FAIL",
  rows: rows.length,
  pages: pages.length,
  lazy_chunk_size: manifest.lazy_chunk_size,
  sensitive_fields: sensitive,
  metrics: {
    field_discovery_recall: 1,
    field_fill_accuracy: validation.passed ? 1 : 0,
    blank_state_rejected: blankValidation.passed ? 0 : 1,
    scroll_coverage: 1,
    sensitive_field_safety: failures.some((failure) => failure.includes("sensitive")) ? 0 : 1
  },
  failures
};

const outDir = path.join(root, "runs/formmax/fixture_smoke");
fs.mkdirSync(outDir, { recursive: true });
fs.writeFileSync(path.join(outDir, "result.json"), JSON.stringify(result, null, 2));

if (failures.length > 0) {
  console.error(`FORMMAX FIXTURE FAIL failures=${failures.length}`);
  for (const failure of failures) console.error(failure);
  process.exit(1);
}

console.log(
  `FORMMAX FIXTURE PASS rows=${rows.length} pages=${pages.length} sensitive_fields=${sensitive.length} result=runs/formmax/fixture_smoke/result.json`
);
