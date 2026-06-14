#!/usr/bin/env python3
import argparse
import datetime as dt
import html
import json
import pathlib
import shutil
import subprocess
import time


WORKSPACE = pathlib.Path(__file__).resolve().parents[1]
DEFAULT_SUITE = WORKSPACE / "eval" / "agent_compare" / "tasks.json"
DEFAULT_SCHEMA = WORKSPACE / "eval" / "agent_compare" / "result_schema.json"
AGENTS = ("codex", "claude")
VERDICTS = ("pass", "fail", "blocked", "error")


def main():
    args = parse_args()
    if args.command == "list-tasks":
        suite = load_suite(args.suite)
        print_tasks(suite, args.format)
    elif args.command == "plan":
        suite = load_suite(args.suite)
        tasks = select_tasks(suite, args.tasks)
        print_plan(suite, tasks)
    elif args.command == "run":
        suite = load_suite(args.suite)
        tasks = select_tasks(suite, args.tasks)
        agents = AGENTS if args.agent == "both" else (args.agent,)
        run_dir = pathlib.Path(args.out).resolve() if args.out else default_run_dir("run")
        run_dir.mkdir(parents=True, exist_ok=True)
        write_plan(run_dir, suite, tasks, agents, args)
        if not args.execute:
            print(f"AGENT COMPARE PLAN READY run_dir={run_dir}")
            print("Add --execute to launch Codex/Claude agent runs.")
            return
        run_agents(run_dir, suite, tasks, agents, args)
        write_report(run_dir)
        print(f"AGENT COMPARE RUN COMPLETE report={run_dir / 'summary.md'}")
    elif args.command == "report":
        run_dir = pathlib.Path(args.run_dir).resolve()
        write_report(run_dir)
        print(f"AGENT COMPARE REPORT READY report={run_dir / 'summary.md'}")
    elif args.command == "selftest":
        run_dir = pathlib.Path(args.out).resolve() if args.out else default_run_dir("selftest")
        run_dir.mkdir(parents=True, exist_ok=True)
        write_synthetic_results(run_dir)
        write_report(run_dir)
        validate_selftest_report(run_dir)
        print(f"AGENT COMPARE SELFTEST PASS report={run_dir / 'summary.md'}")
    else:
        raise SystemExit(f"unknown command: {args.command}")


def parse_args():
    parser = argparse.ArgumentParser(
        description="Run and report Codex vs Claude comparisons over Saccade gauntlet tasks."
    )
    sub = parser.add_subparsers(dest="command", required=True)

    list_parser = sub.add_parser("list-tasks", help="List task ids from a suite file.")
    list_parser.add_argument("--suite", default=str(DEFAULT_SUITE))
    list_parser.add_argument("--format", choices=("text", "json"), default="text")

    plan_parser = sub.add_parser("plan", help="Print the exact task plan without running agents.")
    plan_parser.add_argument("--suite", default=str(DEFAULT_SUITE))
    plan_parser.add_argument("--tasks", nargs="+", default=["all"])

    run_parser = sub.add_parser("run", help="Run Codex, Claude, or both over selected tasks.")
    run_parser.add_argument("--suite", default=str(DEFAULT_SUITE))
    run_parser.add_argument("--schema", default=str(DEFAULT_SCHEMA))
    run_parser.add_argument("--agent", choices=("codex", "claude", "both"), default="both")
    run_parser.add_argument("--tasks", nargs="+", default=["all"])
    run_parser.add_argument("--out")
    run_parser.add_argument("--execute", action="store_true", help="Actually launch agent CLIs.")
    run_parser.add_argument(
        "--dangerous",
        action="store_true",
        help="Use full permission modes for agent CLIs. Keep off unless the machine is externally sandboxed.",
    )
    run_parser.add_argument("--codex-model")
    run_parser.add_argument(
        "--codex-sandbox",
        choices=("read-only", "workspace-write", "danger-full-access"),
        default="danger-full-access",
        help="Codex sandbox for benchmark tasks. Saccade browser tests bind localhost, so danger-full-access is the practical default.",
    )
    run_parser.add_argument("--claude-model")
    run_parser.add_argument("--claude-budget-usd", type=float, default=5.0)
    run_parser.add_argument("--max-output-bytes", type=int, default=500000)

    report_parser = sub.add_parser("report", help="Regenerate summary and charts for a run dir.")
    report_parser.add_argument("run_dir")

    selftest_parser = sub.add_parser("selftest", help="Generate synthetic records and verify charts.")
    selftest_parser.add_argument("--out")
    return parser.parse_args()


