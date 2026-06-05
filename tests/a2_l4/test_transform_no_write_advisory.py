"""Tests for the S2C-1a no-write advisory review-artifact transform.

Proves the merged S2C-1a contract: deterministic classification, a
NON-APPROVABLE artifact for the no-write advisory case (exact message, no
write preview_sha256), and clean refusals for patch_intent / unsafe / ambiguous
candidates. No test calls a model, the broker, or claw plan preview/approve/apply.

Stdlib unittest only (matching tests/a2_l4/test_validate_planner_output_schema.py);
no new dependency. Run with:
    python3 -m unittest tests.a2_l4.test_transform_no_write_advisory
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[2]
_SCRIPT = _REPO_ROOT / "scripts" / "transform_no_write_advisory.py"
_FIXTURES = _REPO_ROOT / "schemas" / "a2-l4" / "fixtures" / "no-write-advisory"


def _load_module():
    spec = importlib.util.spec_from_file_location("transform_no_write_advisory", _SCRIPT)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


T = _load_module()


def _classify_fixture(name: str):
    classification, reason, artifact = T.transform_candidate(str(_FIXTURES / name))
    return classification, reason, artifact


# --- classification: total + deterministic over every fixture ---------------

class ClassificationTests(unittest.TestCase):
    def test_classification(self) -> None:
        cases = [
            ("valid-no-write-advisory.json", T.NO_WRITE_ADVISORY),
            ("has-patch-intent.json", T.WORKSPACE_WRITE_OUT_OF_SCOPE),
            ("unsafe-apply-request.json", T.REJECT_UNSAFE),
            ("raw-11434-reference.json", T.REJECT_UNSAFE),
            ("missing-objective.json", T.REJECT_AMBIGUOUS),
            ("ambiguous-no-next-step.json", T.REJECT_AMBIGUOUS),
        ]
        for fixture, expected in cases:
            with self.subTest(fixture=fixture):
                classification, _reason, artifact = _classify_fixture(fixture)
                self.assertEqual(classification, expected)
                # Only the no-write advisory case produces an artifact.
                if expected == T.NO_WRITE_ADVISORY:
                    self.assertIsNotNone(artifact)
                else:
                    self.assertIsNone(artifact)

    def test_classification_is_deterministic(self) -> None:
        # Same input twice -> identical classification (no Math.random/Date dependence).
        a = _classify_fixture("valid-no-write-advisory.json")[0]
        b = _classify_fixture("valid-no-write-advisory.json")[0]
        self.assertEqual(a, b)
        self.assertEqual(a, T.NO_WRITE_ADVISORY)


# --- the no-write advisory artifact is NON-APPROVABLE ------------------------

class NoWriteArtifactTests(unittest.TestCase):
    def test_no_write_artifact_invariants(self) -> None:
        _cls, _reason, artifact = _classify_fixture("valid-no-write-advisory.json")
        self.assertEqual(artifact["artifact_type"], "no_write_advisory_review")
        self.assertEqual(artifact["message"], "No workspace write proposed.")
        self.assertIs(artifact["approval_allowed"], False)
        self.assertIs(artifact["apply_allowed"], False)
        self.assertIs(artifact["workspace_write_preview"], False)
        self.assertIsNone(artifact["preview_sha256"])
        # Never invents a write target / after-bytes source.
        self.assertNotIn("write_target", artifact)
        self.assertNotIn("after_file", artifact)

    def test_no_write_artifact_carries_review_fields(self) -> None:
        _cls, _reason, artifact = _classify_fixture("valid-no-write-advisory.json")
        self.assertTrue(artifact["objective"])
        self.assertIsInstance(artifact["assumptions_or_plan_summary"], list)
        self.assertTrue(artifact["assumptions_or_plan_summary"])
        self.assertIsInstance(artifact["proposed_next_steps"], list)
        self.assertTrue(artifact["proposed_next_steps"])
        self.assertIsInstance(artifact["risks"], list)
        self.assertTrue(artifact["source_candidate_path"].endswith("valid-no-write-advisory.json"))
        # sha256 hex digest of the candidate bytes.
        self.assertEqual(len(artifact["source_candidate_sha256"]), 64)
        self.assertTrue(artifact["candidate_files_to_inspect"])  # fixture has one


# --- refusals carry no artifact ---------------------------------------------

class RefusalTests(unittest.TestCase):
    def test_refusals_emit_no_artifact(self) -> None:
        fixtures = [
            "has-patch-intent.json",
            "unsafe-apply-request.json",
            "raw-11434-reference.json",
            "missing-objective.json",
            "ambiguous-no-next-step.json",
        ]
        for fixture in fixtures:
            with self.subTest(fixture=fixture):
                _cls, _reason, artifact = _classify_fixture(fixture)
                self.assertIsNone(artifact)

    def test_raw_11434_is_unsafe_not_ambiguous(self) -> None:
        cls, reason, _ = _classify_fixture("raw-11434-reference.json")
        self.assertEqual(cls, T.REJECT_UNSAFE)
        self.assertIn("11434", reason)

    def test_patch_intent_is_out_of_scope_not_transformed(self) -> None:
        cls, _reason, artifact = _classify_fixture("has-patch-intent.json")
        self.assertEqual(cls, T.WORKSPACE_WRITE_OUT_OF_SCOPE)
        self.assertIsNone(artifact)


# --- CLI exit codes ----------------------------------------------------------

class CliExitCodeTests(unittest.TestCase):
    def test_cli_exit_codes(self) -> None:
        # no-write advisory -> 0 and prints an artifact to stdout (or --output)
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "artifact.json"
            rc = T.main(["--candidate", str(_FIXTURES / "valid-no-write-advisory.json"),
                         "--output", str(out)])
            self.assertEqual(rc, 0)
            art = json.loads(out.read_text())
            self.assertEqual(art["message"], "No workspace write proposed.")
            self.assertIs(art["approval_allowed"], False)

        # refusals -> distinct non-zero codes; no artifact file written
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "has-patch-intent.json")]), 10)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "unsafe-apply-request.json")]), 11)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "raw-11434-reference.json")]), 11)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "missing-objective.json")]), 12)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "ambiguous-no-next-step.json")]), 12)

    def test_missing_file_is_usage_error(self) -> None:
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "does-not-exist.json")]), 2)


# --- the transform never reaches A2 / model / broker -------------------------

class NoExecutionPrimitiveTests(unittest.TestCase):
    def test_source_contains_no_execution_primitives(self) -> None:
        """The transform cannot reach a model/broker/claw because it contains no
        shell-out or HTTP primitive. (We check execution primitives, not the
        descriptive prohibition text in the docstring/help.)"""
        src = _SCRIPT.read_text()
        for forbidden in ("httpx", "requests.", "/v1/chat/completions", "urllib",
                          "urlopen", "subprocess", "os.system", "os.popen", "os.exec"):
            self.assertNotIn(
                forbidden, src,
                f"transform must not contain execution primitive {forbidden!r}",
            )


if __name__ == "__main__":
    unittest.main()
