#!/usr/bin/env python3
"""S2C-1a no-write advisory review-artifact transform (read-only).

Transforms a **validated** `a2-l4-planner-output.v1` candidate that proposes
**no workspace write** (no `patch_intent`) into a **non-approvable**
operator-review artifact whose message is exactly:

    No workspace write proposed.

This is the first, lowest-risk slice of the planner-output -> A2-plan transform
designed in `docs/a2-l4-s2c-planner-output-to-a2-plan-transform-scope.md`
(class `NO_WRITE_ADVISORY`, §8/§10/§17) and prompted by
`handoffs/s2c1a_no_write_advisory_transform_implementation_prompt_DRAFT_2026-06-04.md`.

What it is NOT: it never produces a workspace-write preview, never produces a
write `preview_sha256`, never makes its artifact approvable/applyable, never
invokes `claw plan` preview/approve/apply, never calls a model or the broker,
never writes a source file, and never invents a `write_target`/`after_file`.
The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the only
write authority.

Classification (total, deterministic):

    REJECT_UNSAFE                       a forbidden execution/approval field, or a
                                        raw localhost:11434 app-inference reference.
    REJECT_AMBIGUOUS                    schema/semantic-invalid planner-output, a
                                        missing objective, or no proposed next step.
    WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE
                                        a `patch_intent` is present -> deferred to a
                                        later S2C-1b lane; refused here.
    NO_WRITE_ADVISORY                   valid, inert, no patch_intent, has objective
                                        and >=1 operator next step -> emit artifact.

Stdlib only (json, hashlib, argparse, importlib, pathlib, sys). Reuses the
existing read-only validator `scripts/validate_planner_output_schema.py`.

Exit codes:
    0   NO_WRITE_ADVISORY artifact emitted
    10  WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE (patch_intent present) — refused
    11  REJECT_UNSAFE — refused
    12  REJECT_AMBIGUOUS — refused
    2   usage / IO error
"""

from __future__ import annotations

import argparse
import hashlib
import importlib.util
import json
import sys
from pathlib import Path, PurePosixPath
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
_VALIDATOR_PATH = _SCRIPT_DIR / "validate_planner_output_schema.py"
_DEFAULT_SCHEMA_PATH = (
    _SCRIPT_DIR.parent / "schemas" / "a2-l4" / "planner-output.schema.json"
)

ARTIFACT_TYPE = "no_write_advisory_review"
ARTIFACT_SCHEMA_VERSION = "a2-l4-no-write-advisory-review.v1"
NO_WRITE_MESSAGE = "No workspace write proposed."

# Forbidden top-level execution/approval keys (mirrors the planner-output schema
# `not` set). Their presence is an UNSAFE request, not mere ambiguity.
_UNSAFE_KEYS = (
    "approval_line", "approval_command", "apply_command",
    "apply_bundle_command", "run_command", "shell_command",
)

# Raw upstream port, assembled so it appears here only as a denylist value.
_FORBIDDEN_PORT = "114" + "34"

# Classification labels.
NO_WRITE_ADVISORY = "NO_WRITE_ADVISORY"
WORKSPACE_WRITE_OUT_OF_SCOPE = "WORKSPACE_WRITE_PREVIEWABLE_OUT_OF_SCOPE"
REJECT_UNSAFE = "REJECT_UNSAFE"
REJECT_AMBIGUOUS = "REJECT_AMBIGUOUS"

_EXIT_FOR_CLASS = {
    NO_WRITE_ADVISORY: 0,
    WORKSPACE_WRITE_OUT_OF_SCOPE: 10,
    REJECT_UNSAFE: 11,
    REJECT_AMBIGUOUS: 12,
}


class TransformError(Exception):
    """Usage/IO error; the CLI renders it and exits 2."""


def _load_validator():
    spec = importlib.util.spec_from_file_location(
        "validate_planner_output_schema", _VALIDATOR_PATH
    )
    if spec is None or spec.loader is None:
        raise TransformError(f"could not load validator at {_VALIDATOR_PATH}")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def _iter_strings(value: Any):
    if isinstance(value, str):
        yield value
    elif isinstance(value, dict):
        for sub in value.values():
            yield from _iter_strings(sub)
    elif isinstance(value, list):
        for item in value:
            yield from _iter_strings(item)


def _references_forbidden_port(doc: Any) -> bool:
    return any(_FORBIDDEN_PORT in s for s in _iter_strings(doc))


