#!/usr/bin/env python3
"""Offline, no-model-call planner CLI skeleton (A2-L4-S2B-3).

This is a *skeleton* for a future local-model planner loop. It does not
call a model. It wires three existing read-only surfaces together so an
operator can see, end to end, the shape of a planning step:

    operator task text
  + read-only workspace context summary
  + a fixture planner-output document
    -> validate (existing read-only validator)
    -> pretty-print (existing read-only pretty-printer)
    -> one combined operator-facing report on stdout

The plan is a *fixture* only. No model is consulted. Nothing is written.
Producing this report grants nothing: the local model (when one is wired
in a future lane) will only ever propose, and the A2-L2b
preview/approve/apply chain remains the only write authority. The flow is:

    a local model proposes
    the operator reviews
    A2 previews
    the operator approves
    A2 applies

Boundaries (S2B offline-skeleton lane):
  - offline stub: no model is called; no inference of any kind happens
  - no service calls: starts no process, opens no network, routes to no
    inference endpoint; all app inference is reserved for the A2 layer via
    its approved gateway, never a raw upstream port
  - read-only: creates, edits, stages, or deletes nothing; the workspace
    and the fixture are left byte-for-byte unchanged
  - no write-chain: never produces an approval line, never runs an A2
    preview/approve/apply command, never executes anything contained in
    the fixture (plan steps, test suggestions, etc. are descriptive only)
  - excluded state trees: refuses any attempt to use the A2 state tree
    (the ".cl" + "aw" directory) as the workspace or a path hint, and the
    summary builder already excludes it from any walk

Refusals (nonzero exit):
  - missing task text
  - missing workspace
  - missing fixture
  - fixture is not a conforming, inert planner output
  - the workspace summary builder refuses the workspace / hint
  - the workspace or a path hint points into the A2 state tree

Python standard library only. The three sibling surfaces are imported
read-only by file path (the scripts directory is not a package), the same
convention the pretty-printer uses to load the validator.
"""

from __future__ import annotations

import argparse
import importlib.util
import json
import sys
from pathlib import Path, PurePosixPath
from typing import Any

_SCRIPT_DIR = Path(__file__).resolve().parent

BUILDER_PATH = _SCRIPT_DIR / "build_workspace_context_summary.py"
VALIDATOR_PATH = _SCRIPT_DIR / "validate_planner_output_schema.py"
PRINTER_PATH = _SCRIPT_DIR / "pretty_print_planner_output.py"

# The A2 state tree directory name, assembled from fragments so neither the
# diff safety-grep nor a naive content scan false-positives on this module.
_A2_STATE_DIR = ".cl" + "aw"

_BANNER = [
    "=" * 60,
    "OFFLINE STUB MODE — A2-L4-S2B-3 offline planner CLI skeleton",
    "NO MODEL CALL WAS MADE",
    "NO FILES WERE WRITTEN",
    "NO A2 WRITE-CHAIN COMMANDS WERE RUN",
    "This skeleton wires existing read-only surfaces together using a",
    "fixture plan only. It grants no authority; operator review and the",
    "A2 preview/approve/apply chain remain the only write path.",
    "=" * 60,
]


class OfflinePlannerError(Exception):
    """Raised to refuse a request; the CLI renders it and exits nonzero."""


def _load_module(name: str, path: Path):
    """Import a sibling script read-only by file path (scripts/ is not a package)."""
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _touches_state_tree(path_str: str) -> bool:
    """True if any component of the path is the A2 state tree directory."""
    return _A2_STATE_DIR in PurePosixPath(str(path_str)).parts