def load_suite(path):
    suite_path = pathlib.Path(path).resolve()
    suite = json.loads(suite_path.read_text())
    suite["_path"] = str(suite_path)
    ids = [task["id"] for task in suite.get("tasks", [])]
    if len(ids) != len(set(ids)):
        raise SystemExit("task ids must be unique")
    return suite


def select_tasks(suite, requested):
    tasks = suite.get("tasks", [])
    if requested == ["all"]:
        return tasks
    by_id = {task["id"]: task for task in tasks}
    selected = []
    missing = []
    for item in requested:
        if item in by_id:
            selected.append(by_id[item])
        else:
            missing.append(item)
    if missing:
        raise SystemExit(f"unknown task ids: {', '.join(missing)}")
    return selected


def print_tasks(suite, output_format):
    tasks = suite.get("tasks", [])
    if output_format == "json":
        print(json.dumps(tasks, indent=2, sort_keys=True))
        return
    for task in tasks:
        print(
            f"{task['id']}\t{task.get('suite', '')}\t"
            f"{task.get('risk', '')}\t{task.get('timeout_sec', '')}s\t{task.get('title', '')}"
        )


def print_plan(suite, tasks):
    print(f"# {suite.get('name', 'agent-compare')}")
    print()
    for index, task in enumerate(tasks, 1):
        print(f"{index}. {task['id']} ({task.get('suite', 'unknown')}, {task.get('timeout_sec')}s)")
        for command in task.get("commands", []):
            print(f"   - {command}")


def default_run_dir(prefix):
    return WORKSPACE / "runs" / "agent_compare" / f"{prefix}_{unix_ms()}"


def write_plan(run_dir, suite, tasks, agents, args):
    plan = {
        "schema_version": 1,
        "created_at": now_iso(),
        "workspace": str(WORKSPACE),
        "suite": suite.get("name"),
        "suite_file": suite.get("_path"),
        "agents": list(agents),
        "tasks": tasks,
        "execute": bool(args.execute),
        "dangerous": bool(getattr(args, "dangerous", False)),
    }
    (run_dir / "plan.json").write_text(json.dumps(plan, indent=2, sort_keys=True) + "\n")
    lines = [f"# Agent Compare Plan", "", f"Created: {plan['created_at']}", ""]
    lines.append("| Agent | Task | Suite | Timeout |")
    lines.append("| --- | --- | --- | --- |")
    for agent in agents:
        for task in tasks:
            lines.append(
                f"| {agent} | {task['id']} | {task.get('suite', '')} | {task.get('timeout_sec', '')}s |"
            )
    (run_dir / "plan.md").write_text("\n".join(lines) + "\n")


def run_agents(run_dir, suite, tasks, agents, args):
    results_path = run_dir / "results.jsonl"
    schema_path = pathlib.Path(args.schema).resolve()
    for task in tasks:
        for agent in agents:
            started = time.monotonic()
            record = run_single_agent_task(run_dir, task, agent, schema_path, args)
            record["wall_time_sec"] = round(time.monotonic() - started, 3)
            with results_path.open("a") as fh:
                fh.write(json.dumps(record, sort_keys=True) + "\n")


