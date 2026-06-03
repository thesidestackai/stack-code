#!/usr/bin/env python3
"""Read-only validator for the A2-L4 planner-output contract (S2A-5).

This is an operator helper, not an authority surface. It reads the
planner-output JSON Schema (schemas/a2-l4/planner-output.schema.json) and
a candidate document, then reports whether the document is well-formed
and inert. A pass grants nothing: the A2-L2b preview/approve/apply chain
remains the only write authority.

The validator is strictly read-only. It:
  - never writes, edits, or stages any file
  - never produces an approval line or apply artifact
  - never calls a model, the broker, or Ollama (no inference)
  - refuses (never coerces, strips-and-accepts, or partially accepts) any
    non-conforming document

It performs two layers of checks (S2A-4 scope card):
  1. schema conformance — a focused, stdlib-only structural check covering
     exactly the JSON Schema keywords this contract uses (type, required,
     properties, additionalProperties:false, const, minLength, minItems,
     items, $ref/$defs, anyOf, not).
  2. semantic checks the schema alone cannot express —
     workspace-relative/no-path-escape paths, :11434 value refusal, and
     secret/token/key pattern refusal (reported by field path, never by
     value).

Stdlib only (json, re, pathlib, argparse); no third-party dependency. If a
dependency were ever required, that is a STOP and an operator decision.

Exit codes:
  0  valid and inert (all checks pass)
  1  invalid (schema or semantic-check failure) — refused
  2  usage / IO error (could not read input or schema)
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path, PurePosixPath
from typing import Any

DEFAULT_SCHEMA_PATH = (
    Path(__file__).resolve().parents[1]
    / "schemas"
    / "a2-l4"
    / "planner-output.schema.json"
)

# Fields whose values are file paths and must be workspace-relative with no
# path escape (the schema only types them as strings).
PATH_STRING_FIELD = "workspace_root"
PATH_ARRAY_FIELD = "candidate_files"

# Secret/token/key patterns. These match credential-shaped values, not the
# mere words "secret"/"token" appearing in descriptive prose.
SECRET_PATTERNS = [
    (re.compile(r"-----BEGIN (?:RSA |OPENSSH |EC |DSA |PGP )?PRIVATE KEY-----"),
     "private key block"),
    (re.compile(r"\bAKIA[0-9A-Z]{16}\b"), "aws access key id"),
    (re.compile(r"\bsk-[A-Za-z0-9]{16,}\b"), "api key token"),
    (re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"), "github token"),
    (re.compile(
        r"(?i)\b(?:api[_-]?key|secret|token|password|passwd|"
        r"access[_-]?token|bearer)\b\s*[:=]\s*['\"]?[A-Za-z0-9._\-/+]{8,}"),
     "secret assignment"),
]

_PORT_11434 = "11434"


# --------------------------------------------------------------------------
# Schema loading
# --------------------------------------------------------------------------
def load_schema(path: Path) -> dict:
    """Load the JSON Schema document. Raises on IO/parse error (handled by CLI)."""
    return json.loads(Path(path).read_text(encoding="utf-8"))


# --------------------------------------------------------------------------
# Focused structural validation
# --------------------------------------------------------------------------
def _resolve_ref(ref: str, root: dict) -> dict:
    if not ref.startswith("#/"):
        raise ValueError(f"unsupported $ref: {ref}")
    node: Any = root
    for part in ref[2:].split("/"):
        node = node[part]
    return node


_TYPE_PY = {
    "object": dict,
    "string": str,
    "array": list,
    "boolean": bool,
}


def _type_ok(value: Any, type_name: str) -> bool:
    if type_name == "boolean":
        return isinstance(value, bool)
    py = _TYPE_PY.get(type_name)
    if py is None:
        return True  # unknown type keyword: do not constrain
    # bool is a subclass of int but we only use object/string/array/boolean
    return isinstance(value, py)


def _collect_errors(value: Any, schema: dict, path: str, root: dict) -> list[str]:
    """Collect structural validation errors for `value` against `schema`."""
    errors: list[str] = []

    if "$ref" in schema:
        errors.extend(_collect_errors(value, _resolve_ref(schema["$ref"], root), path, root))
        return errors

    if "const" in schema and value != schema["const"]:
        errors.append(f"{path}: must equal {schema['const']!r}")

    if "type" in schema and not _type_ok(value, schema["type"]):
        errors.append(f"{path}: expected type {schema['type']}")
        return errors  # further keyword checks assume the type matched

    if isinstance(value, str) and "minLength" in schema and len(value) < schema["minLength"]:
        errors.append(f"{path}: shorter than minLength {schema['minLength']}")

    if isinstance(value, list):
        if "minItems" in schema and len(value) < schema["minItems"]:
            errors.append(f"{path}: fewer than minItems {schema['minItems']}")
        if "items" in schema:
            for i, item in enumerate(value):
                errors.extend(_collect_errors(item, schema["items"], f"{path}[{i}]", root))

    if isinstance(value, dict):
        for req in schema.get("required", []):
            if req not in value:
                errors.append(f"{path}: missing required property '{req}'")
        props = schema.get("properties", {})
        if schema.get("additionalProperties", True) is False:
            for key in value:
                if key not in props:
                    errors.append(
                        f"{path}: unknown property '{key}' not allowed "
                        f"(additionalProperties:false)"
                    )
        for key, subschema in props.items():
            if key in value:
                child = f"{path}.{key}" if path != "$" else f"$.{key}"
                errors.extend(_collect_errors(value[key], subschema, child, root))

    if "anyOf" in schema:
        if all(_collect_errors(value, branch, path, root) for branch in schema["anyOf"]):
            errors.append(f"{path}: did not match any allowed variant")

    if "not" in schema:
        if not _collect_errors(value, schema["not"], path, root):
            errors.append(f"{path}: matched a forbidden shape (not)")

    return errors


# --------------------------------------------------------------------------
# Semantic checks the schema cannot express (S2A-4 §12)
# --------------------------------------------------------------------------
def _path_escapes(p: str) -> bool:
    if not isinstance(p, str) or not p:
        return False
    if p.startswith("/") or p.startswith("~"):
        return True
    pp = PurePosixPath(p)
    if pp.is_absolute():
        return True
    return any(part == ".." for part in pp.parts)


def _walk_strings(value: Any, path: str):
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for key, sub in value.items():
            child = f"{path}.{key}" if path != "$" else f"$.{key}"
            yield from _walk_strings(sub, child)
    elif isinstance(value, list):
        for i, item in enumerate(value):
            yield from _walk_strings(item, f"{path}[{i}]")


def _semantic_errors(doc: Any) -> list[str]:
    errors: list[str] = []
    if not isinstance(doc, dict):
        return errors

    # Path-escape checks on path-typed fields.
    ws = doc.get(PATH_STRING_FIELD)
    if isinstance(ws, str) and _path_escapes(ws):
        errors.append(f"$.{PATH_STRING_FIELD}: path is not workspace-relative (escape refused)")
    cand = doc.get(PATH_ARRAY_FIELD)
    if isinstance(cand, list):
        for i, entry in enumerate(cand):
            if isinstance(entry, str) and _path_escapes(entry):
                errors.append(
                    f"$.{PATH_ARRAY_FIELD}[{i}]: path is not workspace-relative (escape refused)"
                )

    # :11434 value refusal + secret/token/key refusal across all string values.
    for field_path, text in _walk_strings(doc, "$"):
        if _PORT_11434 in text:
            errors.append(f"{field_path}: forbidden :11434 endpoint reference refused")
        for pattern, label in SECRET_PATTERNS:
            if pattern.search(text):
                errors.append(f"{field_path}: secret-like value refused ({label})")
                break  # report once per field; never echo the matched value
    return errors


# --------------------------------------------------------------------------
# Public API
# --------------------------------------------------------------------------
def validate_document(doc: Any, schema: dict) -> list[str]:
    """Return a list of failure messages; empty means valid and inert."""
    failures = _collect_errors(doc, schema, "$", schema)
    failures.extend(_semantic_errors(doc))
    return failures


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------
def _self_test(schema_path: Path) -> int:
    schema = load_schema(schema_path)
    fixture_dir = schema_path.parent / "fixtures" / "planner-output"
    fixtures = sorted(fixture_dir.glob("*.json"))
    if not fixtures:
        print(f"self-test: no fixtures found under {fixture_dir}", file=sys.stderr)
        return 2
    ok = True
    for fx in fixtures:
        doc = json.loads(fx.read_text(encoding="utf-8"))
        failures = validate_document(doc, schema)
        expect_valid = fx.name.startswith("valid-")
        accepted = not failures
        correct = accepted == expect_valid
        ok = ok and correct
        verdict = "accept" if accepted else "refuse"
        flag = "OK " if correct else "XX "
        print(f"{flag}{fx.name}: {verdict} (expected {'accept' if expect_valid else 'refuse'})")
    print("self-test: PASS" if ok else "self-test: FAIL")
    return 0 if ok else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Read-only validator for the A2-L4 planner-output contract."
    )
    parser.add_argument(
        "input", nargs="?",
        help="path to a planner-output JSON document to validate",
    )
    parser.add_argument(
        "--schema", default=str(DEFAULT_SCHEMA_PATH),
        help="path to the planner-output JSON Schema (default: %(default)s)",
    )
    parser.add_argument(
        "--self-test", action="store_true",
        help="validate the bundled fixture pack and report pass/fail",
    )
    args = parser.parse_args(argv)

    schema_path = Path(args.schema)
    try:
        schema = load_schema(schema_path)
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: could not read schema {schema_path}: {exc}", file=sys.stderr)
        return 2

    if args.self_test:
        return _self_test(schema_path)

    if not args.input:
        print("error: an input document path is required (or use --self-test)", file=sys.stderr)
        return 2

    input_path = Path(args.input)
    try:
        doc = json.loads(input_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        print(f"error: could not read input {input_path}: {exc}", file=sys.stderr)
        return 2

    failures = validate_document(doc, schema)
    if failures:
        print(f"REFUSED: {input_path} is not a conforming, inert planner output:")
        for f in failures:
            print(f"  - {f}")
        return 1
    print(f"VALID (inert): {input_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
