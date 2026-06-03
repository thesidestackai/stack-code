#!/usr/bin/env python3
"""Tests for the offline planner CLI skeleton (A2-L4-S2B-3).

These exercise the offline wiring only: a fixture plan, a tempdir
workspace, the existing read-only validator and pretty-printer. No model
is called, no service is contacted, and nothing is written outside an
auto-cleaned temporary directory.

The CLI module is loaded read-only by file path (the scripts directory is
not a package), the same convention the sibling tests use. Forbidden
runtime/model/network tokens and the raw upstream port are assembled from
fragments so the diff safety-greps do not false-positive on this guard
test itself.
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parents[2]
CLI_PATH = REPO_ROOT / "scripts" / "run_offline_planner_cli.py"
FIXTURE_DIR = REPO_ROOT / "schemas" / "a2-l4" / "fixtures" / "planner-output"
VALID_FIXTURE = FIXTURE_DIR / "valid-minimal.json"
INVALID_FIXTURE = FIXTURE_DIR / "invalid-missing-required.json"


def _load_cli():
    spec = importlib.util.spec_from_file_location("run_offline_planner_cli", CLI_PATH)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


cli = _load_cli()


def _make_workspace(d: str) -> str:
    """Materialise a tiny tempdir workspace (auto-cleaned by the caller)."""
    root = Path(d)
    (root / "README.md").write_text("# hi", encoding="utf-8")
    (root / "main.py").write_text("print(1)", encoding="utf-8")
    return str(root)


def _list_tree(d: str) -> set[str]:
    found: set[str] = set()
    for current, _dirs, files in os.walk(d):
        for f in files:
            found.add(os.path.relpath(os.path.join(current, f), d))
    return found


def _run(argv):
    """Run cli.main(argv), returning (exit_code, stdout, stderr)."""
    out, err = io.StringIO(), io.StringIO()
    with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
        code = cli.main(argv)
    return code, out.getvalue(), err.getvalue()


class TestValidReport(unittest.TestCase):
    def test_valid_fixture_produces_offline_report(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            code, stdout, _ = _run(
                ["--task", "Explain the change", "--workspace", ws,
                 "--fixture", str(VALID_FIXTURE)]
            )
        self.assertEqual(code, 0)
        self.assertIn("OFFLINE STUB MODE", stdout)
        self.assertIn("Explain the change", stdout)
        self.assertIn("Workspace context summary", stdout)
        self.assertIn("VALID (inert)", stdout)
        # the pretty-printed fixture body is present
        self.assertIn("Document the read-only planner output contract", stdout)

    def test_report_states_no_model_call(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            _, stdout, _ = _run(
                ["--task", "t", "--workspace", ws, "--fixture", str(VALID_FIXTURE)]
            )
        self.assertIn("NO MODEL CALL WAS MADE", stdout)

    def test_report_states_no_files_written(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            _, stdout, _ = _run(
                ["--task", "t", "--workspace", ws, "--fixture", str(VALID_FIXTURE)]
            )
        self.assertIn("NO FILES WERE WRITTEN", stdout)
        self.assertIn("NO A2 WRITE-CHAIN COMMANDS WERE RUN", stdout)

    def test_json_mode_is_inert_data(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            code, stdout, _ = _run(
                ["--task", "t", "--workspace", ws,
                 "--fixture", str(VALID_FIXTURE), "--json"]
            )
        self.assertEqual(code, 0)
        payload = json.loads(stdout)
        self.assertEqual(payload["mode"], "offline-stub")
        self.assertFalse(payload["model_called"])
        self.assertFalse(payload["files_written"])
        self.assertFalse(payload["a2_write_chain_invoked"])


class TestRefusals(unittest.TestCase):
    def test_invalid_fixture_is_refused(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            code, stdout, stderr = _run(
                ["--task", "t", "--workspace", ws,
                 "--fixture", str(INVALID_FIXTURE)]
            )
        self.assertNotEqual(code, 0)
        self.assertIn("REFUSED", stderr)
        self.assertNotIn("VALID (inert)", stdout)

    def test_missing_task_refused(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            with self.assertRaises(SystemExit) as ctx:
                _run(["--workspace", ws, "--fixture", str(VALID_FIXTURE)])
        self.assertNotEqual(ctx.exception.code, 0)

    def test_missing_workspace_refused(self) -> None:
        with self.assertRaises(SystemExit) as ctx:
            _run(["--task", "t", "--fixture", str(VALID_FIXTURE)])
        self.assertNotEqual(ctx.exception.code, 0)

    def test_missing_fixture_refused(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            with self.assertRaises(SystemExit) as ctx:
                _run(["--task", "t", "--workspace", ws])
        self.assertNotEqual(ctx.exception.code, 0)

    def test_nonexistent_fixture_refused(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            code, _, stderr = _run(
                ["--task", "t", "--workspace", ws,
                 "--fixture", str(Path(d) / "no-such-fixture.json")]
            )
        self.assertNotEqual(code, 0)
        self.assertIn("REFUSED", stderr)

    def test_state_tree_workspace_refused(self) -> None:
        # ".claw" as (a component of) the workspace must be refused before any walk.
        code, _, stderr = _run(
            ["--task", "t", "--workspace", ".claw",
             "--fixture", str(VALID_FIXTURE)]
        )
        self.assertNotEqual(code, 0)
        self.assertIn("state tree", stderr)

    def test_state_tree_hint_refused(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            code, _, stderr = _run(
                ["--task", "t", "--workspace", ws,
                 "--fixture", str(VALID_FIXTURE), "--path-hint", ".claw/state.json"]
            )
        self.assertNotEqual(code, 0)
        self.assertIn("state tree", stderr)


class TestNoSideEffects(unittest.TestCase):
    def test_no_output_files_written(self) -> None:
        with TemporaryDirectory() as d:
            ws = _make_workspace(d)
            before = _list_tree(d)
            _run(["--task", "t", "--workspace", ws, "--fixture", str(VALID_FIXTURE)])
            after = _list_tree(d)
        self.assertEqual(before, after, "the CLI must not create or remove any file")


class TestSourceBoundaries(unittest.TestCase):
    """Static guards: the CLI source must carry no runtime/model/approval path."""

    def setUp(self) -> None:
        self.src = CLI_PATH.read_text(encoding="utf-8")

    def test_no_subprocess_or_network_import(self) -> None:
        forbidden = [
            "sub" + "process",
            "soc" + "ket",
            "re" + "quests",
            "ht" + "tpx",
            "urllib." + "request",
        ]
        for token in forbidden:
            self.assertNotIn(token, self.src, f"CLI must not reference {token}")

    def test_no_raw_upstream_port_reference(self) -> None:
        self.assertNotIn("114" + "34", self.src, "CLI must not reference the raw upstream port")

    def test_no_runtime_or_service_tokens(self) -> None:
        forbidden = [
            "oll" + "ama",
            "bro" + "ker",
            "system" + "ctl",
            "dock" + "er",
            "Comfy" + "UI",
            "SG" + "Lang",
        ]
        for token in forbidden:
            self.assertNotIn(token, self.src, f"CLI must not reference {token}")

    def test_no_approval_or_apply_commands(self) -> None:
        forbidden = [
            "approval_" + "line",
            "auto_" + "approve",
            "autonomous_" + "apply",
            "cl" + "aw plan run",
            "cl" + "aw plan approve",
            "cl" + "aw plan apply",
        ]
        for token in forbidden:
            self.assertNotIn(token, self.src, f"CLI must not reference {token}")


if __name__ == "__main__":
    unittest.main()