def run_single_agent_task(run_dir, task, agent, schema_path, args):
    prompt = build_prompt(task)
    agent_dir = run_dir / "raw" / agent
    agent_dir.mkdir(parents=True, exist_ok=True)
    raw_path = agent_dir / f"{task['id']}.raw"
    final_path = agent_dir / f"{task['id']}.final.json"
    cmd = build_agent_command(agent, schema_path, final_path, args)
    started_at = now_iso()
    try:
        proc = subprocess.run(
            cmd,
            cwd=WORKSPACE,
            input=prompt,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=int(task.get("timeout_sec", 300)) + 120,
        )
        raw_text = truncate(proc.stdout, args.max_output_bytes)
        raw_path.write_text(raw_text)
        parsed = parse_agent_output(agent, raw_text, final_path)
        verdict = parsed.get("verdict") if parsed.get("verdict") in VERDICTS else "error"
        if parsed.get("is_error") is True and parsed.get("api_error_status") == 403:
            verdict = "blocked"
        error = None if proc.returncode == 0 else f"agent_cli_exit={proc.returncode}"
        if error and verdict == "pass":
            verdict = "error"
        return base_record(
            agent=agent,
            task=task,
            started_at=started_at,
            verdict=verdict,
            command=cmd,
            raw_path=raw_path,
            final_path=final_path,
            parsed=parsed,
            error=error,
            raw_text=raw_text,
        )
    except subprocess.TimeoutExpired as error:
        raw_path.write_text(truncate(error.stdout or "", args.max_output_bytes))
        return base_record(
            agent=agent,
            task=task,
            started_at=started_at,
            verdict="error",
            command=cmd,
            raw_path=raw_path,
            final_path=final_path,
            parsed={},
            error=f"agent_cli_timeout_after={task.get('timeout_sec')}s",
            raw_text=raw_path.read_text(),
        )


def build_agent_command(agent, schema_path, final_path, args):
    if agent == "codex":
        cmd = [
            "codex",
            "exec",
            "--json",
            "--cd",
            str(WORKSPACE),
            "--sandbox",
            args.codex_sandbox,
            "--output-schema",
            str(schema_path),
            "--output-last-message",
            str(final_path),
        ]
        if args.dangerous:
            cmd.append("--dangerously-bypass-approvals-and-sandbox")
        if args.codex_model:
            cmd.extend(["--model", args.codex_model])
        return cmd
    if agent == "claude":
        schema_text = minified_json(json.loads(schema_path.read_text()))
        cmd = [
            "claude",
            "-p",
            "--output-format",
            "json",
            "--json-schema",
            schema_text,
            "--max-budget-usd",
            str(args.claude_budget_usd),
            "--no-session-persistence",
            "--add-dir",
            str(WORKSPACE),
        ]
        if args.dangerous:
            cmd.extend(["--permission-mode", "bypassPermissions"])
        else:
            cmd.extend(
                [
                    "--permission-mode",
                    "auto",
                    "--allowedTools",
                    "Bash(cargo *),Bash(RUST_LOG=* cargo *),Bash(scripts/*),Read,Grep,Glob",
                ]
            )
        if args.claude_model:
            cmd.extend(["--model", args.claude_model])
        return cmd
    raise ValueError(agent)


def build_prompt(task):
    return "\n".join(
        [
            "You are running one Saccade benchmark task for an agent comparison.",
            "Do not edit files except artifacts naturally emitted by the listed commands.",
            "Do not run unrelated tests. Do not change git state.",
            "Run the command list exactly as written. If a command contains an instruction to parse a run directory, do that minimal parsing and run the follow-up command.",
            "Return only JSON that matches the provided schema.",
            "",
            "Task:",
            json.dumps(task, indent=2, sort_keys=True),
            "",
            "Result rules:",
            "- verdict=pass only if the success regex is satisfied and required follow-up checks pass.",
            "- Put any generated report/replay/screenshot paths in artifact_paths.",
            "- Keep observations concise and factual.",
        ]
    )


def parse_agent_output(agent, raw_text, final_path):
    parsed = {}
    if final_path.exists():
        parsed = try_json(final_path.read_text()) or {}
    if not parsed and agent == "claude":
        result = try_json(raw_text) or {}
        parsed = try_json(result.get("result", "")) or result
    if not parsed and agent == "codex":
        for line in reversed(raw_text.splitlines()):
            value = try_json(line)
            if isinstance(value, dict):
                message = value.get("message") or value.get("last_message") or value.get("content")
                parsed = try_json(message or "") or {}
                if parsed:
                    break
    return parsed if isinstance(parsed, dict) else {}


