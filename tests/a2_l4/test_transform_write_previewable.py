"""Tests for the S2C-1b workspace-write-previewable transform.

Proves the S2C-1b contract: a validated `a2-l4-planner-output.v1` candidate that
carries a descriptive `patch_intent`, combined with an OPERATOR-SUPPLIED single
bounded workspace-relative `write_target` and an OPERATOR-SUPPLIED `after_file`
path, is transformed into a NON-APPROVABLE A2-plan preview-request skeleton (one
workspace-write step). The transform never reads `after_file` bytes, never
fabricates a `preview_sha256`, and never approves/applies. The existing A2-L2b
chain remains the only producer of `preview_sha256`.

`patch_intent` is a closed prose object (summary + notes) and the planner-output
schema carries NO `write_target`/`after_file`, so those are operator-supplied via
CLI args; candidate fixtures exercise the candidate-side classification while the
operator-target/after_file conditions are exercised by varying the supplied args.

Stdlib unittest only (matching tests/a2_l4/test_validate_planner_output_schema.py);
no new dependency. Run with:
    python3 -m unittest tests.a2_l4.test_transform_write_previewable
"""

from __future__ import annotations

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

_REPO_ROOT = Path(__file__).resolve().parents[2]
_SCRIPT = _REPO_ROOT / "scripts" / "transform_write_previewable.py"
_FIXTURES = _REPO_ROOT / "schemas" / "a2-l4" / "fixtures" / "write-previewable"

# Canonical operator-supplied, lexically-safe paths for the previewable path.
SAFE_TARGET = "notes/scratch.md"
SAFE_AFTER = "materialized/scratch.after"


def _load_module():
    spec = importlib.util.spec_from_file_location("transform_write_previewable", _SCRIPT)
    assert spec is not None and spec.loader is not None
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


T = _load_module()


def _classify(name: str, write_targets=(SAFE_TARGET,), after_file=SAFE_AFTER):
    return T.transform_candidate(str(_FIXTURES / name), list(write_targets), after_file)


# --- candidate-side classification (good operator args) ----------------------

class CandidateClassificationTests(unittest.TestCase):
    def test_valid_candidate_with_good_args_is_previewable(self) -> None:
        cls, _reason, artifact = _classify("valid-with-patch-intent.json")
        self.assertEqual(cls, T.WORKSPACE_WRITE_PREVIEWABLE)
        self.assertIsNotNone(artifact)

    def test_no_patch_intent_is_out_of_scope(self) -> None:
        cls, _reason, artifact = _classify("no-patch-intent.json")
        self.assertEqual(cls, T.NO_PATCH_INTENT_OUT_OF_SCOPE)
        self.assertIsNone(artifact)

    def test_unsafe_apply_request_rejected(self) -> None:
        cls, _reason, artifact = _classify("unsafe-apply-request.json")
        self.assertEqual(cls, T.REJECT_UNSAFE)
        self.assertIsNone(artifact)

    def test_raw_11434_rejected_unsafe(self) -> None:
        cls, reason, _ = _classify("raw-11434-reference.json")
        self.assertEqual(cls, T.REJECT_UNSAFE)
        self.assertIn("11434", reason)

    def test_dangerous_command_rejected_unsafe(self) -> None:
        cls, _reason, artifact = _classify("dangerous-command-request.json")
        self.assertEqual(cls, T.REJECT_UNSAFE)
        self.assertIsNone(artifact)

    def test_preview_sha256_bypass_rejected_unsafe(self) -> None:
        cls, _reason, artifact = _classify("preview-sha256-bypass.json")
        self.assertEqual(cls, T.REJECT_UNSAFE)
        self.assertIsNone(artifact)

    def test_missing_objective_rejected_ambiguous(self) -> None:
        cls, _reason, artifact = _classify("missing-objective.json")
        self.assertEqual(cls, T.REJECT_AMBIGUOUS)
        self.assertIsNone(artifact)

    def test_ambiguous_no_next_step_rejected_ambiguous(self) -> None:
        cls, _reason, artifact = _classify("ambiguous-no-next-step.json")
        self.assertEqual(cls, T.REJECT_AMBIGUOUS)
        self.assertIsNone(artifact)

    def test_classification_is_total_and_deterministic(self) -> None:
        expected = {
            "valid-with-patch-intent.json": T.WORKSPACE_WRITE_PREVIEWABLE,
            "no-patch-intent.json": T.NO_PATCH_INTENT_OUT_OF_SCOPE,
            "unsafe-apply-request.json": T.REJECT_UNSAFE,
            "raw-11434-reference.json": T.REJECT_UNSAFE,
            "dangerous-command-request.json": T.REJECT_UNSAFE,
            "preview-sha256-bypass.json": T.REJECT_UNSAFE,
            "missing-objective.json": T.REJECT_AMBIGUOUS,
            "ambiguous-no-next-step.json": T.REJECT_AMBIGUOUS,
        }
        for fixture, want in expected.items():
            with self.subTest(fixture=fixture):
                a = _classify(fixture)[0]
                b = _classify(fixture)[0]
                self.assertEqual(a, want)
                self.assertEqual(a, b)  # deterministic


