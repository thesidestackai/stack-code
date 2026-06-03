"""Tests for the read-only A2-L4 planner-output pretty-printer (S2A-7).

Exercises rendering of the S2A-3 fixture pack and the boundaries pinned
by the S2A-6 scope card: read-only, no-write, no-command, no-model,
no-approval; refusal rendering (reject-never-coerce, no copy-runnable
forbidden payloads); secret handling (field path only, never the value).

Stdlib unittest only (matching tests/test_porting_workspace.py); no new
dependency. Run with:
  python3 -m unittest tests.a2_l4.test_pretty_print_planner_output
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
PRINTER_PATH = REPO_ROOT / "scripts" / "pretty_print_planner_output.py"
FIXTURE_DIR = REPO_ROOT / "schemas" / "a2-l4" / "fixtures" / "planner-output"


def _valid_doc() -> dict:
    return {
        "schema_version": "a2-l4-planner-output.v1",
        "task_id": "t1",
        "workspace_root": "stack-code",
        "task_summary": "A descriptive, inert summary.",
        "plan_steps": [{"step_id": "s1", "description": "Do an inert thing."}],
        "risk_notes": [],
        "operator_next_steps": [],
    }


def _run(arg, cwd=None):
    return subprocess.run(
        [sys.executable, str(PRINTER_PATH), str(arg)],
        capture_output=True,
        text=True,
        cwd=cwd,
    )


class RenderValidTests(unittest.TestCase):
    def test_valid_minimal_renders_and_exits_zero(self) -> None:
        result = _run(FIXTURE_DIR / "valid-minimal.json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("VALID", result.stdout)

    def test_valid_full_renders_and_exits_zero(self) -> None:
        result = _run(FIXTURE_DIR / "valid-full.json")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("VALID", result.stdout)
        # advisory / command-like fields are marked, never executed
        self.assertIn("not executed", result.stdout.lower())


class RenderRefusalTests(unittest.TestCase):
    def test_invalid_fixture_returns_failure(self) -> None:
        result = _run(FIXTURE_DIR / "invalid-missing-required.json")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("REFUSED", result.stdout)

    def test_approval_line_displays_refusal_not_execution(self) -> None:
        result = _run(FIXTURE_DIR / "invalid-approval-line.json")
        self.assertEqual(result.returncode, 1)
        self.assertIn("REFUSED", result.stdout)
        self.assertIn("approval_line", result.stdout)
        # must not echo the would-be approval payload as a runnable line
        self.assertNotIn("APPROVED placeholder", result.stdout)

    def test_raw_11434_endpoint_displays_refusal(self) -> None:
        result = _run(FIXTURE_DIR / "invalid-raw-11434-endpoint.json")
        self.assertEqual(result.returncode, 1)
        self.assertIn("REFUSED", result.stdout)
        self.assertIn("raw_11434_endpoint", result.stdout)

    def test_secret_in_permitted_field_shows_path_not_value(self) -> None:
        doc = _valid_doc()
        secret = "sk-livesecretvalue1234567890abcdef"
        doc["risk_notes"] = [f"token={secret}"]
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(doc, fh)
            tmp = Path(fh.name)
        try:
            result = _run(tmp)
            self.assertEqual(result.returncode, 1)
            self.assertIn("REFUSED", result.stdout)
            self.assertIn("risk_notes", result.stdout)
            self.assertNotIn(secret, result.stdout)
            self.assertNotIn(secret, result.stderr)
        finally:
            tmp.unlink()


class ReadOnlyBoundaryTests(unittest.TestCase):
    def test_no_output_file_written(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            fixture = FIXTURE_DIR / "valid-full.json"
            before = set(os.listdir(d))
            _run(fixture, cwd=d)
            after = set(os.listdir(d))
            self.assertEqual(before, after, "pretty-printer must not write any file")

    def test_no_claw_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as d:
            _run(FIXTURE_DIR / "valid-full.json", cwd=d)
            self.assertFalse(
                (Path(d) / ".claw").exists(), "pretty-printer must not create .claw"
            )

    def test_input_left_unchanged(self) -> None:
        doc = _valid_doc()
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            json.dump(doc, fh)
            tmp = Path(fh.name)
        try:
            before = tmp.read_bytes()
            _run(tmp)
            self.assertEqual(tmp.read_bytes(), before)
        finally:
            tmp.unlink()


class CliUsageTests(unittest.TestCase):
    def test_missing_file_exits_2(self) -> None:
        result = _run(FIXTURE_DIR / "does-not-exist.json")
        self.assertEqual(result.returncode, 2)

    def test_malformed_json_exits_2(self) -> None:
        with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as fh:
            fh.write("{ not valid json ")
            tmp = Path(fh.name)
        try:
            result = _run(tmp)
            self.assertEqual(result.returncode, 2)
        finally:
            tmp.unlink()


if __name__ == "__main__":
    unittest.main()
