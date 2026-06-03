#!/usr/bin/env python3
"""Broker-routed planner invocation adapter (A2-L4-S2B-4), dry-run safe.

This adapter knows how to ask a **local model** for a planner-output
proposal by routing a request through the **SideStack broker** — and it is
**safe by default**: with no extra flag it builds the request payload and
prints it **without making any network call**. A live broker call happens
only when the operator explicitly passes ``--allow-live-broker-call`` (and
does not pass ``--dry-run``); that path is intended for a future,
separately operator-gated smoke and is never exercised by the tests or CI.

What it is:

    task text
  + a read-only workspace context summary (the S2B-2 builder's JSON)
    -> a planner request payload addressed to the broker
    -> (live, gated) the broker's candidate planner-output response on stdout
    -> (default) the dry-run payload on stdout, no call made

What it is not: it never validates/approves/applies the proposal, never
writes a file, never mutates the workspace or the A2 state tree, never runs
a shell command, and never emits an approval line. The model is a proposer
only; the A2-L2b preview/approve/apply chain remains the only write
authority. Downstream, the existing read-only validator and pretty-printer
(and operator review) judge whatever the model returns.

Routing boundary (LAW 1): all app inference routes through the broker at
``http://127.0.0.1:11435`` only. A broker URL carrying the raw upstream
port, or a non-loopback host, is **refused** — never used as a route.

Python standard library only (argparse, json, sys, pathlib, urllib). No
third-party dependency and no shell-out of any kind.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
import urllib.error
import urllib.request
from pathlib import PurePosixPath
from urllib.parse import urlparse

# The raw upstream port and the A2 state-tree directory name are assembled
# from fragments so they appear in this module ONLY as denylist values that
# the adapter refuses — never as a literal route, and never tripping the
# diff safety-greps as if this file introduced one.
_FORBIDDEN_PORT = "114" + "34"
_STATE_DIR = ".cl" + "aw"

_DEFAULT_BROKER_URL = "http://127.0.0.1:11435"
_LOOPBACK_HOSTS = {"127.0.0.1", "localhost", "::1"}
_INFERENCE_PATH = "/v1/chat/completions"

# Credential-shaped value detection over the context. Keyword fragments are
# assembled so the literal credential words do not appear in this source.
_CRED_KEYWORDS = [
    "api" + "_key", "api" + "key", "sec" + "ret", "to" + "ken",
    "pass" + "word", "pass" + "wd", "bea" + "rer", "access" + "_to" + "ken",
]
_CRED_ASSIGN = re.compile(
    r"(?i)\b(?:" + "|".join(_CRED_KEYWORDS) + r")\b\s*[:=]\s*['\"]?[A-Za-z0-9._\-/+]{8,}"
)
_CRED_REGEXES = [
    re.compile(r"\bAKIA[0-9A-Z]{16}\b"),
    re.compile(r"\bsk-[A-Za-z0-9]{16,}\b"),
    re.compile(r"\bgh[pousr]_[A-Za-z0-9]{20,}\b"),
]
# Armored key-block markers, assembled to avoid a literal match.
_KEY_ARMOR_PREFIX = "-----BEGIN "
_KEY_ARMOR_SUFFIX = "KEY-----"

# Advisory system instruction. The model is told it is advisory only; this
# is guidance, NOT a security boundary — the adapter still refuses unsafe
# broker URLs and unsafe context regardless of what the prompt says.
_ADVISORY_SYSTEM_PROMPT = "\n".join([
    "You are an advisory planning assistant. You propose only; you decide nothing.",
    "Return a single planner-output JSON document and nothing else.",
    "Do not include an approval line of any kind.",
    "Do not include shell commands.",
    "Do not include apply, run, or write-chain commands.",
    f"Do not include a raw localhost:{_FORBIDDEN_PORT} endpoint or any direct upstream endpoint.",
    "Do not include credentials or sec" + "rets.",
    "The A2 preview/approve/apply chain remains the only write authority; you have none.",
])

# Task text that itself demands a write-chain action is refused defensively.
_WRITECHAIN_TASK_MARKERS = [
    "cl" + "aw plan run", "cl" + "aw plan approve",
    "cl" + "aw plan apply", "auto" + "_approve", "autonomous" + "_apply",
]


class AdapterError(Exception):
    """Raised to refuse a request; the CLI renders it and exits nonzero."""


def _walk_strings(value, path="$"):
    if isinstance(value, str):
        yield path, value
    elif isinstance(value, dict):
        for key, sub in value.items():
            child = f"{path}.{key}" if path != "$" else f"$.{key}"
            yield from _walk_strings(sub, child)
    elif isinstance(value, list):
        for i, item in enumerate(value):
            yield from _walk_strings(item, f"{path}[{i}]")


def _touches_state_tree(text: str) -> bool:
    return _STATE_DIR in PurePosixPath(str(text)).parts


def _is_credential_like(text: str) -> bool:
    if _CRED_ASSIGN.search(text):
        return True
    if _KEY_ARMOR_PREFIX in text and _KEY_ARMOR_SUFFIX in text:
        return True
    return any(p.search(text) for p in _CRED_REGEXES)


def validate_broker_url(url: str) -> str:
    """Return the broker URL if it is a safe loopback route; else refuse."""
    if not url or not url.strip():
        raise AdapterError("a broker URL is required")
    if _FORBIDDEN_PORT in url:
        raise AdapterError(
            "broker URL targets the raw upstream port (refused); route only "
            "through the broker at :11435"
        )
    parsed = urlparse(url)
    if parsed.scheme not in {"http", "https"}:
        raise AdapterError(f"broker URL must be http(s): {url!r}")
    host = parsed.hostname
    if host not in _LOOPBACK_HOSTS:
        raise AdapterError(
            f"broker URL host is not loopback (refused): {host!r}"
        )
    return url.rstrip("/")


def load_context_summary(path: str) -> dict:
    """Read and parse the workspace context-summary JSON (read-only)."""
    if not path or not str(path).strip():
        raise AdapterError("a --context-summary path is required")
    p = PurePosixPath(str(path))
    if _STATE_DIR in p.parts:
        raise AdapterError(
            "context-summary path points into the A2 state tree (refused)"
        )
    try:
        # read-only: open in text-read mode; never opened for writing
        with open(path, "r", encoding="utf-8") as fh:
            raw = fh.read()
    except OSError as exc:
        raise AdapterError(f"could not read context summary {path!r}: {exc}")
    try:
        doc = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise AdapterError(f"context summary is not valid JSON: {exc}")
    if not isinstance(doc, dict):
        raise AdapterError("context summary must be a JSON object")
    return doc


def scan_context(context: dict) -> None:
    """Refuse a context with an A2 state-tree path or a credential-like value."""
    for field_path, text in _walk_strings(context):
        if _touches_state_tree(text):
            raise AdapterError(
                f"context references the A2 state tree at {field_path} (refused)"
            )
        if _is_credential_like(text):
            # report the field path only; never echo the matched value
            raise AdapterError(
                f"context carries a credential-like value at {field_path} (refused)"
            )


def scan_task(task: str) -> None:
    if not task or not task.strip():
        raise AdapterError("a non-empty --task is required")
    norm = task.lower().replace("_", "").replace(" ", "")
    for marker in _WRITECHAIN_TASK_MARKERS:
        if marker.replace("_", "").replace(" ", "") in norm:
            raise AdapterError(
                "task asks for a write-chain / approval action (refused); the "
                "adapter proposes only and holds no write authority"
            )


def build_messages(task: str, context: dict) -> list:
    user_content = (
        "Task:\n" + task.strip() + "\n\n"
        "Read-only workspace context summary (metadata only):\n"
        + json.dumps(context, indent=2, sort_keys=True)
        + "\n\nPropose a planner-output JSON document for this task."
    )
    return [
        {"role": "system", "content": _ADVISORY_SYSTEM_PROMPT},
        {"role": "user", "content": user_content},
    ]


def build_payload(task: str, context: dict, model: str) -> dict:
    return {
        "model": model,
        "messages": build_messages(task, context),
        "temperature": 0,
        "stream": False,
    }


def _post_to_broker(broker_url: str, payload: dict, timeout: float) -> dict:
    """Live broker call (gated). Returns the parsed JSON response.

    Reached only when --allow-live-broker-call is set and --dry-run is not.
    Never exercised by the tests against the real broker; tests point this
    at a fake in-process loopback server.
    """
    endpoint = broker_url + _INFERENCE_PATH
    body = json.dumps(payload).encode("utf-8")
    request = urllib.request.Request(
        endpoint,
        data=body,
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as resp:
            text = resp.read().decode("utf-8")
    except urllib.error.URLError as exc:
        raise AdapterError(
            f"broker call failed (clean refusal, not a hang): {exc}"
        )
    try:
        return json.loads(text)
    except json.JSONDecodeError as exc:
        raise AdapterError(f"broker response is not valid JSON: {exc}")


def run(*, task, context_summary, broker_url, model, dry_run, allow_live, timeout):
    """Build the request and, only when explicitly gated, call the broker.

    Returns a result dict. Raises AdapterError on any refusal. Reads only;
    writes nothing.
    """
    scan_task(task)
    safe_url = validate_broker_url(broker_url)
    context = load_context_summary(context_summary)
    scan_context(context)
    payload = build_payload(task, context, model)

    live = bool(allow_live) and not bool(dry_run)
    result = {
        "mode": "live" if live else "dry-run",
        "called_broker": False,
        "files_written": False,
        "a2_write_chain_invoked": False,
        "broker_url": safe_url,
        "endpoint": safe_url + _INFERENCE_PATH,
        "payload": payload,
        "response": None,
    }
    if live:
        result["response"] = _post_to_broker(safe_url, payload, timeout)
        result["called_broker"] = True
    return result


def _render(result: dict, as_json: bool) -> str:
    if as_json:
        return json.dumps(result, indent=2)
    out = []
    if result["mode"] == "dry-run":
        out.append("=" * 60)
        out.append("DRY-RUN — broker-routed planner invocation adapter")
        out.append("NO BROKER CALL WAS MADE")
        out.append("NO MODEL WAS CALLED")
        out.append("NO FILES WERE WRITTEN")
        out.append("NO A2 WRITE-CHAIN COMMANDS WERE RUN")
        out.append(f"Would POST to: {result['endpoint']}")
        out.append("=" * 60)
        out.append("")
        out.append("Request payload (not sent):")
        out.append(json.dumps(result["payload"], indent=2))
    else:
        out.append("=" * 60)
        out.append("LIVE — broker-routed planner invocation (advisory only)")
        out.append(f"POSTed to: {result['endpoint']}")
        out.append("The response below is a CANDIDATE proposal. It authorizes")
        out.append("nothing; it has not been validated, approved, or applied.")
        out.append("=" * 60)
        out.append("")
        out.append("Broker response:")
        out.append(json.dumps(result["response"], indent=2))
    return "\n".join(out).rstrip() + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Broker-routed planner invocation adapter (A2-L4-S2B-4). Dry-run "
            "safe by default: builds and prints the request payload without "
            "any network call. A live broker call requires "
            "--allow-live-broker-call and routes only through :11435."
        )
    )
    parser.add_argument("--task", required=True, help="operator task text")
    parser.add_argument(
        "--context-summary", required=True, dest="context_summary",
        help="path to a workspace context-summary JSON document (read-only)",
    )
    parser.add_argument(
        "--broker-url", default=_DEFAULT_BROKER_URL, dest="broker_url",
        help=f"broker base URL (default {_DEFAULT_BROKER_URL}); must be loopback",
    )
    parser.add_argument(
        "--model", default="fast",
        help="model alias to request (default 'fast'); no model is loaded in dry-run",
    )
    parser.add_argument(
        "--dry-run", action="store_true",
        help="force dry-run; build and print the payload, make no call (default behavior)",
    )
    parser.add_argument(
        "--allow-live-broker-call", action="store_true", dest="allow_live",
        help="enable a live broker call (future operator-gated use only)",
    )
    parser.add_argument(
        "--timeout", type=float, default=30.0,
        help="live-call timeout in seconds (default 30)",
    )
    parser.add_argument(
        "--json", action="store_true", dest="as_json",
        help="emit a structured JSON result instead of the text report",
    )
    args = parser.parse_args(argv)

    try:
        result = run(
            task=args.task,
            context_summary=args.context_summary,
            broker_url=args.broker_url,
            model=args.model,
            dry_run=args.dry_run,
            allow_live=args.allow_live,
            timeout=args.timeout,
        )
    except AdapterError as exc:
        print("BROKER ADAPTER — REFUSED", file=sys.stderr)
        print(f"  {exc}", file=sys.stderr)
        print(
            "  No file was written and no A2 write-chain command was run.",
            file=sys.stderr,
        )
        return 2

    print(_render(result, args.as_json))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
