#!/usr/bin/env python3
"""Read-only workspace context-summary builder (A2-L4-S2B-2).

Builds an inert, metadata-only JSON summary of a workspace for future
local-model planning input. It is strictly read-only and advisory:
producing a summary grants nothing, and the A2-L2b preview/approve/apply
chain remains the only write authority (see the S2B scope card,
docs/a2-l4-s2b-readonly-planner-cli-scope-card.md).

Boundaries (S2B scope card):
  - read-only: walks the workspace reading directory entries and file
    metadata (size, name, extension) only; reads no file contents by
    default; creates, edits, or deletes nothing; the workspace is left
    unchanged
  - no execution: starts no external process and opens no network access;
    calls no model and no service
  - excludes noisy / state / credential-like paths (.git, .claw,
    node_modules, target, dist, build, out, __pycache__, .venv, and
    credential-like filenames), reporting them only as category + count +
    reason, never as concrete paths and never with any value
  - refuses: a missing workspace, a non-directory workspace, and a path
    hint that is absolute, escapes the workspace, or points into the
    .git / .claw state trees
  - output: inert JSON to stdout only; a structured JSON error to stderr
    with a nonzero exit on refusal

Python standard library only; no third-party dependency. File contents
are intentionally not read in this lane; content snippets are deferred to
a later scope card.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from datetime import datetime, timezone
from pathlib import Path, PurePosixPath

SCHEMA_VERSION = "a2-l4-workspace-context-summary.v1"

# Directory names pruned from the walk, mapped to a reporting category.
EXCLUDED_DIRS = {
    ".git": "vcs",
    ".claw": "a2_state",
    "node_modules": "dependencies",
    ".venv": "dependencies",
    "target": "build_artifacts",
    "dist": "build_artifacts",
    "build": "build_artifacts",
    "out": "build_artifacts",
    "__pycache__": "cache",
}

# Path-hint first components that are refused outright (state trees).
REFUSED_HINT_ROOTS = {".git", ".claw"}

# Exact credential-like filenames (names only; never inspected).
CREDENTIAL_FILENAMES = {".env", "id_rsa", "id_ed25519"}
# Credential-like filename suffixes.
CREDENTIAL_SUFFIXES = {".pem", ".key", ".p12", ".pfx"}

_TEXT_EXTS = {
    ".py", ".js", ".ts", ".tsx", ".jsx", ".rs", ".go", ".java", ".rb",
    ".c", ".h", ".cpp", ".hpp", ".cs", ".sh", ".bash", ".zsh",
    ".md", ".txt", ".rst", ".json", ".toml", ".yaml", ".yml",
    ".ini", ".cfg", ".conf", ".html", ".css", ".scss", ".xml", ".sql",
}
_BINARY_EXTS = {
    ".png", ".jpg", ".jpeg", ".gif", ".webp", ".ico", ".pdf", ".zip",
    ".gz", ".tar", ".tgz", ".bz2", ".7z", ".so", ".dylib", ".dll",
    ".bin", ".exe", ".class", ".o", ".a", ".wasm", ".mp4", ".mp3",
    ".woff", ".woff2", ".ttf", ".otf",
}

REASONS = {
    "vcs": "version-control metadata excluded",
    "a2_state": "A2 state tree excluded; not inspected",
    "dependencies": "dependency tree excluded",
    "build_artifacts": "build artifacts excluded",
    "cache": "cache directory excluded",
    "credential_like": "credential-like filename excluded; not inspected",
    "symlink": "symlink not followed",
}


class ContextSummaryError(Exception):
    """Raised to refuse a request; the CLI renders it as a JSON error."""


def _is_credential_like(name: str) -> bool:
    if name in CREDENTIAL_FILENAMES:
        return True
    if name.startswith(".env."):
        return True
    return PurePosixPath(name).suffix.lower() in CREDENTIAL_SUFFIXES


def _classify_exts(suffix: str) -> tuple[bool, bool]:
    s = suffix.lower()
    return (s in _TEXT_EXTS, s in _BINARY_EXTS)


def _normalize_hint(hint: str) -> str:
    p = PurePosixPath(hint)
    if p.is_absolute() or any(part == ".." for part in p.parts):
        raise ContextSummaryError(
            f"path hint is not workspace-relative (escape refused): {hint!r}"
        )
    if p.parts and p.parts[0] in REFUSED_HINT_ROOTS:
        raise ContextSummaryError(
            f"path hint points into an excluded state tree (refused): {hint!r}"
        )
    return p.as_posix()


def _hint_for(rel_path: str, hints: list[str]) -> str | None:
    for hint in hints:
        if rel_path == hint or rel_path.startswith(hint + "/"):
            return hint
    return None


def build_summary(
    workspace_root: str,
    *,
    path_hints=(),
    max_files: int = 500,
    max_bytes_per_file: int = 0,
    generated_at: str | None = None,
) -> dict:
    """Return an inert metadata-only summary dict. Raises ContextSummaryError on refusal."""
    if max_bytes_per_file and max_bytes_per_file > 0:
        raise ContextSummaryError(
            "file-content snippets are deferred to a later scope card; "
            "run with --max-bytes-per-file 0 (default, metadata-only)"
        )

    root = Path(workspace_root)
    if not root.exists():
        raise ContextSummaryError(f"workspace does not exist: {workspace_root!r}")
    if not root.is_dir():
        raise ContextSummaryError(f"workspace is not a directory: {workspace_root!r}")

    hints = [_normalize_hint(h) for h in path_hints]

    files: list[dict] = []
    excluded_counts: dict[str, int] = {}
    warnings: list[str] = []
    truncated = False

    def bump(category: str) -> None:
        excluded_counts[category] = excluded_counts.get(category, 0) + 1

    for current, dirnames, filenames in os.walk(root, topdown=True, followlinks=False):
        # Prune excluded and symlinked directories before descending.
        kept_dirs = []
        for dname in dirnames:
            full = os.path.join(current, dname)
            if dname in EXCLUDED_DIRS:
                bump(EXCLUDED_DIRS[dname])
                continue
            if os.path.islink(full):
                bump("symlink")
                continue
            kept_dirs.append(dname)
        dirnames[:] = sorted(kept_dirs)

        for fname in sorted(filenames):
            full = os.path.join(current, fname)
            if os.path.islink(full):
                bump("symlink")
                continue
            if _is_credential_like(fname):
                bump("credential_like")
                continue
            rel = os.path.relpath(full, root).replace(os.sep, "/")
            if len(files) >= max_files:
                truncated = True
                continue
            try:
                size = os.stat(full).st_size
            except OSError:
                warnings.append(f"could not stat a file under {rel.rsplit('/', 1)[0] or '.'}")
                continue
            suffix = PurePosixPath(fname).suffix.lower()
            is_text, is_binary = _classify_exts(suffix)
            files.append({
                "relative_path": rel,
                "kind": "file",
                "extension": suffix,
                "size_bytes": size,
                "is_text_candidate": is_text,
                "is_binary_candidate": is_binary,
                "matched_path_hint": _hint_for(rel, hints),
            })

    files.sort(key=lambda f: f["relative_path"])
    if truncated:
        warnings.append(f"file listing truncated at max-files={max_files}")

    excluded = [
        {"category": cat, "count": excluded_counts[cat], "reason": REASONS.get(cat, "excluded")}
        for cat in sorted(excluded_counts)
    ]

    return {
        "schema_version": SCHEMA_VERSION,
        "workspace_root": str(workspace_root),
        "generated_at": generated_at or datetime.now(timezone.utc).isoformat(),
        "summary_policy": {
            "max_files": max_files,
            "max_bytes_per_file": max_bytes_per_file,
            "content_included": False,
            "hashes_included": False,
            "note": "metadata-only; no file contents read; excluded paths "
                    "reported by category/count/reason only",
        },
        "path_hints": hints,
        "files": files,
        "excluded": excluded,
        "warnings": warnings,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Read-only, metadata-only workspace context-summary builder (A2-L4-S2B-2)."
    )
    parser.add_argument("workspace_root", help="path to the workspace root to summarize")
    parser.add_argument(
        "--path-hint", action="append", default=[], dest="path_hints",
        help="workspace-relative path to mark as a focus hint (repeatable)",
    )
    parser.add_argument("--max-files", type=int, default=500, help="max files to list (default 500)")
    parser.add_argument(
        "--max-bytes-per-file", type=int, default=0,
        help="bytes of file content to include (default 0 = metadata only; >0 is deferred)",
    )
    args = parser.parse_args(argv)

    try:
        summary = build_summary(
            args.workspace_root,
            path_hints=args.path_hints,
            max_files=args.max_files,
            max_bytes_per_file=args.max_bytes_per_file,
        )
    except ContextSummaryError as exc:
        print(
            json.dumps({
                "schema_version": SCHEMA_VERSION,
                "error": "refused",
                "reason": str(exc),
            }),
            file=sys.stderr,
        )
        return 2

    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
