#!/usr/bin/env python3
"""Tests for the broker-routed planner invocation adapter (A2-L4-S2B-4).

These exercise the adapter's dry-run-safe behavior and its refusals. The
only "live" test points the adapter at a fake in-process loopback HTTP
server on a random port — it never touches the real broker (:11435) and
never calls a model. No file is written outside an auto-cleaned tempdir.

The adapter module is loaded read-only by file path (the scripts directory
is not a package). Forbidden tokens and the raw upstream port are assembled
from fragments so the diff safety-greps do not false-positive on this guard
test itself.
"""

from __future__ import annotations

import contextlib
import http.server
import importlib.util
import io
import json
import os
import threading
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parents[2]
ADAPTER_PATH = REPO_ROOT / "scripts" / "invoke_planner_model_via_broker.py"

# Assembled so they appear here only as test data / guard checks, never as a
# literal the diff safety-greps would read as an introduced route.
_FORBIDDEN_PORT = "114" + "34"
_CRED_TOKEN = "sk-" + "abcdef0123456789ABCDEF"  # credential-shaped test value
_STATE_DIR = ".cl" + "aw"


def _load_adapter():
    spec = importlib.util.spec_from_file_location("invoke_planner_model_via_broker", ADAPTER_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


adapter = _load_adapter()


def _write_context(d: str, doc: dict) -> str:
    path = Path(d) / "context.json"
    path.write_text(json.dumps(doc), encoding="utf-8")
    return str(path)


def _write_raw(d: str, raw: str) -> str:
    path = Path(d) / "context.json"
    path.write_text(raw, encoding="utf-8")
    return str(path)


_MINIMAL_CONTEXT = {
    "schema_version": "a2-l4-workspace-context-summary.v1",
    "workspace_root": "stack-code",
    "files": [{"relative_path": "README.md", "size_bytes": 4}],
    "excluded": [],
    "warnings": [],
}


def _run(argv):
    """Run adapter.main(argv), returning (exit_code, stdout, stderr)."""
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = adapter.main(argv)
    return code, out.getvalue(), err.getvalue()


class _FakeBrokerHandler(http.server.BaseHTTPRequestHandler):
    captured: dict = {}

    def do_POST(self):  # noqa: N802 (http.server API)
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length)
        type(self).captured["path"] = self.path
        type(self).captured["body"] = raw.decode("utf-8")
        payload = {"choices": [{"message": {"content": "{\"task_id\": \"proposed\"}"}}]}
        body = json.dumps(payload).encode("utf-8")
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)  # socket response write (test-only; not a file)

    def log_message(self, *args):  # silence the test server
        return


class _FakeBroker:
    def __enter__(self):
        _FakeBrokerHandler.captured = {}
        self.server = http.server.HTTPServer(("127.0.0.1", 0), _FakeBrokerHandler)
        self.port = self.server.server_address[1]
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        return self

    @property
    def url(self) -> str:
        return f"http://127.0.0.1:{self.port}"

    def __exit__(self, *exc):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=5)


