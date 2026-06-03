"""Tests for the read-only A2-L4 planner-output schema validator (S2A-5).

Exercises the S2A-3 fixture pack plus the semantic checks the JSON Schema
alone cannot express (path escape, :11434 value, secret pattern). The
validator is a read-only operator helper: it reads the schema and a
candidate document and reports a pass/fail verdict, never writing,
executing, or loading a model.

Stdlib unittest only (matching tests/test_porting_workspace.py); no new
dependency. Run with:  python3 -m unittest tests.a2_l4.test_validate_planner_output_schema
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]
VALIDATOR_PATH = REPO_ROOT / "scripts" / "validate_planner_output_schema.py"
SCHEMA_PATH = REPO_ROOT / "schemas" / "a2-l4" / "planner-output.schema.json"
FIXTURE_DIR = REPO_ROOT / "schemas" / "a2-l4" / "fixtures" / "planner-output"


def _load_validator():
    spec = importlib.util.spec_from_file_location(
        "validate_planner_output_schema", VALIDATOR_PATH
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


validator = _load_validator()


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


class FixturePackTests(unittest.TestCase):
    """Every S2A-3 fixture validates to the expected accept/refuse outcome."""

    def test_all_fixtures_present(self) -> None:
        names = {p.name for p in FIXTURE_DIR.glob("*.json")}
        self.assertIn("valid-minimal.json", names)
        self.assertIn("valid-full.json", names)
        self.assertEqual(len([n for n in names if n.startswith("invalid-")]), 9)

    def test_valid_fixtures_accept(self) -> None:
        schema = validator.load_schema(SCHEMA_PATH)
        for path in sorted(FIXTURE_DIR.glob("valid-*.json")):
            with self.subTest(fixture=path.name):
                doc = json.loads(path.read_text())
                failures = validator.validate_document(doc, schema)
                self.assertEqual(failures, [], f"{path.name} should be accepted")

    def test_invalid_fixtures_refuse(self) -> None:
        schema = validator.load_schema(SCHEMA_PATH)
        for path in sorted(FIXTURE_DIR.glob("invalid-*.json")):
            with self.subTest(fixture=path.name):
                doc = json.loads(path.read_text())
                failures = validator.validate_document(doc, schema)
                self.assertTrue(failures, f"{path.name} should be refused")


class SchemaConformanceTests(unittest.TestCase):
    def setUp(self) -> None:
        self.schema = validator.load_schema(SCHEMA_PATH)

    def test_wrong_schema_version_refused(self) -> None:
        doc = _valid_doc()
        doc["schema_version"] = "a2-l4-planner-output.v2"
        self.assertTrue(validator.validate_document(doc, self.schema))

    def test_missing_required_refused(self) -> None:
        doc = _valid_doc()
        del doc["task_summary"]
        failures = validator.validate_document(doc, self.schema)
        self.assertTrue(any("task_summary" in f for f in failures))

    def test_unknown_top_level_field_refused(self) -> None:
        doc = _valid_doc()
        doc["surprise"] = "x"
        failures = validator.validate_document(doc, self.schema)
        self.assertTrue(any("surprise" in f for f in failures))

    def test_forbidden_field_refused(self) -> None:
        doc = _valid_doc()
        doc["approval_line"] = "apply s1 deadbeef"
        self.assertTrue(validator.validate_document(doc, self.schema))

    def test_nested_closed_object_refused(self) -> None:
        doc = _valid_doc()
        doc["preview_request"] = {"requested": True, "command": "claw plan run"}
        failures = validator.validate_document(doc, self.schema)
        self.assertTrue(any("command" in f for f in failures))

    def test_empty_plan_steps_refused(self) -> None:
        doc = _valid_doc()
        doc["plan_steps"] = []
        self.assertTrue(validator.validate_document(doc, self.schema))

    def test_plan_step_missing_description_refused(self) -> None:
        doc = _valid_doc()
        doc["plan_steps"] = [{"step_id": "s1"}]
        self.assertTrue(validator.validate_document(doc, self.schema))

    def test_wrong_type_refused(self) -> None:
        doc = _valid_doc()
        doc["task_summary"] = 123
        self.assertTrue(validator.validate_document(doc, self.schema))


class SemanticCheckTests(unittest.TestCase):
    """Checks the schema alone cannot express (S2A-4 §12)."""

    def setUp(self) -> None:
        self.schema = validator.load_schema(SCHEMA_PATH)

    def test_path_escape_in_candidate_files_refused(self) -> None:
        doc = _valid_doc()
        doc["candidate_files"] = ["../../etc/passwd"]
        failures = validator.validate_document(doc, self.schema)
        self.assertTrue(any("candidate_files" in f for f in failures))

    def test_absolute_path_in_candidate_files_refused(self) -> None:
        doc = _valid_doc()
        doc["candidate_files"] = ["/etc/passwd"]
        self.assertTrue(validator.validate_document(doc, self.schema))

    def test_path_escape_in_workspace_root_refused(self) -> None:
        doc = _valid_doc()
        doc["workspace_root"] = "../escape"
        self.assertTrue(validator.validate_document(doc, self.schema))

    def test_11434_value_in_permitted_field_refused(self) -> None:
        doc = _valid_doc()
        doc["task_summary"] = "call http://localhost:11434/api/generate directly"
        failures = validator.validate_document(doc, self.schema)
        self.assertTrue(any("11434" in f for f in failures))

    def test_secret_pattern_in_permitted_field_refused(self) -> None:
        doc = _valid_doc()
        doc["risk_notes"] = ["api_key=sk-livesecretvalue1234567890abcdef"]
        failures = validator.validate_document(doc, self.schema)
        self.assertTrue(failures)

    def test_secret_verdict_reports_field_path_not_value(self) -> None:
        doc = _valid_doc()
        secret = "sk-livesecretvalue1234567890abcdef"
        doc["risk_notes"] = [f"token={secret}"]
        failures = validator.validate_document(doc, self.schema)
        joined = " ".join(failures)
        self.assertIn("risk_notes", joined)
        self.assertNotIn(secret, joined)


class CliTests(unittest.TestCase):
    def _run(self, path: Path):
        return subprocess.run(
            [sys.executable, str(VALIDATOR_PATH), str(path)],
            capture_output=True,
            text=True,
        )

    def test_exit_0_on_valid(self) -> None:
        result = self._run(FIXTURE_DIR / "valid-minimal.json")
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_exit_1_on_invalid(self) -> None:
        result = self._run(FIXTURE_DIR / "invalid-approval-line.json")
        self.assertEqual(result.returncode, 1)

    def test_exit_2_on_missing_file(self) -> None:
        result = self._run(FIXTURE_DIR / "does-not-exist.json")
        self.assertEqual(result.returncode, 2)

    def test_self_test_mode_passes(self) -> None:
        result = subprocess.run(
            [sys.executable, str(VALIDATOR_PATH), "--self-test"],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_does_not_mutate_input(self) -> None:
        with tempfile.NamedTemporaryFile(
            "w", suffix=".json", delete=False
        ) as fh:
            json.dump(_valid_doc(), fh)
            tmp = Path(fh.name)
        try:
            before = tmp.read_bytes()
            self._run(tmp)
            self.assertEqual(tmp.read_bytes(), before)
        finally:
            tmp.unlink()


if __name__ == "__main__":
    unittest.main()
