"""Tests for the S2C-1a no-write advisory review-artifact transform.

Proves the merged S2C-1a contract: deterministic classification, a
NON-APPROVABLE artifact for the no-write advisory case (exact message, no
write preview_sha256), and clean refusals for patch_intent / unsafe / ambiguous
candidates. No test calls a model, the broker, or claw plan preview/approve/apply.
"""

from __future__ import annotations

import importlib.util
import json
from pathlib import Path

import pytest

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

@pytest.mark.parametrize(
    "fixture,expected",
    [
        ("valid-no-write-advisory.json", T.NO_WRITE_ADVISORY),
        ("has-patch-intent.json", T.WORKSPACE_WRITE_OUT_OF_SCOPE),
        ("unsafe-apply-request.json", T.REJECT_UNSAFE),
        ("raw-11434-reference.json", T.REJECT_UNSAFE),
        ("missing-objective.json", T.REJECT_AMBIGUOUS),
        ("ambiguous-no-next-step.json", T.REJECT_AMBIGUOUS),
    ],
)
def test_classification(fixture, expected):
    classification, _reason, artifact = _classify_fixture(fixture)
    assert classification == expected
    # Only the no-write advisory case produces an artifact.
    if expected == T.NO_WRITE_ADVISORY:
        assert artifact is not None
    else:
        assert artifact is None


def test_classification_is_deterministic():
    # Same input twice -> identical classification (no Math.random/Date dependence).
    a = _classify_fixture("valid-no-write-advisory.json")[0]
    b = _classify_fixture("valid-no-write-advisory.json")[0]
    assert a == b == T.NO_WRITE_ADVISORY


# --- the no-write advisory artifact is NON-APPROVABLE ------------------------

def test_no_write_artifact_invariants():
    _cls, _reason, artifact = _classify_fixture("valid-no-write-advisory.json")
    assert artifact["artifact_type"] == "no_write_advisory_review"
    assert artifact["message"] == "No workspace write proposed."
    assert artifact["approval_allowed"] is False
    assert artifact["apply_allowed"] is False
    assert artifact["workspace_write_preview"] is False
    assert artifact["preview_sha256"] is None
    # Never invents a write target / after-bytes source.
    assert "write_target" not in artifact
    assert "after_file" not in artifact


def test_no_write_artifact_carries_review_fields():
    _cls, _reason, artifact = _classify_fixture("valid-no-write-advisory.json")
    assert artifact["objective"]
    assert isinstance(artifact["assumptions_or_plan_summary"], list) and artifact["assumptions_or_plan_summary"]
    assert isinstance(artifact["proposed_next_steps"], list) and artifact["proposed_next_steps"]
    assert isinstance(artifact["risks"], list)
    assert artifact["source_candidate_path"].endswith("valid-no-write-advisory.json")
    # sha256 hex digest of the candidate bytes.
    assert len(artifact["source_candidate_sha256"]) == 64
    assert artifact["candidate_files_to_inspect"]  # fixture has one


# --- refusals carry no artifact ---------------------------------------------

@pytest.mark.parametrize(
    "fixture",
    [
        "has-patch-intent.json",
        "unsafe-apply-request.json",
        "raw-11434-reference.json",
        "missing-objective.json",
        "ambiguous-no-next-step.json",
    ],
)
def test_refusals_emit_no_artifact(fixture):
    _cls, _reason, artifact = _classify_fixture(fixture)
    assert artifact is None


def test_raw_11434_is_unsafe_not_ambiguous():
    cls, reason, _ = _classify_fixture("raw-11434-reference.json")
    assert cls == T.REJECT_UNSAFE
    assert "11434" in reason


def test_patch_intent_is_out_of_scope_not_transformed():
    cls, _reason, artifact = _classify_fixture("has-patch-intent.json")
    assert cls == T.WORKSPACE_WRITE_OUT_OF_SCOPE
    assert artifact is None


# --- CLI exit codes ----------------------------------------------------------

def test_cli_exit_codes(tmp_path):
    # no-write advisory -> 0 and prints an artifact to stdout (or --output)
    out = tmp_path / "artifact.json"
    rc = T.main(["--candidate", str(_FIXTURES / "valid-no-write-advisory.json"),
                 "--output", str(out)])
    assert rc == 0
    art = json.loads(out.read_text())
    assert art["message"] == "No workspace write proposed."
    assert art["approval_allowed"] is False

    # refusals -> distinct non-zero codes; no artifact file written
    assert T.main(["--candidate", str(_FIXTURES / "has-patch-intent.json")]) == 10
    assert T.main(["--candidate", str(_FIXTURES / "unsafe-apply-request.json")]) == 11
    assert T.main(["--candidate", str(_FIXTURES / "raw-11434-reference.json")]) == 11
    assert T.main(["--candidate", str(_FIXTURES / "missing-objective.json")]) == 12
    assert T.main(["--candidate", str(_FIXTURES / "ambiguous-no-next-step.json")]) == 12


def test_missing_file_is_usage_error():
    assert T.main(["--candidate", str(_FIXTURES / "does-not-exist.json")]) == 2


# --- the transform never reaches A2 / model / broker -------------------------

def test_source_contains_no_execution_primitives():
    """The transform cannot reach a model/broker/claw because it contains no
    shell-out or HTTP primitive. (We check execution primitives, not the
    descriptive prohibition text in the docstring/help.)"""
    src = _SCRIPT.read_text()
    for forbidden in ("httpx", "requests.", "/v1/chat/completions", "urllib",
                      "urlopen", "subprocess", "os.system", "os.popen", "os.exec"):
        assert forbidden not in src, f"transform must not contain execution primitive {forbidden!r}"
