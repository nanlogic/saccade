# Saccade Forms and PDF Focus Plan

Date: 2026-07-11

## Product statement

Saccade completes large web and PDF forms quickly while the user keeps control
of sensitive information, signatures, legal attestations, payments, and final
submission. The agent receives field purpose and completion status, not the
user's protected values.

This is the product wedge. Servo, Chrome compatibility, MCP, truth, action maps,
and replay are implementation layers behind it.

## Why this is useful

The existing FORMMAX gate already fills 672 ordinary controls across 96 rows and
two pages while blocking three sensitive fields. The current PDF feasibility
gate detects five AcroForm fields, fills ordinary fields programmatically, keeps
tax ID/signature/attestation empty, and reports flat PDFs as unsupported.

The workflow appears in government, insurance, healthcare, finance, legal,
vendor onboarding, compliance, RFP, expense, HR, and admin systems. Many forms
combine repeated ordinary data with a small number of sensitive or legally
meaningful fields. That split matches Saccade's ownership model.

## Competitive boundary

Adobe Acrobat already offers AI document analysis and PDF tasks. Adobe PDF
Services and open-source libraries such as pypdf can import or update AcroForm
fields. OpenAI and other model products can analyze PDF text and images.

Saccade should not claim that PDF filling is new. Its differentiated claim is:

- one field-map pass followed by deterministic bulk fill;
- local or user-controlled document processing;
- no raw sensitive values in agent truth, logs, replay, or screenshots;
- the same ownership policy across web forms and PDFs;
- verification of every write and a reviewable before/after field-state diff;
- explicit unsupported results for flat, scanned, encrypted, or XFA files;
- no automatic signature, attestation, upload, payment, or submission.

Sources:

- [Adobe Acrobat AI Assistant](https://helpx.adobe.com/acrobat/using/get-ai-generated-answers.html)
- [Adobe PDF Services form-data import](https://developer.adobe.com/document-services/docs/overview/pdf-services-api/howtos/import-pdf-form-data/)
- [pypdf form interactions](https://pypdf.readthedocs.io/en/3.17.3/user/forms.html)
- [OpenAI file workflows](https://openai.com/academy/working-with-files/)

## FORMMAX product path

### F1 Inspect once

Create a normalized field inventory for the current form:

- stable field ID and page/section;
- label, type, options, constraints, and current completion state;
- owner: agent, human, or unknown;
- sensitivity kind and side-effect relationship;
- confidence and evidence used for classification.

Do not return raw values for human-owned sensitive controls.

### F2 Compile a fill plan

The LLM maps user-approved source data to field IDs once. Saccade validates the
plan, freezes the page revision, and rejects ambiguous, sensitive, hidden,
disabled, and already-completed targets.

### F3 Execute quickly

Use native browser input where required and a bounded deterministic batch for
the remaining eligible controls. Scrolling, virtualized rows, pagination,
dropdowns, dates, radio groups, and validation errors belong in the executor,
not in repeated LLM turns.

### F4 Verify and hand off

Re-read field state, report missing or rejected fields, and show the user the
remaining sensitive and side-effect steps. The user can edit any result. Submit
remains a separate user action or a point-of-risk confirmation.

## DOCMAX product path

### P1 Classify

Detect before processing:

- AcroForm: supported first;
- static XFA: inspect, then route until tested;
- dynamic XFA: unsupported initially;
- flat text PDF: extraction/review only;
- scanned PDF: OCR-assisted mapping later;
- encrypted or signed PDF: preserve and report restrictions.

### P2 Inspect fields

Return field names, types, page/rectangle, required state, option lists,
ownership, sensitivity, and completion status. Do not return protected values.

### P3 Fill a copy

Never overwrite the source file. Write ordinary fields into a new PDF, preserve
form metadata and appearance settings, and leave protected fields untouched.

### P4 Verify rendering

Re-open the output, verify field values internally, render affected pages, and
produce a redacted state diff. If the PDF viewer would not display the updated
appearance, report failure instead of returning a misleading file.

### P5 Human completion

Open the output for user review. The user enters tax/government IDs, payment
details, signatures, initials, consent, and legal attestations. Saccade may
report `completed_without_value`; it must not read those values back.

## Measurements

Every benchmark records:

- task success and field-level success;
- wall time to first useful plan and completed draft;
- LLM input/output tokens;
- browser/PDF actions and retries;
- user corrections;
- blocked sensitive fields and false positives;
- raw-value leaks in logs, replay, truth, URLs, and screenshots;
- output rendering and receipt verification.

Compare against manual entry, Playwright MCP/Chrome DevTools MCP for web forms,
and a plain pypdf script for AcroForm. Adobe can serve as a product reference,
not an automated benchmark unless its public interface permits one.

## Release order

1. Redact raw ordinary values from the existing PDF feasibility result.
2. Add `pdf inspect`, `pdf fill-plan`, `pdf fill`, and `pdf verify` commands.
3. Run local AcroForm positive/negative fixtures.
4. Run a blank public form using synthetic data; keep identity/signature fields
   empty and do not submit or upload it.
5. Generalize FORMMAX field inventory and compiled fill plan.
6. Measure two real web forms with a human review checkpoint.
7. Publish the web/PDF comparison report and dogfood package.

## Non-goals

- replacing Acrobat as a general PDF editor;
- OCR for every scanned document in the first release;
- signing on behalf of the user;
- submitting government, legal, medical, or financial forms;
- bypassing document permissions or website controls;
- claiming compatibility without an artifact from the exact form or PDF class.