# --- operator-supplied target / after_file conditions (valid candidate) ------

class OperatorTargetTests(unittest.TestCase):
    def test_missing_write_target_rejected_ambiguous(self) -> None:
        cls, _reason, artifact = _classify("valid-with-patch-intent.json", write_targets=())
        self.assertEqual(cls, T.REJECT_AMBIGUOUS)
        self.assertIsNone(artifact)

    def test_multiple_write_targets_rejected_ambiguous(self) -> None:
        cls, _reason, _ = _classify(
            "valid-with-patch-intent.json", write_targets=("a/one.md", "b/two.md")
        )
        self.assertEqual(cls, T.REJECT_AMBIGUOUS)

    def test_missing_after_file_rejected_ambiguous(self) -> None:
        cls, _reason, _ = _classify("valid-with-patch-intent.json", after_file=None)
        self.assertEqual(cls, T.REJECT_AMBIGUOUS)

    def test_absolute_target_rejected_unsafe(self) -> None:
        cls, _reason, _ = _classify("valid-with-patch-intent.json", write_targets=("/etc/passwd",))
        self.assertEqual(cls, T.REJECT_UNSAFE)

    def test_target_outside_workspace_rejected_unsafe(self) -> None:
        cls, _reason, _ = _classify("valid-with-patch-intent.json", write_targets=("../escape.md",))
        self.assertEqual(cls, T.REJECT_UNSAFE)

    def test_denyglob_target_rejected_unsafe(self) -> None:
        for bad in (".git/config", "notes/.env", "secrets/app.pem", ".claw/x"):
            with self.subTest(target=bad):
                cls, _reason, _ = _classify("valid-with-patch-intent.json", write_targets=(bad,))
                self.assertEqual(cls, T.REJECT_UNSAFE)

    def test_after_file_equal_target_rejected_unsafe(self) -> None:
        cls, _reason, _ = _classify(
            "valid-with-patch-intent.json", write_targets=(SAFE_TARGET,), after_file=SAFE_TARGET
        )
        self.assertEqual(cls, T.REJECT_UNSAFE)

    def test_after_file_denyglob_rejected_unsafe(self) -> None:
        cls, _reason, _ = _classify(
            "valid-with-patch-intent.json", write_targets=(SAFE_TARGET,), after_file="../x.after"
        )
        self.assertEqual(cls, T.REJECT_UNSAFE)


# --- the previewable skeleton is NON-APPROVABLE ------------------------------