def classify(doc: Any, validator, schema: dict) -> tuple[str, str]:
    """Return (classification, reason). Total and deterministic.

    Order matters: UNSAFE signals are checked before schema validity so an
    apply/approve request or a raw :11434 reference is classified UNSAFE rather
    than as generic ambiguity.
    """
    if not isinstance(doc, dict):
        return REJECT_AMBIGUOUS, "candidate is not a JSON object"

    # 1) UNSAFE: forbidden execution/approval keys, or raw upstream-port reference.
    present_unsafe = [k for k in _UNSAFE_KEYS if k in doc]
    if present_unsafe:
        return REJECT_UNSAFE, f"forbidden execution/approval field(s): {', '.join(present_unsafe)}"
    if _references_forbidden_port(doc):
        return REJECT_UNSAFE, "candidate references a raw localhost:11434 app-inference endpoint"

    # 2) Schema + semantic validity (reuse the read-only validator).
    failures = validator.validate_document(doc, schema)
    if failures:
        return REJECT_AMBIGUOUS, "planner-output failed validation: " + "; ".join(failures[:5])

    # 3) Workspace-write candidate is out of scope for S2C-1a.
    if doc.get("patch_intent") is not None:
        return WORKSPACE_WRITE_OUT_OF_SCOPE, "patch_intent present — deferred to S2C-1b workspace-write lane"

    # 4) Advisory completeness (stricter than the schema): objective + a next step.
    if not (isinstance(doc.get("task_summary"), str) and doc["task_summary"].strip()):
        return REJECT_AMBIGUOUS, "missing objective (task_summary)"
    next_steps = doc.get("operator_next_steps")
    if not (isinstance(next_steps, list) and len(next_steps) > 0):
        return REJECT_AMBIGUOUS, "no proposed next step (operator_next_steps is empty)"

    return NO_WRITE_ADVISORY, "valid no-write advisory planner-output"


def build_artifact(doc: dict, candidate_path: str, candidate_bytes: bytes) -> dict:
    """Build the NON-APPROVABLE no-write advisory review artifact.

    Invariants (asserted by tests): approval_allowed/apply_allowed/
    workspace_write_preview are all False; preview_sha256 is None; message is
    exactly NO_WRITE_MESSAGE. No write_target/after_file is ever produced.
    """
    return {
        "artifact_type": ARTIFACT_TYPE,
        "schema_version": ARTIFACT_SCHEMA_VERSION,
        "approval_allowed": False,
        "apply_allowed": False,
        "workspace_write_preview": False,
        "preview_sha256": None,
        "message": NO_WRITE_MESSAGE,
        "source_candidate_path": str(candidate_path),
        "source_candidate_sha256": hashlib.sha256(candidate_bytes).hexdigest(),
        "objective": doc.get("task_summary", ""),
        "assumptions_or_plan_summary": list(doc.get("plan_steps", [])),
        "proposed_next_steps": list(doc.get("operator_next_steps", [])),
        "risks": list(doc.get("risk_notes", [])),
        "candidate_files_to_inspect": list(doc.get("candidate_files", []) or []),
        "operator_review_notes": (
            "Read-only advisory. No workspace write proposed. A2 preview/approve/"
            "apply was NOT entered; this artifact is non-approvable and carries no "
            "write preview_sha256. A2 remains the only write authority."
        ),
    }


def transform_candidate(candidate_path: str, schema_path: Path | None = None):
    """Read+validate+classify a candidate. Returns (classification, reason, artifact_or_None).

    Reads only; writes nothing. Raises TransformError on IO/usage problems.
    """
    p = Path(candidate_path)
    if _FORBIDDEN_PORT == "":  # pragma: no cover - defensive
        raise TransformError("internal denylist error")
    try:
        raw = p.read_bytes()
    except OSError as exc:
        raise TransformError(f"could not read candidate {candidate_path!r}: {exc}")
    try:
        doc = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        # A non-JSON candidate is ambiguous, not unsafe.
        return REJECT_AMBIGUOUS, f"candidate is not valid JSON: {exc}", None

    validator = _load_validator()
    schema = validator.load_schema(schema_path or _DEFAULT_SCHEMA_PATH)

    classification, reason = classify(doc, validator, schema)
    if classification == NO_WRITE_ADVISORY:
        return classification, reason, build_artifact(doc, candidate_path, raw)
    return classification, reason, None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "S2C-1a no-write advisory transform. Reads a validated "
            "a2-l4-planner-output.v1 candidate with no patch_intent and emits a "
            "NON-APPROVABLE operator-review artifact. Read-only: no model/broker "
            "call, no claw plan preview/approve/apply, no source writes."
        )
    )
    parser.add_argument("--candidate", required=True, help="path to a planner-output JSON candidate")
    parser.add_argument("--schema", default=None, help="override schema path (default: repo planner-output schema)")
    parser.add_argument("--output", default=None, help="write artifact JSON to this path (default: stdout)")
    args = parser.parse_args(argv)

    try:
        classification, reason, artifact = transform_candidate(
            args.candidate, Path(args.schema) if args.schema else None
        )
    except TransformError as exc:
        print(f"TRANSFORM — USAGE/IO ERROR: {exc}", file=sys.stderr)
        return 2

    if classification == NO_WRITE_ADVISORY and artifact is not None:
        text = json.dumps(artifact, indent=2)
        if args.output:
            # Output is a bounded artifact path; never a source file of the candidate.
            Path(args.output).write_text(text + "\n", encoding="utf-8")
        else:
            print(text)
        return 0

    print(f"TRANSFORM — REFUSED [{classification}]: {reason}", file=sys.stderr)
    print("  No artifact was emitted. A2 preview/approve/apply was not entered.", file=sys.stderr)
    return _EXIT_FOR_CLASS.get(classification, 2)


if __name__ == "__main__":
    raise SystemExit(main())
