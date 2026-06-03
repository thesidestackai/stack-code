#!/usr/bin/env python3
"""Read-only pretty-printer for the A2-L4 planner-output contract (S2A-7).

An operator display tool, not an authority surface. It reads one
planner-output JSON document, validates it with the read-only validator
(scripts/validate_planner_output_schema.py), and prints a human-friendly
report to stdout. Rendering a document grants nothing: a rendered
document is still inert and still requires operator judgement. The
A2-L2b preview/approve/apply chain remains the only write authority.

Boundaries (S2A-6 scope card):
  - read-only: opens nothing for writing, creates no file/dir, leaves the
    input byte-for-byte unchanged, never touches .claw/**
  - no-command: never runs a shell command/subprocess and never executes
    anything contained in the document; command-like text (plan steps,
    test suggestions, preview requests) is displayed as descriptive only,
    clearly marked "not executed"
  - no-model: calls no model/broker/Ollama; routes to no :11435 or :11434
  - no-approval: never generates/templates/echoes an approval line and
    never emits output a downstream tool could treat as an approval
  - refusal: a validator refusal (or unknown schema_version, malformed
    JSON, unreadable file) is rendered as REFUSED with a nonzero exit;
    never coerced, repaired, or partially rendered as accepted; forbidden
    payloads are reported by field name, never reproduced as a runnable
    body; secret-pattern hits are reported by field path, never by value

Stdlib only (json, argparse, importlib, pathlib, sys); no third-party
dependency. Imports the validator read-only (never modifies it).

Exit codes:
  0  valid and inert (rendered as VALID)
  1  refused (validator refusal) — rendered as REFUSED
  2  usage / IO error (unreadable input or schema)
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent
VALIDATOR_PATH = _SCRIPT_DIR / "validate_planner_output_schema.py"

_NOT_EXECUTED = "(descriptive only — not executed)"


def _load_validator():
    spec = importlib.util.spec_from_file_location(
        "validate_planner_output_schema", VALIDATOR_PATH
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


# --------------------------------------------------------------------------
# Rendering helpers (pure string building; no IO)
# --------------------------------------------------------------------------
def _line(out: list[str], text: str = "") -> None:
    out.append(text)


def _render_text_list(out: list[str], title: str, items: Any, note: str = "") -> None:
    if not items:
        return
    heading = f"{title}:" if not note else f"{title} {note}:"
    _line(out, heading)
    for item in items:
        _line(out, f"  - {item}")
    _line(out)


def _render_valid(doc: dict) -> str:
    out: list[str] = []
    _line(out, "=" * 60)
    _line(out, "VALID (inert) — planner output is well-formed and carries no")
    _line(out, "executable command, approval line, raw :11434 endpoint, or secret.")
    _line(out, "Rendering grants no authority; operator review is still required.")
    _line(out, "=" * 60)
    _line(out)
    _line(out, f"task_id:        {doc.get('task_id', '')}")
    _line(out, f"schema_version: {doc.get('schema_version', '')}")
    _line(out, f"workspace_root: {doc.get('workspace_root', '')}")
    _line(out)
    _line(out, "Task summary:")
    _line(out, f"  {doc.get('task_summary', '')}")
    _line(out)

    if doc.get("repo_context_summary"):
        _line(out, "Repo context summary:")
        _line(out, f"  {doc['repo_context_summary']}")
        _line(out)

    steps = doc.get("plan_steps", [])
    if steps:
        _line(out, "Plan steps:")
        for i, step in enumerate(steps, 1):
            _line(out, f"  {i}. [{step.get('step_id', '')}] {step.get('description', '')}")
            if step.get("rationale"):
                _line(out, f"     rationale: {step['rationale']}")
        _line(out)

    _render_text_list(out, "Risk notes", doc.get("risk_notes"))
    _render_text_list(out, "Candidate files (for inspection)", doc.get("candidate_files"))
    _render_text_list(out, "Test suggestions", doc.get("test_suggestions"), _NOT_EXECUTED)
    _render_text_list(out, "Operator next steps", doc.get("operator_next_steps"))

    patch = doc.get("patch_intent")
    if isinstance(patch, dict) and (patch.get("summary") or patch.get("notes")):
        _line(out, "Patch intent (descriptive notes only — not an applyable patch):")
        if patch.get("summary"):
            _line(out, f"  summary: {patch['summary']}")
        for note in patch.get("notes", []):
            _line(out, f"  - {note}")
        _line(out)

    preview = doc.get("preview_request")
    if isinstance(preview, dict) and preview.get("requested"):
        _line(out, f"Preview request (advisory option for the operator {_NOT_EXECUTED}):")
        if preview.get("reason"):
            _line(out, f"  reason: {preview['reason']}")
        _line(out)

    verifier = doc.get("external_verifier_handoff")
    if isinstance(verifier, dict) and (verifier.get("summary") or verifier.get("context")):
        _line(out, "External verifier handoff (advisory, secret-free; sending is an operator gesture):")
        if verifier.get("summary"):
            _line(out, f"  summary: {verifier['summary']}")
        for ctx in verifier.get("context", []):
            _line(out, f"  - {ctx}")
        _line(out)

    status = doc.get("status_snapshot")
    if isinstance(status, dict) and (status.get("summary") or status.get("plan_state")):
        _line(out, "Status snapshot (read-only summary):")
        if status.get("plan_state"):
            _line(out, f"  plan_state: {status['plan_state']}")
        if status.get("summary"):
            _line(out, f"  summary: {status['summary']}")
        _line(out)

    return "\n".join(out).rstrip() + "\n"


def _render_refusal(failures: list[str]) -> str:
    out: list[str] = []
    _line(out, "=" * 60)
    _line(out, "REFUSED — this document is NOT a valid, inert planner output and")
    _line(out, "must not be acted on. The following checks failed:")
    _line(out, "=" * 60)
    _line(out)
    for f in failures:
        _line(out, f"  - {f}")
    _line(out)
    _line(out, "No fields are rendered as accepted. Forbidden payloads are named,")
    _line(out, "not reproduced; secret-pattern hits show the field path only.")
    return "\n".join(out).rstrip() + "\n"


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Read-only pretty-printer for the A2-L4 planner-output contract."
    )
    parser.add_argument(
        "input",
        help="path to a planner-output JSON document ('-' to read stdin)",
    )
    parser.add_argument(
        "--schema", default=None,
        help="path to the planner-output JSON Schema (default: validator default)",
    )
    args = parser.parse_args(argv)

    validator = _load_validator()
    schema_path = Path(args.schema) if args.schema else validator.DEFAULT_SCHEMA_PATH
    try:
        schema = validator.load_schema(schema_path)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: could not read schema {schema_path}: {exc}", file=sys.stderr)
        return 2

    try:
        if args.input == "-":
            raw = sys.stdin.read()
        else:
            raw = Path(args.input).read_text(encoding="utf-8")
        doc = json.loads(raw)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: could not read input {args.input}: {exc}", file=sys.stderr)
        return 2

    failures = validator.validate_document(doc, schema)
    if failures:
        sys.stdout.write(_render_refusal(failures))
        return 1
    sys.stdout.write(_render_valid(doc))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