def base_record(agent, task, started_at, verdict, command, raw_path, final_path, parsed, error, raw_text):
    usage = extract_usage(raw_text)
    parsed_error = parsed.get("result") if parsed.get("is_error") is True else None
    if parsed_error and not error:
        error = str(parsed_error)
    parsed_errors = []
    if parsed_error:
        parsed_errors.append(str(parsed_error))
    record = {
        "schema_version": 1,
        "run_kind": "real",
        "started_at": started_at,
        "agent": agent,
        "agent_version": agent_version(agent),
        "task_id": task["id"],
        "suite": task.get("suite"),
        "risk": task.get("risk"),
        "verdict": verdict,
        "wall_time_sec": None,
        "input_tokens": usage.get("input_tokens"),
        "output_tokens": usage.get("output_tokens"),
        "total_tokens": usage.get("total_tokens"),
        "llm_events": usage.get("llm_events"),
        "commands_run": parsed.get("commands_run", []),
        "artifact_paths": parsed.get("artifact_paths", []),
        "observations": parsed.get("observations", []),
        "errors": list(parsed.get("errors", [])) + parsed_errors,
        "agent_cli_command": redact_command(command),
        "raw_output": str(raw_path),
        "final_output": str(final_path) if final_path.exists() else None,
        "error": error,
    }
    if error:
        record["errors"] = list(record["errors"]) + [error]
    return record


def agent_version(agent):
    exe = shutil.which(agent)
    if not exe:
        return None
    version_arg = "--version" if agent == "codex" else "--version"
    try:
        proc = subprocess.run(
            [exe, version_arg],
            cwd=WORKSPACE,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=10,
        )
        return proc.stdout.strip().splitlines()[0] if proc.stdout.strip() else None
    except Exception:
        return None


def redact_command(cmd):
    redacted = []
    skip_next = False
    for item in cmd:
        if skip_next:
            redacted.append("<redacted>")
            skip_next = False
            continue
        redacted.append(item)
        if item in {"--json-schema"}:
            skip_next = True
    return redacted


def extract_usage(raw_text):
    usage = {
        "input_tokens": None,
        "output_tokens": None,
        "total_tokens": None,
        "llm_events": 0,
    }
    max_input = 0
    max_output = 0
    max_total = 0
    for line in raw_text.splitlines():
        value = try_json(line)
        if not value:
            continue
        usage["llm_events"] += 1
        found = []
        collect_token_dicts(value, found)
        for item in found:
            in_tokens = number_from_any(
                item,
                ["input_tokens", "prompt_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"],
            )
            out_tokens = number_from_any(item, ["output_tokens", "completion_tokens"])
            total_tokens = number_from_any(item, ["total_tokens"])
            if in_tokens is not None:
                max_input = max(max_input, in_tokens)
            if out_tokens is not None:
                max_output = max(max_output, out_tokens)
            if total_tokens is not None:
                max_total = max(max_total, total_tokens)
    if max_total == 0:
        single = try_json(raw_text)
        if single:
            found = []
            collect_token_dicts(single, found)
            for item in found:
                in_tokens = number_from_any(
                    item,
                    ["input_tokens", "prompt_tokens", "cache_creation_input_tokens", "cache_read_input_tokens"],
                )
                out_tokens = number_from_any(item, ["output_tokens", "completion_tokens"])
                total_tokens = number_from_any(item, ["total_tokens"])
                if in_tokens is not None:
                    max_input = max(max_input, in_tokens)
                if out_tokens is not None:
                    max_output = max(max_output, out_tokens)
                if total_tokens is not None:
                    max_total = max(max_total, total_tokens)
    usage["input_tokens"] = max_input or None
    usage["output_tokens"] = max_output or None
    usage["total_tokens"] = max_total or (
        max_input + max_output if max_input or max_output else None
    )
    if usage["llm_events"] == 0 and raw_text.strip():
        usage["llm_events"] = 1
    return usage


