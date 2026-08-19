#!/usr/bin/env python3
"""Run the official Playwright MCP locator baseline on real MouseAccuracy."""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from typing import Any

sys.path.insert(0, str(Path(__file__).resolve().parent))
from benchmark_playwright_parity import Mcp, result_text  # noqa: E402


def decoded_result(text: str) -> dict[str, Any]:
    start = text.find("{")
    if start < 0:
        raise RuntimeError(f"Playwright result omitted JSON: {text[-500:]}")
    value, _ = json.JSONDecoder().raw_decode(text[start:])
    if not isinstance(value, dict):
        raise RuntimeError("Playwright result was not an object")
    return value


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--version", default="0.0.79")
    parser.add_argument("--duration-ms", type=int, default=30_000)
    args = parser.parse_args()
    if not 1_000 <= args.duration_ms <= 60_000:
        raise SystemExit("--duration-ms must be between 1000 and 60000")

    command = [
        "npx", "-y", f"@playwright/mcp@{args.version}",
        "--headless", "--browser", "chrome", "--isolated",
        "--snapshot-mode", "none", "--image-responses", "omit",
        "--timeout-action", "1000", "--timeout-settle", "0",
    ]
    client = Mcp(command, os.environ.copy())
    report: dict[str, Any]
    phase = "initialize"
    try:
        initialized = client.initialize()
        phase = "tools_list"
        tools = {tool["name"] for tool in client.tools()}
        if "browser_run_code_unsafe" not in tools:
            raise RuntimeError("official Playwright MCP omitted browser_run_code_unsafe")
        settings_code = """async (page) => {
          await page.goto('https://mouseaccuracy.com/');
          for (const value of ['Normal', 'Hard']) {
            await page.getByText(value, {exact: true}).locator('..').getByRole('button').nth(1).click();
          }
          for (const value of ['Medium', 'Small']) {
            await page.getByText(value, {exact: true}).locator('..').getByRole('button').nth(0).click();
          }
          const text = await page.locator('body').innerText();
          return {difficulty: text.includes('Insane') ? 'Insane' : null,
            target_size: text.includes('Tiny') ? 'Tiny' : null,
            verified_highest: text.includes('Insane') && text.includes('Tiny')};
        }"""
        phase = "settings"
        settings_response, settings_ms = client.tool(
            "browser_run_code_unsafe", {"code": settings_code}, timeout=30.0
        )
        settings = decoded_result(result_text(settings_response))
        if settings.get("verified_highest") is not True:
            raise RuntimeError(f"Playwright did not reach Insane + Tiny: {settings}")

        loop_code = f"""async (page) => {{
          await page.getByText('START', {{exact: true}}).click();
          const currentTarget = page.locator('.target:not(.hit):visible').last();
          await currentTarget.waitFor({{state: 'visible', timeout: 8000}});
          const score = async () => {{
            const text = await page.locator('body').innerText();
            const match = text.match(/SCORE\\s+(\\d+)/i);
            return match ? Number(match[1]) : 0;
          }};
          const before_score = await score();
          const started = Date.now();
          let actions = 0;
          let locator_failures = 0;
          while (Date.now() - started < {args.duration_ms}) {{
            const remaining = {args.duration_ms} - (Date.now() - started);
            try {{
              await page.locator('.target:not(.hit):visible').last().click({{timeout: Math.min(1000, Math.max(50, remaining))}});
              actions += 1;
            }} catch (error) {{
              locator_failures += 1;
              if (remaining <= 1000) break;
              await page.waitForTimeout(1);
            }}
          }}
          const after_score = await score();
          return {{actions, locator_failures, before_score, after_score,
            duration_ms: Date.now() - started, semantic_advances: after_score - before_score}};
        }}"""
        phase = "locator_loop"
        loop_response, loop_tool_ms = client.tool(
            "browser_run_code_unsafe",
            {"code": loop_code},
            timeout=args.duration_ms / 1000 + 90.0,
        )
        loop = decoded_result(result_text(loop_response))
        report = {
            "schema": "saccade-playwright-reflex-baseline/1",
            "passed": loop.get("actions", 0) > 0
            and loop.get("semantic_advances", 0) > 0,
            "route": "official_playwright_mcp_browser_run_code_unsafe_locator",
            "playwright_mcp_version": args.version,
            "server": initialized.get("serverInfo"),
            "settings": settings,
            "settings_tool_ms": settings_ms,
            "loop_tool_ms": loop_tool_ms,
            "loop": loop,
            "limitations": [
                "DOM locator automation baseline; not physical mouse accuracy",
                "test-only site selector .target; not Saccade product logic",
            ],
        }
    except Exception as error:  # noqa: BLE001
        report = {
            "schema": "saccade-playwright-reflex-baseline/1",
            "passed": False,
            "failure_phase": phase,
            "error": str(error),
        }
    finally:
        client.close()

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"passed": report["passed"], "output": str(args.output)}))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(main())
