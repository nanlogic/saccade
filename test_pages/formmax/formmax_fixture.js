(function attach(root, factory) {
  if (typeof module === "object" && module.exports) {
    module.exports = factory();
  } else {
    root.FormmaxFixture = factory();
  }
})(typeof globalThis !== "undefined" ? globalThis : window, function buildModule() {
  const OWNERS = ["Ari", "Mina", "Ravi", "Sol", "Theo", "Uma"];

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
      chunkSize: 16
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
      const tr = document.createElement("tr");
      tr.dataset.rowId = row.id;
      tr.innerHTML = `
        <td>${row.id}</td>
        <td><input name="${row.id}_site_name" value="${row.site_name}"></td>
        <td><input name="${row.id}_rack_count" type="number" value="${row.rack_count}"></td>
        <td><input name="${row.id}_power_mw" type="number" step="0.01" value="${row.power_mw}"></td>
        <td><input name="${row.id}_cooling_tons" type="number" value="${row.cooling_tons}"></td>
        <td><select name="${row.id}_owner">${OWNERS.map((owner) => `<option${owner === row.owner ? " selected" : ""}>${owner}</option>`).join("")}</select></td>
        <td><input name="${row.id}_target_date" type="date" value="${row.target_date}"></td>
        <td><input name="${row.id}_approved" type="checkbox"${row.approved ? " checked" : ""}></td>
      `;
      return tr;
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
      const receipt = makeReceipt(rows, 2);
      receipt.sensitive_fields_present = sensitiveFields();
      receiptPanel.classList.remove("hidden");
      receiptNode.textContent = JSON.stringify(receipt, null, 2);
      status.textContent = "Receipt produced";
    });

    window.__FORMMAX_FIXTURE = {
      rows,
      pages,
      sensitiveFields: sensitiveFields()
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
    makeReceipt,
    validateReceipt
  };
});
