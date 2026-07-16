# Saccade DOCMAX PDF And Sensitive Gate Report

Date: 2026-07-15

## Result

DOCMAX now has reusable `inspect` and `fill` commands plus a value-free gate.

Command:

```bash
python3 scripts/probe_docmax.py \
  --output-dir runs/docmax/product_gate_final
```

The official blank-form inventory was measured separately without writing:

```bash
python3 scripts/probe_docmax.py \
  --output-dir runs/docmax/product_gate_2 \
  --public-url https://www.irs.gov/pub/irs-pdf/fw9.pdf
```

The local gate generates:

- a fillable AcroForm PDF,
- a flat PDF with no form fields,
- a completed PDF with only non-sensitive fields filled,
- a result JSON report.

It fills only `full_name` and `capacity_mw`. Tax ID, signature, and legal
attestation remain user-owned. The output PDF visually renders both ordinary
values while all three protected fields remain empty.

The same inventory path read the official blank IRS W-9 as a six-page
AcroForm with 27 field nodes. Twenty-four fields lacked reliable semantic
labels and were conservatively classified `unknown_requires_human`; DOCMAX did
not write the public form.

Flat or scanned PDFs report `no_fillable_fields` rather than faking a fill.
Browser PDF-viewer automation remains outside this gate; DOCMAX operates on
the PDF document and returns a verified output artifact.

## Acceptance

DOCMAX passes when the command reports `DOCMAX_GATE verdict=PASS` and writes:

`/Users/waynema/Documents/GitHub/SACCADE/runs/docmax/product_gate_final/report.json`

Observed output:

```text
DOCMAX_GATE verdict=PASS report=runs/docmax/product_gate_final/report.json
```

The public W-9 read-only evidence remains in
`runs/docmax/product_gate_2/report.json`.