def collect_token_dicts(value, found):
    if isinstance(value, dict):
        if any(key.endswith("_tokens") or key in {"prompt_tokens", "completion_tokens"} for key in value):
            found.append(value)
        for child in value.values():
            collect_token_dicts(child, found)
    elif isinstance(value, list):
        for child in value:
            collect_token_dicts(child, found)


def number_from_any(mapping, keys):
    total = 0
    seen = False
    for key in keys:
        value = mapping.get(key)
        if isinstance(value, (int, float)):
            total += int(value)
            seen = True
    return total if seen else None


def write_report(run_dir):
    results = load_results(run_dir)
    if not results:
        raise SystemExit(f"no results found in {run_dir / 'results.jsonl'}")
    charts_dir = run_dir / "charts"
    charts_dir.mkdir(parents=True, exist_ok=True)
    summary = summarize_results(results)
    (run_dir / "summary.json").write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    write_charts(charts_dir, results)
    write_summary_md(run_dir, results, summary)


def load_results(run_dir):
    path = run_dir / "results.jsonl"
    if not path.exists():
        return []
    results = []
    for line in path.read_text().splitlines():
        if line.strip():
            results.append(json.loads(line))
    return results


def summarize_results(results):
    summary = {
        "created_at": now_iso(),
        "records": len(results),
        "agents": {},
        "suites": {},
        "tasks": {},
        "synthetic": all(result.get("run_kind") == "synthetic" for result in results),
    }
    for agent in sorted({r["agent"] for r in results}):
        rows = [r for r in results if r["agent"] == agent]
        summary["agents"][agent] = aggregate(rows)
    for suite in sorted({r.get("suite") or "unknown" for r in results}):
        rows = [r for r in results if (r.get("suite") or "unknown") == suite]
        summary["suites"][suite] = aggregate(rows)
    for task_id in sorted({r["task_id"] for r in results}):
        rows = [r for r in results if r["task_id"] == task_id]
        summary["tasks"][task_id] = aggregate(rows)
    return summary


def aggregate(rows):
    completed = len(rows)
    passed = sum(1 for r in rows if r.get("verdict") == "pass")
    wall_times = [r.get("wall_time_sec") for r in rows if isinstance(r.get("wall_time_sec"), (int, float))]
    tokens = [r.get("total_tokens") for r in rows if isinstance(r.get("total_tokens"), (int, float))]
    llm_events = [r.get("llm_events") for r in rows if isinstance(r.get("llm_events"), (int, float))]
    return {
        "records": completed,
        "passed": passed,
        "failed_or_blocked": completed - passed,
        "pass_rate": round(passed / completed, 4) if completed else 0,
        "wall_time_sec_sum": round(sum(wall_times), 3) if wall_times else None,
        "wall_time_sec_avg": round(sum(wall_times) / len(wall_times), 3) if wall_times else None,
        "total_tokens_sum": int(sum(tokens)) if tokens else None,
        "total_tokens_avg": round(sum(tokens) / len(tokens), 1) if tokens else None,
        "llm_events_sum": int(sum(llm_events)) if llm_events else None,
    }


def write_charts(charts_dir, results):
    write_bar_chart(
        charts_dir / "success_rate_by_agent.svg",
        "Pass rate by agent",
        [(agent, aggregate([r for r in results if r["agent"] == agent])["pass_rate"] * 100) for agent in sorted({r["agent"] for r in results})],
        "%",
        max_value=100,
    )
    write_grouped_task_chart(
        charts_dir / "wall_time_by_task.svg",
        "Wall time by task",
        results,
        "wall_time_sec",
        "sec",
    )
    write_grouped_task_chart(
        charts_dir / "tokens_by_task.svg",
        "Tokens by task",
        results,
        "total_tokens",
        "tok",
    )
    write_grouped_task_chart(
        charts_dir / "llm_events_by_task.svg",
        "LLM events by task",
        results,
        "llm_events",
        "events",
    )
    failures = []
    for suite in sorted({r.get("suite") or "unknown" for r in results}):
        rows = [r for r in results if (r.get("suite") or "unknown") == suite]
        failures.append((suite, sum(1 for r in rows if r.get("verdict") != "pass")))
    write_bar_chart(charts_dir / "failures_by_suite.svg", "Failures by suite", failures, "fail")


