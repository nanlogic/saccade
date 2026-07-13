# AI-034 GitHub Human/Agent Agreement

Date: 2026-07-13
Status: task-surface gate complete; native account-menu hit-test remains routed

## Result

GitHub exposed two independent agreement failures. A writable control can
belong to the wrong task, and a visible control can fail native hit-testing.
Saccade now reports both cases without reading field values or capturing the
logged-in page.

| Canary | Result | Route |
| --- | --- | --- |
| GitHub Dashboard while the host expects `github_issue` | `red`; current URL is not an Issue authoring surface | `navigate_task_surface` |
| `https://github.com/servo/servo/issues/new` with `github_issue` intent | `green`; 25 fields, 3 eligible, 2 visible authoring editors, same revision | `servo` under normal field policy |
| Logged-in Gist account menu at three viewport sizes | Workflow passes through the narrow userscript; native hit-test is 0/3 and shim hit-test is 3/3 | `servo_with_github_pointer_shim` |

The New Issue result is page- and repository-specific. Earlier dogfood on a
different repository returned only zero-rect backing editors and correctly
routed to Chrome compatibility. Saccade must measure the current page rather
than assign one compatibility label to all GitHub Issue forms.

## Contract Change

`saccade.web.render_preflight` accepts an optional `expected_surface`:

```json
{"expected_surface":"github_issue"}
```

Supported values are `page`, `github_issue`, and `github_discussion`. The
bridge validates the current URL before it treats unrelated eligible fields as
evidence for the task. A GitHub Dashboard Copilot input can no longer make an
Issue task green.

The account-menu probe now records native and shim hit-test accuracy
separately. A verified shim returns `ROUTE_COMPATIBILITY`, not native green.

## Evidence

- `runs/ai034_human_agent_agreement/github_dashboard_expected_issue_20260713/report.json`
- `runs/ai034_human_agent_agreement/github_new_issue_expected_issue_20260713/report.json`
- `runs/ai034_human_agent_agreement/github_account_menu_agreement_20260713/report.json`

All three runs used the persistent Saccade profile. They returned no field
values, cookies, storage, or screenshot pixels. The account-menu result has
`full_agreement_measured=false` because logged-in screenshots remain disabled
by default.

## Remaining Work

1. Run the same task-scoped preflight on the user's real repository Issue form.
2. Add user-authorized, pre-task screenshot evidence only when the page contains
   no protected values.
3. Keep the account-menu shim visible in compatibility evidence until Servo's
   native hit-test reaches 3/3.