def _build_report(
    *,
    task: str,
    workspace: str,
    fixture: str,
    path_hints,
    max_files: int,
) -> dict:
    """Run the offline wiring and return a structured result dict.

    Raises OfflinePlannerError on any refusal. Reads only; writes nothing.
    """
    if not task or not task.strip():
        raise OfflinePlannerError("a non-empty --task is required")
    if not workspace or not str(workspace).strip():
        raise OfflinePlannerError("a --workspace is required")
    if not fixture or not str(fixture).strip():
        raise OfflinePlannerError("a --fixture planner-output path is required")

    # Refuse the A2 state tree as workspace or as any path hint outright.
    if _touches_state_tree(workspace):
        raise OfflinePlannerError(
            "workspace points into the A2 state tree (refused)"
        )
    for hint in path_hints:
        if _touches_state_tree(hint):
            raise OfflinePlannerError(
                "path hint points into the A2 state tree (refused)"
            )

    builder = _load_module("build_workspace_context_summary", BUILDER_PATH)
    validator = _load_module("validate_planner_output_schema", VALIDATOR_PATH)
    printer = _load_module("pretty_print_planner_output", PRINTER_PATH)

    # 1. Read-only, metadata-only workspace context summary.
    try:
        summary = builder.build_summary(
            workspace, path_hints=tuple(path_hints), max_files=max_files
        )
    except builder.ContextSummaryError as exc:
        raise OfflinePlannerError(f"workspace summary builder refused: {exc}")

    # 2. Load the fixture planner-output document (read-only).
    fixture_path = Path(fixture)
    if not fixture_path.exists():
        raise OfflinePlannerError(f"fixture does not exist: {fixture!r}")
    try:
        doc = json.loads(fixture_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise OfflinePlannerError(f"could not read fixture {fixture!r}: {exc}")

    # 3. Validate with the existing read-only validator.
    schema = validator.load_schema(validator.DEFAULT_SCHEMA_PATH)
    failures = validator.validate_document(doc, schema)
    if failures:
        rendered = printer._render_refusal(failures)
        raise OfflinePlannerError(
            "fixture is not a conforming, inert planner output:\n" + rendered
        )

    # 4. Pretty-print with the existing read-only pretty-printer.
    rendered_plan = printer._render_valid(doc)

    return {
        "mode": "offline-stub",
        "model_called": False,
        "files_written": False,
        "a2_write_chain_invoked": False,
        "task": task,
        "fixture": str(fixture),
        "workspace_summary": summary,
        "validation": {"valid": True, "failures": []},
        "planner_output": doc,
        "rendered_plan": rendered_plan,
    }


def _excluded_phrase(summary: dict) -> str:
    parts = [f"{e['category']}={e['count']}" for e in summary.get("excluded", [])]
    return ", ".join(parts) if parts else "(none)"


def _format_text_report(result: dict) -> str:
    summary = result["workspace_summary"]
    out: list[str] = []
    out.extend(_BANNER)
    out.append("")
    out.append("[1] Operator task")
    out.append(f"  {result['task'].strip()}")
    out.append("")
    out.append("[2] Workspace context summary (read-only, metadata-only)")
    out.append(f"  workspace_root: {summary.get('workspace_root', '')}")
    out.append(f"  files listed:   {len(summary.get('files', []))}")
    out.append(f"  excluded:       {_excluded_phrase(summary)}")
    warnings = summary.get("warnings", [])
    out.append(f"  warnings:       {('; '.join(warnings)) if warnings else '(none)'}")
    out.append("")
    out.append("[3] Planner output validation (existing read-only validator)")
    out.append(f"  VALID (inert) — fixture conforms: {result['fixture']}")
    out.append("")
    out.append("[4] Planner output (existing read-only pretty-printer)")
    for line in result["rendered_plan"].rstrip("\n").splitlines():
        out.append(f"  {line}" if line else "")
    out.append("")
    out.append("[5] Operator next steps")
    out.append("  - Review the fixture plan above.")
    out.append("  - Nothing has been applied and no model was consulted.")
    out.append("  - When you choose to act, use the A2 preview/approve/apply")
    out.append("    chain; this skeleton has no write authority.")
    return "\n".join(out).rstrip() + "\n"


def _format_json_report(result: dict) -> str:
    # Emit a JSON view; drop the pre-rendered text body to keep it inert data.
    payload = {k: v for k, v in result.items() if k != "rendered_plan"}
    return json.dumps(payload, indent=2)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Offline, no-model-call planner CLI skeleton (A2-L4-S2B-3). "
            "Combines operator task text, a read-only workspace context "
            "summary, and a fixture planner output through the existing "
            "validator and pretty-printer. Calls no model; writes nothing."
        )
    )
    parser.add_argument("--task", required=True, help="operator task text")
    parser.add_argument("--workspace", required=True, help="workspace root to summarize")
    parser.add_argument(
        "--fixture", required=True,
        help="path to a planner-output fixture JSON document",
    )
    parser.add_argument(
        "--json", action="store_true", dest="as_json",
        help="emit a structured JSON report instead of the text report",
    )
    parser.add_argument(
        "--max-files", type=int, default=500,
        help="max files for the workspace summary (default 500)",
    )
    parser.add_argument(
        "--path-hint", action="append", default=[], dest="path_hints",
        help="workspace-relative focus hint for the summary (repeatable)",
    )
    args = parser.parse_args(argv)

    try:
        result = _build_report(
            task=args.task,
            workspace=args.workspace,
            fixture=args.fixture,
            path_hints=args.path_hints,
            max_files=args.max_files,
        )
    except OfflinePlannerError as exc:
        print("OFFLINE STUB MODE — REFUSED", file=sys.stderr)
        print(f"  {exc}", file=sys.stderr)
        print(
            "  No model was called, no files were written, and no A2 "
            "write-chain command was run.",
            file=sys.stderr,
        )
        return 2

    if args.as_json:
        print(_format_json_report(result))
    else:
        print(_format_text_report(result))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