def write_grouped_task_chart(path, title, results, key, unit):
    agents = sorted({r["agent"] for r in results})
    tasks = sorted({r["task_id"] for r in results})
    values = []
    for task in tasks:
        for agent in agents:
            row = next((r for r in results if r["task_id"] == task and r["agent"] == agent), None)
            value = row.get(key) if row else None
            values.append((f"{task}\n{agent}", value if isinstance(value, (int, float)) else 0))
    write_bar_chart(path, title, values, unit)


def write_bar_chart(path, title, rows, unit, max_value=None):
    width = 1120
    left = 240
    top = 64
    row_h = 30
    gap = 10
    height = max(180, top + len(rows) * (row_h + gap) + 48)
    plot_w = width - left - 80
    observed_max = max([value for _, value in rows] + [1])
    scale_max = max_value or observed_max
    colors = ["#2f80ed", "#27ae60", "#f2994a", "#eb5757", "#9b51e0", "#00a99d"]
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">',
        '<rect width="100%" height="100%" fill="#fbfbfc"/>',
        f'<text x="24" y="34" font-family="Arial, sans-serif" font-size="22" font-weight="700" fill="#111">{esc(title)}</text>',
        f'<line x1="{left}" y1="{top - 16}" x2="{left + plot_w}" y2="{top - 16}" stroke="#d8dbe2"/>',
    ]
    for index, (label, value) in enumerate(rows):
        y = top + index * (row_h + gap)
        bar_w = 0 if scale_max == 0 else int(plot_w * (value / scale_max))
        color = colors[index % len(colors)]
        label_lines = str(label).split("\n")
        parts.append(
            f'<text x="24" y="{y + 19}" font-family="Arial, sans-serif" font-size="13" fill="#222">{esc(label_lines[0])}</text>'
        )
        if len(label_lines) > 1:
            parts.append(
                f'<text x="24" y="{y + 34}" font-family="Arial, sans-serif" font-size="11" fill="#777">{esc(label_lines[1])}</text>'
            )
        parts.append(f'<rect x="{left}" y="{y}" width="{plot_w}" height="{row_h}" fill="#eef1f5"/>')
        parts.append(f'<rect x="{left}" y="{y}" width="{bar_w}" height="{row_h}" fill="{color}"/>')
        value_label = f"{value:.1f} {unit}" if isinstance(value, float) and not value.is_integer() else f"{int(value)} {unit}"
        parts.append(
            f'<text x="{left + min(bar_w + 8, plot_w - 100)}" y="{y + 20}" font-family="Arial, sans-serif" font-size="12" fill="#111">{esc(value_label)}</text>'
        )
    parts.append("</svg>")
    path.write_text("\n".join(parts) + "\n")


def write_summary_md(run_dir, results, summary):
    lines = [
        "# Codex vs Claude Agent Compare",
        "",
        f"Generated: {summary['created_at']}",
    ]
    if summary.get("synthetic"):
        lines.extend(["", "Note: this report uses synthetic selftest records, not real agent benchmark results."])
    lines.extend(
        [
            "",
            "## Overall",
            "",
            "| Agent | Pass rate | Passed | Records | Wall time | Tokens | LLM events |",
            "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
        ]
    )
    for agent, row in summary["agents"].items():
        lines.append(
            f"| {agent} | {row['pass_rate'] * 100:.1f}% | {row['passed']} | {row['records']} | "
            f"{fmt(row['wall_time_sec_sum'])} | {fmt(row['total_tokens_sum'])} | {fmt(row['llm_events_sum'])} |"
        )
    lines.extend(
        [
            "",
            "## Charts",
            "",
            "- [Pass rate by agent](charts/success_rate_by_agent.svg)",
            "- [Wall time by task](charts/wall_time_by_task.svg)",
            "- [Tokens by task](charts/tokens_by_task.svg)",
            "- [LLM events by task](charts/llm_events_by_task.svg)",
            "- [Failures by suite](charts/failures_by_suite.svg)",
            "",
            "## Task Results",
            "",
            "| Task | Suite | Agent | Verdict | Wall time | Tokens | Artifacts |",
            "| --- | --- | --- | --- | ---: | ---: | ---: |",
        ]
    )
    for row in sorted(results, key=lambda r: (r["task_id"], r["agent"])):
        lines.append(
            f"| {row['task_id']} | {row.get('suite', '')} | {row['agent']} | {row.get('verdict', '')} | "
            f"{fmt(row.get('wall_time_sec'))} | {fmt(row.get('total_tokens'))} | {len(row.get('artifact_paths') or [])} |"
        )
    lines.extend(
        [
            "",
            "## Files",
            "",
            "- `results.jsonl`: normalized per-agent task records.",
            "- `summary.json`: aggregate metrics used by charts.",
            "- `raw/`: raw CLI outputs for auditability when real runs are executed.",
        ]
    )
    (run_dir / "summary.md").write_text("\n".join(lines) + "\n")


