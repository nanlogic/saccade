#!/usr/bin/env python3
"""Exercise the MCP server embedded in an installed Saccade.app."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import pathlib
import shutil
import subprocess
import tempfile
import time
from typing import Any


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--app", type=pathlib.Path, required=True)
    parser.add_argument("--output-dir", type=pathlib.Path, required=True)
    parser.add_argument("--forbidden-path", action="append", default=[])
    parser.add_argument("--timeout-sec", type=float, default=20.0)
    return parser.parse_args()


def sha256(path: pathlib.Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        while chunk := source.read(1024 * 1024):
            digest.update(chunk)
    return digest.hexdigest()


class McpClient:
    def __init__(self, command: pathlib.Path, env: dict[str, str]) -> None:
        self.process = subprocess.Popen(
            [str(command)],
            cwd="/private/tmp",
            env=env,
            text=True,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=1,
        )
        self.next_id = 1
        self.public: list[dict[str, Any]] = []

    def request(self, method: str, params: dict[str, Any]) -> dict[str, Any]:
        assert self.process.stdin is not None and self.process.stdout is not None
        request_id = self.next_id
        self.next_id += 1
        self.process.stdin.write(
            json.dumps(
                {"jsonrpc": "2.0", "id": request_id, "method": method, "params": params}
            )
            + "\n"
        )
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        if not line:
            stderr = self.process.stderr.read() if self.process.stderr else ""
            raise RuntimeError(f"MCP exited during {method}: {stderr[-1200:]}")
        response = json.loads(line)
        self.public.append(response)
        if response.get("error"):
            raise RuntimeError(f"MCP {method} failed: {response['error']}")
        return response.get("result", {})

    def tool(self, name: str, arguments: dict[str, Any]) -> dict[str, Any]:
        result = self.request("tools/call", {"name": name, "arguments": arguments})
        structured = result.get("structuredContent")
        if not isinstance(structured, dict):
            raise RuntimeError(f"MCP tool {name} returned no structured content")
        return structured

    def close(self) -> None:
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=3)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


def main() -> int:
    args = parse_args()
    app = args.app.resolve()
    macos = app / "Contents" / "MacOS"
    resources = app / "Contents" / "Resources" / "Saccade"
    browser = macos / "Saccade"
    server = macos / "saccade-mcp"
    launcher = macos / "saccade-current-tab-mcp"
    fixture = resources / "fixtures" / "mcp_installation" / "index.html"
    dynamic_fixture = (
        resources / "fixtures" / "mcp_installation" / "dynamic-form.html"
    )
    installation = resources / "INSTALLATION.json"
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / "report.json"
    clean_home = pathlib.Path(tempfile.mkdtemp(prefix="saccade-clean-home-"))
    os.chmod(clean_home, 0o700)
    client: McpClient | None = None
    stage = "layout"
    started = time.monotonic()
    report: dict[str, Any]
    try:
        required = [browser, server, launcher, fixture, dynamic_fixture, installation]
        missing = [str(path) for path in required if not path.is_file()]
        if missing:
            raise AssertionError(f"installed app is not self-contained: {missing}")
        metadata = json.loads(installation.read_text(encoding="utf-8"))
        if metadata.get("repo_required") is not False:
            raise AssertionError(f"invalid installation metadata: {metadata}")
        launcher_text = launcher.read_text(encoding="utf-8")
        leaked_paths = [path for path in args.forbidden_path if path and path in launcher_text]
        if leaked_paths:
            raise AssertionError(f"installed launcher contains forbidden paths: {leaked_paths}")

        stage = "signature"
        subprocess.run(
            ["codesign", "--verify", "--strict", "--verbose=2", str(app)],
            check=True,
            capture_output=True,
            text=True,
        )
        subprocess.run(
            ["codesign", "--verify", "--strict", "--verbose=2", str(server)],
            check=True,
            capture_output=True,
            text=True,
        )

        stage = "mcp_start"
        env = {
            "HOME": str(clean_home),
            "PATH": "/usr/bin:/bin:/usr/sbin:/sbin",
            "LANG": "en_US.UTF-8",
            "TMPDIR": tempfile.gettempdir(),
        }
        client = McpClient(launcher, env)
        initialized = client.request("initialize", {})
        tools = client.request("tools/list", {}).get("tools", [])
        names = {tool.get("name") for tool in tools}
        required_tools = {
            "saccade.tabs.open_agent",
            "saccade.tabs.grant_current",
            "saccade.web.truth",
            "saccade.web.article_text",
        }
        if not required_tools.issubset(names):
            raise AssertionError(f"installed MCP is missing tools: {required_tools - names}")
        forbidden_tools = {
            "saccade.dev.open_local",
            "saccade.dev.audit_page",
            "saccade.web.fill_form",
            "saccade.report.validate_run",
        }
        if forbidden_tools & names:
            raise AssertionError(
                f"installed MCP advertised workspace-only tools: {forbidden_tools & names}"
            )
        capabilities = initialized.get("saccade") or {}
        if (
            capabilities.get("runtime_profile") != "installed_product"
            or capabilities.get("developer_tools_available") is not False
        ):
            raise AssertionError(f"installed MCP reported wrong runtime profile: {capabilities}")

        stage = "open_agent"
        opened = client.tool("saccade.tabs.open_agent", {"url": fixture.as_uri()})
        tab = opened.get("tab") or {}
        tab_id = int(tab["tab_id"])
        revision = int(tab["page_revision"])
        if (
            opened.get("ready") is not True
            or opened.get("agent_input_grant") is not True
            or str(tab.get("owner", "")).lower() != "agent"
        ):
            raise AssertionError(f"installed MCP did not attach to the new Agent tab: {opened}")

        stage = "read"
        truth = client.tool("saccade.web.truth", {"tab_id": tab_id})
        article_minimal = client.tool(
            "saccade.web.article_text",
            {"tab_id": tab_id, "basis_page_revision": revision, "max_chars": 5000},
        )
        article_compact = client.tool(
            "saccade.web.article_text",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "max_chars": 5000,
                "mode": "compact",
            },
        )
        article_evidence = client.tool(
            "saccade.web.article_text",
            {
                "tab_id": tab_id,
                "basis_page_revision": revision,
                "max_chars": 5000,
                "mode": "evidence",
            },
        )
        article_minimal_bytes = len(
            json.dumps(article_minimal, separators=(",", ":"))
        )
        article_compact_bytes = len(
            json.dumps(article_compact, separators=(",", ":"))
        )
        article_evidence_bytes = len(
            json.dumps(article_evidence, separators=(",", ":"))
        )
        if (
            truth.get("url") != fixture.as_uri()
            or "without a source repository"
            not in str(article_minimal.get("text") or "")
        ):
            raise AssertionError(
                f"installed read path returned the wrong page: {truth} {article_minimal}"
            )
        if (
            set(article_minimal) - {"text", "page_revision", "untrusted", "truncated"}
            or article_minimal.get("page_revision") != revision
            or article_minimal.get("untrusted") is not True
            or article_minimal_bytes >= article_compact_bytes * 0.7
        ):
            raise AssertionError(
                "minimal article response remained too large or ambiguous: "
                f"minimal={article_minimal_bytes} compact={article_compact_bytes} "
                f"payload={article_minimal}"
            )
        if (
            article_compact.get("response_mode") != "compact"
            or article_compact_bytes >= article_evidence_bytes * 0.7
        ):
            raise AssertionError(
                "compact article response remained too large: "
                f"compact={article_compact_bytes} evidence={article_evidence_bytes}"
            )
        public_blob = json.dumps(client.public, sort_keys=True)
        leaked_public = [path for path in args.forbidden_path if path and path in public_blob]
        if leaked_public:
            raise AssertionError(f"public MCP output exposed forbidden paths: {leaked_public}")

        stage = "close"
        client.tool("saccade.tabs.close", {"tab_id": tab_id})

        stage = "dynamic_form_open"
        dynamic_opened = client.tool(
            "saccade.tabs.open_agent", {"url": dynamic_fixture.as_uri()}
        )
        if dynamic_opened.get("ready") is not True:
            raise AssertionError(
                f"dynamic Agent tab was not ready after open: {dynamic_opened}"
            )
        dynamic_tab_id = int((dynamic_opened.get("tab") or {})["tab_id"])

        stage = "dynamic_form_inventory"
        compact = client.tool(
            "saccade.web.form_inventory",
            {"tab_id": dynamic_tab_id, "mode": "compact"},
        )
        minimal = client.tool(
            "saccade.web.form_inventory",
            {"tab_id": dynamic_tab_id},
        )
        full = client.tool(
            "saccade.web.form_inventory",
            {
                "tab_id": dynamic_tab_id,
                "mode": "full",
                "wait_for_fields_ms": 1000,
            },
        )
        minimal_bytes = len(json.dumps(minimal, separators=(",", ":")))
        compact_bytes = len(json.dumps(compact, separators=(",", ":")))
        full_bytes = len(json.dumps(full, separators=(",", ":")))
        if (
            minimal.get("field_count") != 6
            or minimal.get("sensitive_count") != 1
            or minimal.get("ready") is not True
            or any(
                set(field)
                - {"field_id", "label", "type", "status", "required", "protected"}
                for field in minimal.get("fields", [])
            )
        ):
            raise AssertionError(f"minimal form inventory was invalid: {minimal}")
        if minimal_bytes >= compact_bytes * 0.75:
            raise AssertionError(
                f"minimal inventory remained too large: minimal={minimal_bytes} compact={compact_bytes}"
            )
        if (
            compact.get("field_count") != 6
            or compact.get("sensitive_count") != 1
            or compact.get("field_inventory_stable") is not True
            or compact.get("waited_for_fields_ms", 0) < 800
        ):
            raise AssertionError(f"dynamic form readiness failed: {compact}")
        if compact_bytes >= full_bytes * 0.72:
            raise AssertionError(
                f"compact inventory remained too large: compact={compact_bytes} full={full_bytes}"
            )
        if any(
            "selector_hash" in field or "blocked_reasons" in field
            for field in compact.get("fields", [])
        ):
            raise AssertionError("compact inventory exposed verbose field diagnostics")
        client.tool("saccade.tabs.close", {"tab_id": dynamic_tab_id})
        report = {
            "schema": "saccade-installed-mcp-cleanroom-v1",
            "verdict": "PASS",
            "app": str(app),
            "release_stamp": metadata.get("release_stamp"),
            "configured_command": metadata.get("mcp_command"),
            "repo_required": False,
            "external_runtime_required": False,
            "clean_home": True,
            "cwd_outside_repo": True,
            "mcp_tools": len(tools),
            "workspace_only_tools_hidden": True,
            "agent_tab_opened": True,
            "same_webview_attached": True,
            "collector_ready": True,
            "article_read": True,
            "article_minimal_bytes": article_minimal_bytes,
            "article_compact_bytes": article_compact_bytes,
            "article_evidence_bytes": article_evidence_bytes,
            "article_compact_ratio": round(
                article_compact_bytes / article_evidence_bytes, 3
            ),
            "dynamic_form_ready": True,
            "dynamic_form_waited_ms": compact.get("waited_for_fields_ms"),
            "minimal_inventory_bytes": minimal_bytes,
            "compact_inventory_bytes": compact_bytes,
            "full_inventory_bytes": full_bytes,
            "compact_reduction_ratio": round(compact_bytes / full_bytes, 3),
            "browser_sha256": sha256(browser),
            "mcp_sha256": sha256(server),
            "values_logged": False,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    except Exception as error:
        report = {
            "schema": "saccade-installed-mcp-cleanroom-v1",
            "verdict": "FAIL",
            "stage": stage,
            "error": str(error),
            "values_logged": False,
            "duration_sec": round(time.monotonic() - started, 3),
        }
    finally:
        if client is not None:
            client.close()
        shutil.rmtree(clean_home, ignore_errors=True)

    report_path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    print(f"INSTALLED_MCP_CLEANROOM verdict={report['verdict']} report={report_path}")
    return 0 if report["verdict"] == "PASS" else 1


if __name__ == "__main__":
    raise SystemExit(main())