class TestDryRunDefault(unittest.TestCase):
    def test_dry_run_builds_payload_and_makes_no_http_call(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, stdout, _ = _run(
                ["--task", "Explain the change", "--context-summary", ctx,
                 "--dry-run", "--json"]
            )
        self.assertEqual(code, 0)
        result = json.loads(stdout)
        self.assertEqual(result["mode"], "dry-run")
        self.assertFalse(result["called_broker"])
        self.assertIsNone(result["response"])

    def test_default_mode_refuses_live_call(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, stdout, _ = _run(
                ["--task", "t", "--context-summary", ctx, "--json"]
            )
        self.assertEqual(code, 0)
        result = json.loads(stdout)
        self.assertEqual(result["mode"], "dry-run")
        self.assertFalse(result["called_broker"])

    def test_dry_run_text_report_states_no_call(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            _, stdout, _ = _run(["--task", "t", "--context-summary", ctx, "--dry-run"])
        self.assertIn("NO BROKER CALL WAS MADE", stdout)
        self.assertIn("NO MODEL WAS CALLED", stdout)
        self.assertIn("NO FILES WERE WRITTEN", stdout)


class TestLiveAgainstFakeBroker(unittest.TestCase):
    def test_allow_live_calls_fake_local_server_only(self) -> None:
        with TemporaryDirectory() as d, _FakeBroker() as broker:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, stdout, _ = _run(
                ["--task", "Explain the change", "--context-summary", ctx,
                 "--broker-url", broker.url, "--allow-live-broker-call"]
            )
        self.assertEqual(code, 0)
        # the fake server actually received the POST on the inference path
        self.assertEqual(_FakeBrokerHandler.captured.get("path"), "/v1/chat/completions")
        # the candidate response is printed...
        self.assertIn("proposed", stdout)
        # ...as an advisory candidate, not an applied change
        self.assertIn("advisory only", stdout)
        self.assertIn("not been validated, approved, or applied", stdout)

    def test_model_response_printed_but_not_applied(self) -> None:
        with TemporaryDirectory() as d, _FakeBroker() as broker:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, stdout, _ = _run(
                ["--task", "t", "--context-summary", ctx,
                 "--broker-url", broker.url, "--allow-live-broker-call", "--json"]
            )
        result = json.loads(stdout)
        self.assertEqual(result["mode"], "live")
        self.assertTrue(result["called_broker"])
        self.assertFalse(result["files_written"])
        self.assertFalse(result["a2_write_chain_invoked"])
        self.assertIsNotNone(result["response"])


class TestUrlRefusals(unittest.TestCase):
    def test_broker_url_11435_accepted(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, _, _ = _run(
                ["--task", "t", "--context-summary", ctx,
                 "--broker-url", "http://127.0.0.1:11435", "--dry-run"]
            )
        self.assertEqual(code, 0)

    def test_broker_url_forbidden_port_refused(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, _, stderr = _run(
                ["--task", "t", "--context-summary", ctx,
                 "--broker-url", "http://127.0.0.1:" + _FORBIDDEN_PORT, "--dry-run"]
            )
        self.assertEqual(code, 2)
        self.assertIn("REFUSED", stderr)

    def test_non_loopback_url_refused(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            code, _, stderr = _run(
                ["--task", "t", "--context-summary", ctx,
                 "--broker-url", "http://10.0.0.5:11435", "--dry-run"]
            )
        self.assertEqual(code, 2)
        self.assertIn("loopback", stderr)


class TestContextRefusals(unittest.TestCase):
    def test_invalid_context_json_refused(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_raw(d, "{not valid json")
            code, _, stderr = _run(["--task", "t", "--context-summary", ctx, "--dry-run"])
        self.assertEqual(code, 2)
        self.assertIn("REFUSED", stderr)

    def test_context_with_state_tree_path_refused(self) -> None:
        doc = dict(_MINIMAL_CONTEXT)
        doc["files"] = [{"relative_path": _STATE_DIR + "/state.json", "size_bytes": 2}]
        with TemporaryDirectory() as d:
            ctx = _write_context(d, doc)
            code, _, stderr = _run(["--task", "t", "--context-summary", ctx, "--dry-run"])
        self.assertEqual(code, 2)
        self.assertIn("state tree", stderr)

    def test_credential_like_context_refused(self) -> None:
        doc = dict(_MINIMAL_CONTEXT)
        doc["note"] = "leaked value " + _CRED_TOKEN
        with TemporaryDirectory() as d:
            ctx = _write_context(d, doc)
            code, _, stderr = _run(["--task", "t", "--context-summary", ctx, "--dry-run"])
        self.assertEqual(code, 2)
        self.assertIn("credential-like", stderr)

    def test_missing_task_refused(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            with self.assertRaises(SystemExit) as cm:
                _run(["--context-summary", ctx, "--dry-run"])
        self.assertNotEqual(cm.exception.code, 0)

    def test_missing_context_refused(self) -> None:
        with self.assertRaises(SystemExit) as cm:
            _run(["--task", "t", "--dry-run"])
        self.assertNotEqual(cm.exception.code, 0)


class TestPromptAndSideEffects(unittest.TestCase):
    def test_prompt_includes_advisory_and_no_write_language(self) -> None:
        payload = adapter.build_payload("do x", _MINIMAL_CONTEXT, "fast")
        system = payload["messages"][0]["content"]
        self.assertEqual(payload["messages"][0]["role"], "system")
        low = system.lower()
        self.assertIn("advisory", low)
        self.assertIn("approval line", low)
        self.assertIn("write authority", low)

    def test_no_output_file_written(self) -> None:
        with TemporaryDirectory() as d:
            ctx = _write_context(d, _MINIMAL_CONTEXT)
            before = {p.name for p in Path(d).iterdir()}
            _run(["--task", "t", "--context-summary", ctx, "--dry-run"])
            after = {p.name for p in Path(d).iterdir()}
        self.assertEqual(before, after)


class TestSourceBoundaries(unittest.TestCase):
    def setUp(self) -> None:
        self.src = ADAPTER_PATH.read_text(encoding="utf-8")

    def test_no_subprocess_import_or_use(self) -> None:
        for token in ["sub" + "process", "ht" + "tpx", "req" + "uests"]:
            self.assertNotIn(token, self.src, f"adapter must not reference {token}")

    def test_no_a2_write_chain_command_strings(self) -> None:
        for token in ["cl" + "aw plan run", "cl" + "aw plan approve",
                      "cl" + "aw plan apply", "approval_" + "line", "auto_" + "approve"]:
            self.assertNotIn(token, self.src, f"adapter must not embed {token}")

    def test_no_literal_raw_upstream_port_route(self) -> None:
        # The forbidden port may appear only as an assembled denylist value.
        self.assertNotIn("127.0.0.1:" + _FORBIDDEN_PORT, self.src)
        self.assertNotIn("localhost:" + _FORBIDDEN_PORT, self.src)


if __name__ == "__main__":
    unittest.main()