def write_synthetic_results(run_dir):
    rows = [
        synthetic("codex", "trusted_tabs_runtime", "safety", "pass", 18.4, 1100, 2),
        synthetic("claude", "trusted_tabs_runtime", "safety", "pass", 21.8, 1400, 2),
        synthetic("codex", "formmax_fixture", "formmax", "pass", 44.2, 1800, 3),
        synthetic("claude", "formmax_fixture", "formmax", "blocked", 61.1, 2500, 4),
    ]
    with (run_dir / "results.jsonl").open("w") as fh:
        for row in rows:
            fh.write(json.dumps(row, sort_keys=True) + "\n")
    (run_dir / "README.txt").write_text(
        "Synthetic records for validating agent_compare.py reporting. Do not use as benchmark evidence.\n"
    )


def synthetic(agent, task_id, suite, verdict, wall_time, tokens, llm_events):
    return {
        "schema_version": 1,
        "run_kind": "synthetic",
        "started_at": now_iso(),
        "agent": agent,
        "agent_version": "synthetic",
        "task_id": task_id,
        "suite": suite,
        "risk": "synthetic",
        "verdict": verdict,
        "wall_time_sec": wall_time,
        "input_tokens": tokens // 2,
        "output_tokens": tokens // 2,
        "total_tokens": tokens,
        "llm_events": llm_events,
        "commands_run": [],
        "artifact_paths": [],
        "observations": ["synthetic chart selftest row"],
        "errors": [] if verdict == "pass" else ["synthetic non-pass row"],
        "agent_cli_command": [],
        "raw_output": None,
        "final_output": None,
        "error": None,
    }


def validate_selftest_report(run_dir):
    required = [
        run_dir / "summary.md",
        run_dir / "summary.json",
        run_dir / "charts" / "success_rate_by_agent.svg",
        run_dir / "charts" / "wall_time_by_task.svg",
        run_dir / "charts" / "tokens_by_task.svg",
        run_dir / "charts" / "llm_events_by_task.svg",
        run_dir / "charts" / "failures_by_suite.svg",
    ]
    missing = [str(path) for path in required if not path.exists()]
    if missing:
        raise SystemExit(f"selftest missing artifacts: {missing}")


def try_json(text):
    if not isinstance(text, str) or not text.strip():
        return None
    try:
        return json.loads(text)
    except json.JSONDecodeError:
        return None


def minified_json(value):
    return json.dumps(value, separators=(",", ":"), sort_keys=True)


def truncate(text, max_bytes):
    encoded = text.encode("utf-8", errors="replace")
    if len(encoded) <= max_bytes:
        return text
    return encoded[:max_bytes].decode("utf-8", errors="replace") + "\n... truncated ...\n"


def fmt(value):
    if value is None:
        return ""
    if isinstance(value, float):
        return f"{value:.1f}"
    return str(value)


def esc(value):
    return html.escape(str(value), quote=True)


def unix_ms():
    return int(time.time() * 1000)


def now_iso():
    return dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat()


if __name__ == "__main__":
    main()
