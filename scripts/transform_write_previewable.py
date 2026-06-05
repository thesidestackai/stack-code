#!/usr/bin/env python3
"""S2C-1b workspace-write-previewable transform (read-only, non-approvable).

Transforms a **validated** `a2-l4-planner-output.v1` candidate that carries an
explicit, descriptive `patch_intent`, together with an **operator-supplied**
single bounded workspace-relative `write_target` and an **operator-supplied**
`after_file` path, into a **non-approvable** A2-plan preview-request skeleton (one
`mode: workspace-write` step) that the **existing** A2-L2b chain can take to
**preview only**.

What it is NOT: it never produces a workspace-write preview, never produces or
fabricates a `preview_sha256`/`PreviewRecord`, never makes its artifact
approvable/applyable, never invokes the A2 preview/approve/apply chain, never
calls a model or the broker, never writes a source file, and **never reads the
`after_file` bytes** — `after_file` is an operator-supplied PATH PLACEHOLDER whose
exact bytes are materialized later, by the operator, for the existing A2-L2b
preview. The A2-L2b preview/approve/apply chain (preview_sha256-bound) remains the
only write authority.

Design fact (load-bearing): the planner-output `patch_intent` is a CLOSED prose
object (`summary` + `notes`) and the planner-output schema carries NO
`write_target`/`after_file`. The transform therefore can never derive exact
after-bytes or a target from the candidate; both are operator-supplied via CLI
args, and the candidate's `patch_intent` is only the write-intent gate.

Classification (total, deterministic):

    REJECT_UNSAFE                       a forbidden execution/approval/bypass field,
                                        a raw localhost:11434 reference, or an
                                        operator write_target/after_file that
                                        violates workspace write-policy.
    REJECT_AMBIGUOUS                    schema/semantic-invalid planner-output, a
                                        missing objective, no proposed next step,
                                        or a missing/multiple write_target /
                                        missing after_file.
    NO_PATCH_INTENT_OUT_OF_SCOPE        valid planner-output with no patch_intent —
                                        a read-only advisory handled by S2C-1a
                                        (NO_WRITE_ADVISORY), not this slice.
    WORKSPACE_WRITE_PREVIEWABLE         valid, inert, has patch_intent + objective +
                                        a next step, with one safe operator
                                        write_target and a safe operator after_file
                                        -> emit a non-approvable plan skeleton.

Stdlib only (json, hashlib, argparse, fnmatch, importlib, pathlib, sys). Reuses
the existing read-only validator `scripts/validate_planner_output_schema.py`.

Exit codes:
    0   WORKSPACE_WRITE_PREVIEWABLE skeleton emitted
    10  NO_PATCH_INTENT_OUT_OF_SCOPE — refused (S2C-1a handles no-write candidates)
    11  REJECT_UNSAFE — refused
    12  REJECT_AMBIGUOUS — refused
    2   usage / IO error
"""

from __future__ import annotations

import argparse
import fnmatch
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

ARTIFACT_TYPE = "workspace_write_preview_request"
ARTIFACT_SCHEMA_VERSION = "a2-l4-write-preview-request.v1"

# Forbidden top-level execution/approval/bypass keys. Their presence is an UNSAFE
# request, not mere ambiguity. Mirrors the planner-output schema `not` set plus
# explicit preview/approval-bypass keys an S2C-1b candidate must never carry.
_UNSAFE_KEYS = (
    "approval_line", "approval_command", "apply_command", "apply_bundle_command",
    "run_command", "shell_command", "write_command", "autonomous_apply",
    "auto_approve", "raw_11434_endpoint", "secret_value", "token_value",
    "env_secret", "private_key",
    # preview/approval-bypass attempts: the transform never carries these.
    "preview_sha256", "preview_bundle", "approval_result", "apply_bundle",
)

# Raw upstream port, assembled so it appears here only as a denylist value.
_FORBIDDEN_PORT = "114" + "34"

# Workspace write-policy path denials (lexical only; mirrors a2-l2a after_file /
# write_target rules in docs/a2-plan-schema.md).
_DENIED_COMPONENTS = (".git", ".claw", ".claude")
_DENIED_FINAL_GLOBS = (".env", ".env*", "secret*", "credentials*", "*.pem", "*.key")

# Classification labels.
WORKSPACE_WRITE_PREVIEWABLE = "WORKSPACE_WRITE_PREVIEWABLE"
NO_PATCH_INTENT_OUT_OF_SCOPE = "NO_PATCH_INTENT_OUT_OF_SCOPE"
REJECT_UNSAFE = "REJECT_UNSAFE"
REJECT_AMBIGUOUS = "REJECT_AMBIGUOUS"

