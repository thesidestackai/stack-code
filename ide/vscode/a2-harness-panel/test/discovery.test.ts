import * as assert from "assert";
import {
  parseAuditWorkspace,
  parseFindArtifacts,
  parseHelpClawPath,
  selectCandidate,
  auditPathFor,
  ARTIFACT_NAMES,
} from "../src/discovery";

const AUDIT_PREVIEW_READY = `----------------------------------------------------------------
A2 audit-workspace (read-only; artifact/hash-based) — workspace: /disposable/wks
----------------------------------------------------------------
chain state: preview-ready
  present : preview-bundle.json  (/disposable/wks/.claw/preview-bundle.json)
  absent  : preview-generator-result.json
  absent  : approval-result.json
  absent  : apply-bundle.json
  absent  : apply-result.json

## next-step hint (state: preview-ready)
  Preview present, approval-result missing. Next: print-approval (REAL terminal required).
----------------------------------------------------------------
`;

const AUDIT_APPLIED_MATCH = `chain state: applied
  present : preview-bundle.json  (/disposable/wks/.claw/preview-bundle.json)
  present : preview-generator-result.json  (/disposable/wks/.claw/preview-generator-result.json)
  present : approval-result.json  (/disposable/wks/.claw/approval-result.json)
  present : apply-bundle.json  (/disposable/wks/.claw/apply-bundle.json)
  present : apply-result.json  (/disposable/wks/.claw/apply-result.json)

## target hash check
  target   : /disposable/wks/out.txt
  expected : abc123
  actual   : abc123
  MATCH — target is at the expected after_sha256.
`;

const AUDIT_MISMATCH = `chain state: applied
  present : apply-result.json  (/disposable/wks/.claw/apply-result.json)

## target hash check
  MISMATCH — target hash does not equal the expected after_sha256.
`;

const AUDIT_NOT_STARTED = `chain state: not-started
  no .claw directory under /disposable/wks

## next-step hint (state: not-started)
  No .claw yet. Next: print-preview, then run the preview command yourself.
`;

describe("discovery — parseAuditWorkspace", () => {
  it("parses chain state and a single present artifact path", () => {
    const a = parseAuditWorkspace(AUDIT_PREVIEW_READY);
    assert.strictEqual(a.chainState, "preview-ready");
    assert.strictEqual(auditPathFor(a, "preview-bundle.json"), "/disposable/wks/.claw/preview-bundle.json");
    assert.strictEqual(auditPathFor(a, "approval-result.json"), null);
    assert.strictEqual(a.targetHash.checked, false);
    assert.strictEqual(a.targetHash.match, null);
  });

  it("always returns presence for every known artifact name", () => {
    const a = parseAuditWorkspace(AUDIT_PREVIEW_READY);
    const names = a.artifacts.map((x) => x.name).sort();
    assert.deepStrictEqual(names, [...ARTIFACT_NAMES].sort());
  });

  it("parses a target-hash MATCH", () => {
    const a = parseAuditWorkspace(AUDIT_APPLIED_MATCH);
    assert.strictEqual(a.chainState, "applied");
    assert.strictEqual(a.targetHash.checked, true);
    assert.strictEqual(a.targetHash.match, true);
  });

  it("parses a target-hash MISMATCH", () => {
    const a = parseAuditWorkspace(AUDIT_MISMATCH);
    assert.strictEqual(a.targetHash.checked, true);
    assert.strictEqual(a.targetHash.match, false);
  });

  it("handles a not-started workspace with no artifacts", () => {
    const a = parseAuditWorkspace(AUDIT_NOT_STARTED);
    assert.strictEqual(a.chainState, "not-started");
    for (const art of a.artifacts) {
      assert.strictEqual(art.present, false);
      assert.strictEqual(art.path, null);
    }
  });

  it("coerces an unrecognized chain state to 'unknown'", () => {
    const a = parseAuditWorkspace("chain state: bananas\n");
    assert.strictEqual(a.chainState, "unknown");
  });

  it("is resilient to empty input", () => {
    const a = parseAuditWorkspace("");
    assert.strictEqual(a.chainState, "unknown");
    assert.strictEqual(a.artifacts.length, ARTIFACT_NAMES.length);
  });
});

describe("discovery — parseFindArtifacts", () => {
  const FIND = `----------------------------------------------------------------
A2 find-artifacts (read-only) under: /disposable/wks/.claw
----------------------------------------------------------------

## preview-bundle.json
  path : /disposable/wks/.claw/preview-bundle.json
  sha  : deadbeef

## preview-generator-result.json
  (none found)

## approval-result.json
  path : /disposable/wks/.claw/approval-result.json
  sha  : cafef00d

## apply-bundle.json
  (none found)
`;

  it("groups paths under their artifact name", () => {
    const f = parseFindArtifacts(FIND);
    const byName = new Map(f.groups.map((g) => [g.name, g.paths]));
    assert.deepStrictEqual(byName.get("preview-bundle.json"), [
      "/disposable/wks/.claw/preview-bundle.json",
    ]);
    assert.deepStrictEqual(byName.get("preview-generator-result.json"), []);
    assert.deepStrictEqual(byName.get("approval-result.json"), [
      "/disposable/wks/.claw/approval-result.json",
    ]);
  });
});

describe("discovery — parseHelpClawPath", () => {
  it("extracts the configured claw path from usage output", () => {
    const help = `usage: a2-ide-harness.sh <subcommand>
  A2_CLAW   path to the built claw binary (default: the dated build artifact).
            current: /media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw
`;
    assert.strictEqual(
      parseHelpClawPath(help),
      "/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw",
    );
  });

  it("returns null when no current: line is present", () => {
    assert.strictEqual(parseHelpClawPath("usage: ...\n"), null);
  });
});

describe("discovery — selectCandidate", () => {
  it("auto-selects exactly one unique candidate", () => {
    const s = selectCandidate(["/a/plan.yaml"]);
    assert.strictEqual(s.mode, "auto");
    assert.strictEqual(s.path, "/a/plan.yaml");
  });

  it("collapses duplicates to a single auto candidate", () => {
    const s = selectCandidate(["/a/plan.yaml", "/a/plan.yaml"]);
    assert.strictEqual(s.mode, "auto");
    assert.strictEqual(s.path, "/a/plan.yaml");
  });

  it("requires a pick when multiple distinct candidates exist", () => {
    const s = selectCandidate(["/a/plan.yaml", "/b/plan.yaml"]);
    assert.strictEqual(s.mode, "select-needed");
    assert.strictEqual(s.path, null);
    assert.deepStrictEqual(s.candidates, ["/a/plan.yaml", "/b/plan.yaml"]);
  });

  it("reports none for an empty/blank set (never silently infers)", () => {
    assert.strictEqual(selectCandidate([]).mode, "none");
    assert.strictEqual(selectCandidate([null, undefined, "  "]).mode, "none");
  });
});
