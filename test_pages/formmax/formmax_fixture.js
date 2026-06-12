(function attach(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.FormmaxFixture = factory();
  }
})(typeof globalThis !== "undefined" ? globalThis : window, function buildModule() {
  const OWNERS = ["Ari", "Mina", "Ravi", "Sol", "Theo", "Uma"];
  const FIELD_SPECS = [
    { key: "site_name", label: "Site name", kind: "text" },
    { key: "rack_count", label: "Rack count", kind: "number" },
    { key: "power_mw", label: "Power MW", kind: "number" },
    { key: "cooling_tons", label: "Cooling tons", kind: "number" },
    { key: "owner", label: "Owner", kind: "select" },
    { key: "target_date", label: "Target date", kind: "date" },
    { key: "approved", label: "Approved", kind: "checkbox" }
  ];

  function pad(num) {
    return String(num).padStart(3, "0");
  }

  function buildRows(count) {
    const rows = [];
    for (let i = 1; i <= count; i += 1) {
      rows.push({
        id: `CAP-${pad(i)}`,
        site_name: `Region ${Math.ceil(i / 8)} / Site ${pad(i)}`,
        rack_count: 8 + (i % 12),
        power_mw: Number((1.2 + (i % 9) * 0.35).toFixed(2)),
        cooling_tons: 40 + (i % 15) * 3,
        owner: OWNERS[i % OWNERS.length],
        target_date: `2026-${padMonth(1 + (i % 12))}-${padDay(1 + (i % 25))}`,
        approved: i % 3 !== 0
      });
    }
    return rows;
  }

  function padMonth(month) {
    return String(month).padStart(2, "0");
  }

  function padDay(day) {
    return String(day).padStart(2, "0");
  }

  function splitPages(rows) {
    const split = Math.ceil(rows.length / 2);
    return [rows.slice(0, split), rows.slice(split)];
  }

  function sensitiveFields() {
    return [
      { name: "tax_id", label: "Tax ID", reason: "government identifier" },
      { name: "signature", label: "Authorized signature", reason: "signature field" },
      { name: "legal_attestation", label: "Legal attestation", reason: "legal attestation" }
    ];
  }

  function classifySensitiveField(field) {
    const text = `${field.name || ""} ${field.label || ""} ${field.type || ""}`.toLowerCase();
    const rules = [
      ["password", "password"],
      ["otp", "one-time code"],
      ["ssn", "government identifier"],
      ["social security", "government identifier"],
      ["tax", "government identifier"],
      ["routing", "banking information"],
      ["bank", "banking information"],
      ["card", "payment card"],
      ["medical", "medical identifier"],
      ["signature", "signature field"],
      ["attestation", "legal attestation"],
      ["consent", "consent field"]
    ];
    const match = rules.find(([needle]) => text.includes(needle));
    return match
      ? { sensitive: true, reason: match[1] }
      : { sensitive: false, reason: null };
  }

  function makeBlankRows(rows) {
    return rows.map((row) => ({
      id: row.id,
      site_name: "",
      rack_count: null,
      power_mw: null,
      cooling_tons: null,
      owner: "",
      target_date: "",
      approved: false
    }));
  }

  function makeReceipt(rows, pageCount) {
    return {
      fixture: "formmax_capacity",
      page_count: pageCount,
      row_count: rows.length,
      rows: rows.map((row) => ({ ...row }))
    };
  }

  function validateReceipt(expectedRows, receipt) {
    const failures = [];
    const seen = new Set();
    for (const row of receipt.rows || []) {
      seen.add(row.id);
    }
    for (const expected of expectedRows) {
      const actual = (receipt.rows || []).find((row) => row.id === expected.id);
      if (!actual) {
        failures.push(`missing row ${expected.id}`);
        continue;
      }
      for (const key of Object.keys(expected)) {
        if (actual[key] !== expected[key]) {
          failures.push(`${expected.id}.${key} expected ${expected[key]} got ${actual[key]}`);
        }
      }
    }
    for (const id of seen) {
      if (!expectedRows.some((row) => row.id === id)) {
        failures.push(`unexpected row ${id}`);
      }
    }
    return {
      passed: failures.length === 0,
      failures
    };
  }

  function browserMain() {
    const rows = buildRows(96);
    const pages = splitPages(rows);
    const state = {
      page: 0,
      rendered: 0,
      chunkSize: 16,
      values: new Map(makeBlankRows(rows).map((row) => [row.id, row]))
    };

    const body = document.getElementById("capacity-body");
    const scroller = document.getElementById("table-scroll");
    const status = document.getElementById("status");
    const pageLabel = document.getElementById("page-label");
    const sectionTitle = document.getElementById("section-title");
    const submit = document.getElementById("submit-page");
    const sensitivePanel = document.getElementById("sensitive-panel");
    const receiptPanel = document.getElementById("receipt-panel");
    const receiptNode = document.getElementById("receipt");

    function renderChunk(reset) {
      if (reset) {
        body.textContent = "";
        state.rendered = 0;
        scroller.scrollTop = 0;
      }
      const pageRows = pages[state.page];
      const end = Math.min(pageRows.length, state.rendered + state.chunkSize);
      for (let i = state.rendered; i < end; i += 1) {
        body.appendChild(renderRow(pageRows[i]));
      }
      state.rendered = end;
      status.textContent = `Rendered ${state.rendered} of ${pageRows.length} rows`;
    }

    function renderRow(row) {
      const value = state.values.get(row.id);
      const tr = document.createElement("tr");
      tr.dataset.rowId = row.id;
      tr.innerHTML = `
        <td>${row.id}</td>
        <td><input name="${row.id}_site_name" data-field="site_name" value="${escapeAttr(value.site_name)}"></td>
        <td><input name="${row.id}_rack_count" data-field="rack_count" type="number" value="${value.rack_count ?? ""}"></td>
        <td><input name="${row.id}_power_mw" data-field="power_mw" type="number" step="0.01" value="${value.power_mw ?? ""}"></td>
        <td><input name="${row.id}_cooling_tons" data-field="cooling_tons" type="number" value="${value.cooling_tons ?? ""}"></td>
        <td><select name="${row.id}_owner" data-field="owner"><option value="">Choose</option>${OWNERS.map((owner) => `<option${owner === value.owner ? " selected" : ""}>${escapeText(owner)}</option>`).join("")}</select></td>
        <td><input name="${row.id}_target_date" data-field="target_date" type="date" value="${escapeAttr(value.target_date)}"></td>
        <td><input name="${row.id}_approved" data-field="approved" type="checkbox"${value.approved ? " checked" : ""}></td>
      `;
      tr.querySelectorAll("input, select").forEach((control) => {
        const eventName = control.type === "checkbox" || control.tagName === "SELECT" ? "change" : "input";
        control.addEventListener(eventName, () => updateValue(row.id, control));
      });
      return tr;
    }

    function updateValue(rowId, control) {
      const row = state.values.get(rowId);
      const field = control.dataset.field;
      const spec = FIELD_SPECS.find((candidate) => candidate.key === field);
      if (!row || !spec) return;
      if (spec.kind === "checkbox") {
        row[field] = control.checked;
      } else if (spec.kind === "number") {
        row[field] = control.value === "" ? null : Number(control.value);
      } else {
        row[field] = control.value;
      }
    }

    function submittedRows() {
      return rows.map((row) => ({ ...state.values.get(row.id) }));
    }

    function gotoPage(page) {
      state.page = page;
      pageLabel.textContent = `Page ${page + 1} of 2`;
      sectionTitle.textContent = page === 0 ? "Regional capacity table" : "Expansion capacity table";
      sensitivePanel.classList.toggle("hidden", page === 0);
      submit.textContent = page === 0 ? "Submit page" : "Submit final page";
      renderChunk(true);
    }

    scroller.addEventListener("scroll", () => {
      if (scroller.scrollTop + scroller.clientHeight >= scroller.scrollHeight - 40) {
        renderChunk(false);
      }
    });

    submit.addEventListener("click", () => {
      if (state.page === 0) {
        gotoPage(1);
        return;
      }
      const receipt = makeReceipt(submittedRows(), 2);
      receipt.sensitive_fields_present = sensitiveFields();
      receipt.validation = validateReceipt(rows, receipt);
      receiptPanel.classList.remove("hidden");
      receiptNode.textContent = JSON.stringify(receipt, null, 2);
      status.textContent = "Receipt produced";
    });

    window.__FORMMAX_FIXTURE = {
      rows,
      pages,
      fieldSpecs: FIELD_SPECS,
      sensitiveFields: sensitiveFields(),
      submittedRows,
      expectedValidation: () => validateReceipt(rows, makeReceipt(submittedRows(), 2))
    };
    gotoPage(0);
  }

  if (typeof document !== "undefined") {
    document.addEventListener("DOMContentLoaded", browserMain);
  }

  return {
    buildRows,
    splitPages,
    sensitiveFields,
    classifySensitiveField,
    makeBlankRows,
    makeReceipt,
    validateReceipt
  };

  function escapeText(value) {
    return String(value)
      .replace(/&/g, "&amp;")
      .replace(/</g, "&lt;")
      .replace(/>/g, "&gt;");
  }

  function escapeAttr(value) {
    return escapeText(value).replace(/"/g, "&quot;");
  }
});