_EXIT_FOR_CLASS = {
    WORKSPACE_WRITE_PREVIEWABLE: 0,
    NO_PATCH_INTENT_OUT_OF_SCOPE: 10,
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


def _path_policy_violation(path: str) -> str | None:
    """Return a reason string if the workspace-relative path violates write-policy,
    else None. Lexical only: never opens, stat-s, or canonicalizes the path."""
    if not isinstance(path, str) or not path.strip():
        return "is empty"
    p = PurePosixPath(path)
    if p.is_absolute():
        return f"is absolute: {path!r}"
    parts = p.parts
    if ".." in parts:
        return f"contains a `..` escape: {path!r}"
    for denied in _DENIED_COMPONENTS:
        if denied in parts:
            return f"contains denied component {denied!r}: {path!r}"
    final = parts[-1] if parts else ""
    for pat in _DENIED_FINAL_GLOBS:
        if fnmatch.fnmatch(final, pat):
            return f"matches denied pattern {pat!r}: {path!r}"
    return None


def classify(doc: Any, write_targets, after_file, validator, schema: dict) -> tuple[str, str]:
    """Return (classification, reason). Total and deterministic.

    Order matters: UNSAFE candidate signals are checked before schema validity so a
    smuggled apply/approve/bypass field or a raw :11434 reference is classified
    UNSAFE rather than as generic ambiguity. Operator write_target/after_file checks
    run last, after the candidate is confirmed a valid, inert, write-intent document.
    """
    if not isinstance(doc, dict):
        return REJECT_AMBIGUOUS, "candidate is not a JSON object"

    # 1) UNSAFE: forbidden execution/approval/bypass keys, or raw upstream-port reference.
    present_unsafe = [k for k in _UNSAFE_KEYS if k in doc]
    if present_unsafe:
        return REJECT_UNSAFE, f"forbidden execution/approval/bypass field(s): {', '.join(present_unsafe)}"
    if _references_forbidden_port(doc):
        return REJECT_UNSAFE, "candidate references a raw localhost:11434 app-inference endpoint"

    # 2) Schema + semantic validity (reuse the read-only validator).
    failures = validator.validate_document(doc, schema)
    if failures:
        return REJECT_AMBIGUOUS, "planner-output failed validation: " + "; ".join(failures[:5])

    # 3) Write-intent gate: S2C-1b requires an explicit patch_intent. A no-write
    #    candidate is S2C-1a's NO_WRITE_ADVISORY, out of scope here.
    if doc.get("patch_intent") is None:
        return NO_PATCH_INTENT_OUT_OF_SCOPE, "no patch_intent — read-only advisory is handled by S2C-1a (NO_WRITE_ADVISORY)"

    # 4) Advisory completeness (stricter than the schema): objective + a next step.
    if not (isinstance(doc.get("task_summary"), str) and doc["task_summary"].strip()):
        return REJECT_AMBIGUOUS, "missing objective (task_summary)"
    next_steps = doc.get("operator_next_steps")
    if not (isinstance(next_steps, list) and len(next_steps) > 0):
        return REJECT_AMBIGUOUS, "no proposed next step (operator_next_steps is empty)"

    # 5) Operator-supplied write_target: exactly one, workspace-policy-safe.
    targets = [t for t in (write_targets or []) if isinstance(t, str) and t.strip()]
    if len(targets) == 0:
        return REJECT_AMBIGUOUS, "no operator-supplied write_target for the workspace-write step"
    if len(targets) > 1:
        return REJECT_AMBIGUOUS, f"multiple write_targets ({len(targets)}) — S2C-1b is single-target per bundle"
    target = targets[0]
    violation = _path_policy_violation(target)
    if violation:
        return REJECT_UNSAFE, f"write_target {violation}"

    # 6) Operator-supplied after_file: a PATH ONLY (bytes never read), policy-safe,
    #    and distinct from the live write_target.
    if not (isinstance(after_file, str) and after_file.strip()):
        return REJECT_AMBIGUOUS, "no operator-supplied after_file path for the workspace-write step"
    violation = _path_policy_violation(after_file)
    if violation:
        return REJECT_UNSAFE, f"after_file {violation}"
    if PurePosixPath(after_file) == PurePosixPath(target):
        return REJECT_UNSAFE, "after_file must not equal write_target (incoherent self-source)"

    return WORKSPACE_WRITE_PREVIEWABLE, "valid single-file workspace-write-previewable candidate"


def build_skeleton(doc: dict, candidate_path: str, candidate_bytes: bytes,
                   write_target: str, after_file: str) -> dict:
    """Build the NON-APPROVABLE A2-plan preview-request skeleton.

    Invariants (asserted by tests): approval_allowed/apply_allowed/
    workspace_write_preview are all False; preview_sha256 is None; exactly one
    workspace-write step carrying the operator write_target and the operator
    after_file PLACEHOLDER (whose bytes the transform never reads). No
    preview_sha256/PreviewRecord is ever fabricated.
    """
    patch_intent = doc.get("patch_intent") or {}
    summary = patch_intent.get("summary", "") if isinstance(patch_intent, dict) else ""
    description = summary or doc.get("task_summary", "")
    name = doc.get("task_id") or doc.get("task_summary", "") or "s2c1b-write-preview-request"
    return {
        "artifact_type": ARTIFACT_TYPE,
        "schema_version": ARTIFACT_SCHEMA_VERSION,
        "approval_allowed": False,
        "apply_allowed": False,
        "workspace_write_preview": False,
        "preview_sha256": None,
        "operator_action_required": (
            "Supply the exact after_file bytes, then run the existing A2-L2b "
            "workspace-write-preview (it alone produces the preview_sha256)."
        ),
        "plan": {
            "name": str(name),
            "mode": "workspace-write",
            "model_tier": "FAST",
            "steps": [
                {
                    "id": "s1",
                    "description": str(description),
                    "mode": "workspace-write",
                    "tools": ["Write"],
                    "write_target": {"path": write_target, "create_if_absent": False},
                    "after_file": after_file,
                    "expected_post_write": {"must_contain": [], "must_not_contain": []},
                }
            ],
        },
        "source_candidate_path": str(candidate_path),
        "source_candidate_sha256": hashlib.sha256(candidate_bytes).hexdigest(),
        "objective": doc.get("task_summary", ""),
        "assumptions_or_plan_summary": list(doc.get("plan_steps", [])),
        "risks": list(doc.get("risk_notes", [])),
        "operator_review_notes": (
            "Read-only preview-request skeleton. NON-APPROVABLE: it carries no write "
            "preview_sha256 and cannot be fed to plan approve/apply. The after_file is "
            "an operator-supplied placeholder; its bytes were NOT read or invented by "
            "this transform. A2-L2b remains the only write authority; supply the exact "
            "after_file bytes and run the existing workspace-write-preview."
        ),
    }


def transform_candidate(candidate_path: str, write_targets, after_file,
                        schema_path: Path | None = None):
    """Read+validate+classify a candidate. Returns (classification, reason, skeleton_or_None).

    Reads only the candidate bytes; never reads the after_file bytes and writes
    nothing. Raises TransformError on IO/usage problems.
    """
    p = Path(candidate_path)
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

    classification, reason = classify(doc, write_targets, after_file, validator, schema)
    if classification == WORKSPACE_WRITE_PREVIEWABLE:
        target = [t for t in write_targets if isinstance(t, str) and t.strip()][0]
        return classification, reason, build_skeleton(doc, candidate_path, raw, target, after_file)
    return classification, reason, None


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "S2C-1b workspace-write-previewable transform. Reads a validated "
            "a2-l4-planner-output.v1 candidate with an explicit patch_intent plus an "
            "operator-supplied single write_target and after_file path, and emits a "
            "NON-APPROVABLE A2-plan preview-request skeleton. Read-only: no model/broker "
            "call, no claw plan preview/approve/apply, no source writes, and the "
            "after_file bytes are never read."
        )
    )
    parser.add_argument("--candidate", required=True, help="path to a planner-output JSON candidate")
    parser.add_argument("--write-target", action="append", default=[], dest="write_target",
                        help="operator-supplied workspace-relative write target (give exactly one)")
    parser.add_argument("--after-file", default=None, dest="after_file",
                        help="operator-supplied after-bytes PATH placeholder (never read by this transform)")
    parser.add_argument("--schema", default=None, help="override schema path (default: repo planner-output schema)")
    parser.add_argument("--output", default=None, help="write skeleton JSON to this path (default: stdout)")
    args = parser.parse_args(argv)

    try:
        classification, reason, artifact = transform_candidate(
            args.candidate, args.write_target, args.after_file,
            Path(args.schema) if args.schema else None,
        )
    except TransformError as exc:
        print(f"TRANSFORM — USAGE/IO ERROR: {exc}", file=sys.stderr)
        return 2

    if classification == WORKSPACE_WRITE_PREVIEWABLE and artifact is not None:
        text = json.dumps(artifact, indent=2)
        if args.output:
            # Output is a bounded artifact path; never a source file of the candidate.
            Path(args.output).write_text(text + "\n", encoding="utf-8")
        else:
            print(text)
        return 0

    print(f"TRANSFORM — REFUSED [{classification}]: {reason}", file=sys.stderr)
    print("  No skeleton was emitted. A2 preview/approve/apply was not entered.", file=sys.stderr)
    return _EXIT_FOR_CLASS.get(classification, 2)


if __name__ == "__main__":
    raise SystemExit(main())
