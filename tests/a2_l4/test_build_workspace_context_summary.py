"""Tests for the read-only A2-L4-S2B-2 workspace context-summary builder.

Exercises the inert, metadata-only summary and the boundaries pinned by
the S2B scope card: read-only, no writes, no inspection of .git/.claw,
excluded noisy/credential-like paths, and refusal of out-of-workspace or
state-tree path hints.

Stdlib unittest only (matching the sibling A2-L4 test modules); no new
dependency. The module is loaded by path and its functions are called
directly (this test starts no external process), so the diff carries no
runtime/network tokens. Run with:
  python3 -m unittest tests.a2_l4.test_build_workspace_context_summary
"""

from __future__ import annotations

import contextlib
import importlib.util
import io
import json
import os
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

REPO_ROOT = Path(__file__).resolve().parents[2]
BUILDER_PATH = REPO_ROOT / "scripts" / "build_workspace_context_summary.py"


def _load_builder():
    spec = importlib.util.spec_from_file_location(
        "build_workspace_context_summary", BUILDER_PATH
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


builder = _load_builder()


def _write(root: Path, rel: str, content: str = "x") -> None:
    """Test-fixture helper: materialise a file under a tempdir workspace."""
    target = root / rel
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_text(content, encoding="utf-8")


def _make_workspace(d: str) -> Path:
    root = Path(d)
    _write(root, "README.md", "# hi")
    _write(root, "src/main.py", "print(1)")
    _write(root, "src/util/helpers.py", "pass")
    _write(root, "assets/logo.png", "binaryish")
    # noisy / excluded trees
    _write(root, ".git/config", "[core]")
    _write(root, ".claw/state.json", "{}")
    _write(root, "node_modules/pkg/index.js", "x")
    _write(root, "target/debug/app", "x")
    _write(root, "__pycache__/cache.pyc", "x")
    # credential-like filenames (names only; placeholder bodies)
    _write(root, ".env", "PLACEHOLDER=not-real")
    _write(root, "deploy.pem", "PLACEHOLDER")
    _write(root, "server.key", "PLACEHOLDER")
    return root


def _relpaths(summary: dict) -> list[str]:
    return [f["relative_path"] for f in summary["files"]]


class SummaryShapeTests(unittest.TestCase):
    def test_valid_small_workspace_emits_summary(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            summary = builder.build_summary(d)
            self.assertEqual(summary["schema_version"], builder.SCHEMA_VERSION)
            self.assertIn("files", summary)
            self.assertIn("excluded", summary)
            rels = _relpaths(summary)
            self.assertIn("README.md", rels)
            self.assertIn("src/main.py", rels)
            self.assertIn("src/util/helpers.py", rels)

    def test_files_sorted_deterministically(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            rels = _relpaths(builder.build_summary(d))
            self.assertEqual(rels, sorted(rels))
            # two builds yield identical ordering
            self.assertEqual(rels, _relpaths(builder.build_summary(d)))

    def test_metadata_only_no_content_field(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            summary = builder.build_summary(d)
            for entry in summary["files"]:
                self.assertNotIn("content", entry)
                self.assertIn("size_bytes", entry)
                self.assertIn("extension", entry)

    def test_text_and_binary_candidate_flags(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            by_path = {f["relative_path"]: f for f in builder.build_summary(d)["files"]}
            self.assertTrue(by_path["src/main.py"]["is_text_candidate"])
            self.assertFalse(by_path["src/main.py"]["is_binary_candidate"])
            self.assertTrue(by_path["assets/logo.png"]["is_binary_candidate"])

    def test_json_output_is_parseable(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                rc = builder.main([d])
            self.assertEqual(rc, 0)
            parsed = json.loads(buf.getvalue())
            self.assertEqual(parsed["schema_version"], builder.SCHEMA_VERSION)


class PathHintTests(unittest.TestCase):
    def test_path_hint_marks_matching_files(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            summary = builder.build_summary(d, path_hints=["src/util"])
            matched = [f for f in summary["files"] if f["matched_path_hint"]]
            self.assertTrue(matched)
            for f in matched:
                self.assertTrue(f["relative_path"].startswith("src/util"))
                self.assertEqual(f["matched_path_hint"], "src/util")

    def test_absolute_path_hint_refused(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            with self.assertRaises(builder.ContextSummaryError):
                builder.build_summary(d, path_hints=["/etc/passwd"])

    def test_traversal_path_hint_refused(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            with self.assertRaises(builder.ContextSummaryError):
                builder.build_summary(d, path_hints=["../escape"])

    def test_state_tree_path_hint_refused(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            with self.assertRaises(builder.ContextSummaryError):
                builder.build_summary(d, path_hints=[".claw/state.json"])
            with self.assertRaises(builder.ContextSummaryError):
                builder.build_summary(d, path_hints=[".git/config"])


class ExclusionTests(unittest.TestCase):
    def setUp(self) -> None:
        self._d = TemporaryDirectory()
        _make_workspace(self._d.name)
        self.summary = builder.build_summary(self._d.name)
        self.rels = _relpaths(self.summary)

    def tearDown(self) -> None:
        self._d.cleanup()

    def test_git_excluded(self) -> None:
        self.assertFalse(any(r.startswith(".git/") for r in self.rels))

    def test_claw_excluded(self) -> None:
        self.assertFalse(any(r.startswith(".claw/") for r in self.rels))

    def test_node_modules_excluded(self) -> None:
        self.assertFalse(any(r.startswith("node_modules/") for r in self.rels))

    def test_build_artifacts_excluded(self) -> None:
        self.assertFalse(any(r.startswith("target/") for r in self.rels))
        self.assertFalse(any(r.startswith("__pycache__/") for r in self.rels))

    def test_credential_like_filenames_excluded(self) -> None:
        self.assertNotIn(".env", self.rels)
        self.assertNotIn("deploy.pem", self.rels)
        self.assertNotIn("server.key", self.rels)

    def test_excluded_reported_as_category_count_reason_only(self) -> None:
        for entry in self.summary["excluded"]:
            self.assertIn("category", entry)
            self.assertIn("count", entry)
            self.assertIn("reason", entry)
            # category/count/reason only — never a list of concrete paths
            self.assertNotIn("paths", entry)
            self.assertNotIn("relative_path", entry)


class RefusalTests(unittest.TestCase):
    def test_nonexistent_workspace_refused(self) -> None:
        with TemporaryDirectory() as d:
            missing = str(Path(d) / "does-not-exist")
            with self.assertRaises(builder.ContextSummaryError):
                builder.build_summary(missing)

    def test_workspace_not_a_directory_refused(self) -> None:
        with TemporaryDirectory() as d:
            f = Path(d) / "afile"
            f.write_text("x", encoding="utf-8")
            with self.assertRaises(builder.ContextSummaryError):
                builder.build_summary(str(f))

    def test_main_returns_nonzero_and_json_error_on_refusal(self) -> None:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = builder.main(["/no/such/workspace/here"])
        self.assertNotEqual(rc, 0)
        payload = json.loads(err.getvalue())
        self.assertEqual(payload["error"], "refused")


class ReadOnlyBoundaryTests(unittest.TestCase):
    def test_no_file_written_into_workspace(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            before = sorted(p for p in Path(d).rglob("*"))
            builder.build_summary(d)
            after = sorted(p for p in Path(d).rglob("*"))
            self.assertEqual(before, after, "builder must not create or remove files")

    def test_no_claw_content_in_output(self) -> None:
        with TemporaryDirectory() as d:
            _make_workspace(d)
            blob = json.dumps(builder.build_summary(d))
            # the .claw file body must never appear anywhere in the output
            self.assertNotIn("state.json", blob)

    def test_no_runtime_or_network_imports_present(self) -> None:
        # Forbidden module names are assembled from fragments so the diff
        # safety-grep does not false-positive on this guard test itself.
        forbidden = [
            "sub" + "process",
            "soc" + "ket",
            "re" + "quests",
            "ht" + "tpx",
            "urllib." + "request",
            "shu" + "til",
        ]
        src = BUILDER_PATH.read_text(encoding="utf-8")
        for token in forbidden:
            self.assertNotIn(token, src, f"builder must not reference {token}")


if __name__ == "__main__":
    unittest.main()
