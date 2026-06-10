import * as assert from "assert";
import {
  EVIDENCE_SNAPSHOT_SCHEMA,
  UNKNOWN,
  ABSENT,
  parseEvidenceSnapshot,
  viewFromSnapshot,
} from "../src/tier3EvidenceSnapshot";

function completeSnapshot(): any {
  return {
    schema_version: EVIDENCE_SNAPSHOT_SCHEMA,
    generated_from: {
      control_checkout: "/home/suki/stack-code",
      named_worktree: "/wt/canonical",
    },
    tier3_status: "READY_WITH_NOTES",
    fields: {
      last_successful_smoke_at: "2026-06-09",
      canonical_success_worktree: "/wt/canonical",
      last_written_file: "SMOKE_NOTES.md",
      approval_result_path: "/wt/canonical/.claw/approval-result.json",
      apply_bundle_path: "/wt/canonical/.claw/.../apply-bundle.json",
      checkpoint_manifest_path: "/wt/canonical/.claw/.../manifest.json",
      payload_sha256: "cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8",
      apply_result_mode: "stdout_only",
      control_checkout_status: "READY",
      partial_smoke_count: 11,
      next_safe_action: "Review evidence",
      blocked_reason: null,
    },
    subjects: [
      { subject: "control checkout", status: "READY" },
      { subject: "approval gate", status: "DO_NOT_RUN" },
    ],
    links: {
      closure_doc: "handoffs/closure.md",
      runbook: "handoffs/runbook.md",
    },
    caveats: ["apply-result evidenced on stdout only; no persisted apply-result.json file on this build"],
  };
}

describe("tier3 evidence snapshot — parse/view", () => {
  it("builds a supported view from a complete v0 snapshot", () => {
    const v = viewFromSnapshot(completeSnapshot());
    assert.strictEqual(v.unsupported, false);
    assert.strictEqual(v.unsupportedReason, null);
    assert.strictEqual(v.tier3Status, "READY_WITH_NOTES");
    assert.strictEqual(v.nextSafeAction, "Review evidence");
    assert.strictEqual(v.blockedReason, null);
  });

  it("surfaces the payload sha256 and partial count as rows", () => {
    const v = viewFromSnapshot(completeSnapshot());
    const sha = v.rows.find((r) => /payload/i.test(r.label));
    assert.ok(sha, "expected a payload sha256 row");
    assert.strictEqual(
      sha!.value,
      "cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8",
    );
    const partial = v.rows.find((r) => /partial/i.test(r.label));
    assert.ok(partial, "expected a partial smoke count row");
    assert.strictEqual(partial!.value, "11");
  });

  it("renders null path/value fields as ABSENT and missing status as UNKNOWN", () => {
    const snap = completeSnapshot();
    snap.fields.last_successful_smoke_at = null;
    snap.fields.payload_sha256 = null;
    delete snap.fields.apply_result_mode; // missing status-like field
    const v = viewFromSnapshot(snap);
    const last = v.rows.find((r) => /last proven|last successful/i.test(r.label));
    assert.ok(last, "expected a last-proven row");
    assert.strictEqual(last!.value, ABSENT);
    const sha = v.rows.find((r) => /payload/i.test(r.label));
    assert.strictEqual(sha!.value, ABSENT);
    const mode = v.rows.find((r) => /apply.?result mode/i.test(r.label));
    assert.ok(mode, "expected an apply-result-mode row");
    assert.strictEqual(mode!.value, UNKNOWN);
  });

  it("carries subjects, caveats, and doc links through to the view", () => {
    const v = viewFromSnapshot(completeSnapshot());
    assert.strictEqual(v.subjects.length, 2);
    assert.deepStrictEqual(
      v.subjects.find((s) => s.label === "approval gate")?.value,
      "DO_NOT_RUN",
    );
    assert.strictEqual(v.caveats.length, 1);
    assert.ok(/stdout only/i.test(v.caveats[0]));
    assert.strictEqual(v.links.length, 2);
    assert.ok(v.links.some((l) => l.href === "handoffs/closure.md"));
  });

  it("FAIL-CLOSED: a mismatched schema_version is unsupported with a reason", () => {
    const snap = completeSnapshot();
    snap.schema_version = "a2-tier3-evidence-snapshot.v999";
    const v = viewFromSnapshot(snap);
    assert.strictEqual(v.unsupported, true);
    assert.ok(/unsupported snapshot version/i.test(v.unsupportedReason || ""));
    assert.ok(/v999/.test(v.unsupportedReason || ""));
    // fail-closed: no evidence rows leak through
    assert.strictEqual(v.rows.length, 0);
  });

  it("FAIL-CLOSED: unparseable input is unsupported, never throws", () => {
    const v = parseEvidenceSnapshot("{ not json");
    assert.strictEqual(v.unsupported, true);
    assert.ok(/not valid json/i.test(v.unsupportedReason || ""));
    assert.strictEqual(v.rows.length, 0);
  });

  it("parseEvidenceSnapshot round-trips a valid JSON string to a supported view", () => {
    const v = parseEvidenceSnapshot(JSON.stringify(completeSnapshot()));
    assert.strictEqual(v.unsupported, false);
    assert.strictEqual(v.tier3Status, "READY_WITH_NOTES");
  });

  it("carries a blocked_reason when the snapshot is BLOCKED", () => {
    const snap = completeSnapshot();
    snap.tier3_status = "BLOCKED";
    snap.fields.blocked_reason = "control checkout dirty";
    const v = viewFromSnapshot(snap);
    assert.strictEqual(v.tier3Status, "BLOCKED");
    assert.strictEqual(v.blockedReason, "control checkout dirty");
  });
});