class SkeletonInvariantTests(unittest.TestCase):
    def _artifact(self):
        _cls, _reason, artifact = _classify("valid-with-patch-intent.json")
        return artifact

    def test_previewable_skeleton_invariants(self) -> None:
        art = self._artifact()
        self.assertEqual(art["artifact_type"], "workspace_write_preview_request")
        self.assertIs(art["approval_allowed"], False)
        self.assertIs(art["apply_allowed"], False)
        self.assertIs(art["workspace_write_preview"], False)
        self.assertIsNone(art["preview_sha256"])

    def test_skeleton_has_exactly_one_workspace_write_step(self) -> None:
        art = self._artifact()
        plan = art["plan"]
        self.assertEqual(plan["mode"], "workspace-write")
        self.assertEqual(len(plan["steps"]), 1)
        step = plan["steps"][0]
        self.assertEqual(step["mode"], "workspace-write")
        self.assertEqual(step["tools"], ["Write"])
        self.assertEqual(step["write_target"]["path"], SAFE_TARGET)
        self.assertEqual(step["after_file"], SAFE_AFTER)
        self.assertIn("expected_post_write", step)

    def test_skeleton_carries_review_fields(self) -> None:
        art = self._artifact()
        self.assertTrue(art["source_candidate_path"].endswith("valid-with-patch-intent.json"))
        self.assertEqual(len(art["source_candidate_sha256"]), 64)
        self.assertTrue(art["operator_review_notes"])

    def test_after_file_is_placeholder_not_read(self) -> None:
        # A non-existent after_file path must NOT block the skeleton: the transform
        # records the path and never opens it.
        cls, _reason, art = _classify(
            "valid-with-patch-intent.json", after_file="materialized/does-not-exist.after"
        )
        self.assertEqual(cls, T.WORKSPACE_WRITE_PREVIEWABLE)
        self.assertEqual(art["plan"]["steps"][0]["after_file"], "materialized/does-not-exist.after")

    def test_no_fabricated_preview_sha256_anywhere(self) -> None:
        art = self._artifact()
        blob = json.dumps(art)
        # preview_sha256 may appear as the explicit null field, but never as a 64-hex value.
        self.assertIsNone(art["preview_sha256"])
        self.assertNotIn("payload_sha256", blob)
        self.assertNotIn("before_sha256", blob)
        self.assertNotIn("after_sha256", blob)


# --- CLI exit codes ----------------------------------------------------------

class CliExitCodeTests(unittest.TestCase):
    def test_cli_exit_codes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "skeleton.json"
            rc = T.main([
                "--candidate", str(_FIXTURES / "valid-with-patch-intent.json"),
                "--write-target", SAFE_TARGET,
                "--after-file", SAFE_AFTER,
                "--output", str(out),
            ])
            self.assertEqual(rc, 0)
            art = json.loads(out.read_text())
            self.assertEqual(art["artifact_type"], "workspace_write_preview_request")
            self.assertIs(art["approval_allowed"], False)

        base = ["--write-target", SAFE_TARGET, "--after-file", SAFE_AFTER]
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "no-patch-intent.json")] + base), 10)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "unsafe-apply-request.json")] + base), 11)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "raw-11434-reference.json")] + base), 11)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "missing-objective.json")] + base), 12)
        self.assertEqual(T.main(["--candidate", str(_FIXTURES / "ambiguous-no-next-step.json")] + base), 12)

    def test_missing_file_is_usage_error(self) -> None:
        rc = T.main([
            "--candidate", str(_FIXTURES / "does-not-exist.json"),
            "--write-target", SAFE_TARGET, "--after-file", SAFE_AFTER,
        ])
        self.assertEqual(rc, 2)


# --- the transform never reaches A2 / model / broker -------------------------

class NoExecutionPrimitiveTests(unittest.TestCase):
    def test_source_contains_no_execution_primitives(self) -> None:
        src = _SCRIPT.read_text()
        for forbidden in ("httpx", "requests.", "/v1/chat/completions", "urllib",
                          "urlopen", "subprocess", "os.system", "os.popen", "os.exec"):
            self.assertNotIn(
                forbidden, src,
                f"transform must not contain execution primitive {forbidden!r}",
            )


if __name__ == "__main__":
    unittest.main()
