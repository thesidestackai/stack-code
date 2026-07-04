import * as assert from "assert";
import { deriveLadderReadiness } from "../src/n5ReadinessModel";

const FULL_OPTS = {
  planNonExecutable: true,
  targetSafe: true,
  evidencePresent: true,
  planValidated: true,
};

describe("n5ReadinessModel — package-plan rung (read-only provable)", () => {
  it("READY when N4_EVIDENCE_READY and all preconditions VERIFIED", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "READY");
  });
  it("READY note says 'separate approved execution lane'", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.ok(r.packagePlan.note.toLowerCase().includes("separate approved"));
  });
  it("READY note says N5 does not run it", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.ok(r.packagePlan.note.toLowerCase().includes("n5 does not run"));
  });
  it("NOT_READY when plan not validated", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", { ...FULL_OPTS, planValidated: false });
    assert.strictEqual(r.packagePlan.readiness, "NOT_READY");
  });
  it("NOT_READY when N4 not at evidence-ready (preview missing)", () => {
    const r = deriveLadderReadiness("N4_PREVIEW_DATA_MISSING", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "NOT_READY");
  });
  it("NOT_READY when N4 is diff-ready only", () => {
    const r = deriveLadderReadiness("N4_DIFF_READY", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "NOT_READY");
  });
  it("BLOCKED when target not safe", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", { ...FULL_OPTS, targetSafe: false });
    assert.strictEqual(r.packagePlan.readiness, "BLOCKED");
  });
  it("BLOCKED when plan executable", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", { ...FULL_OPTS, planNonExecutable: false });
    assert.strictEqual(r.packagePlan.readiness, "BLOCKED");
  });
  it("BLOCKED when N4 is N4_BLOCKED_UNSAFE_TARGET", () => {
    const r = deriveLadderReadiness("N4_BLOCKED_UNSAFE_TARGET", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "BLOCKED");
  });
  it("BLOCKED when N4 is N4_BLOCKED_EXECUTABLE_STEP", () => {
    const r = deriveLadderReadiness("N4_BLOCKED_EXECUTABLE_STEP", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "BLOCKED");
  });
  it("BLOCKED when N4 is N4_BLOCKED_AMBIGUOUS_ARTIFACTS", () => {
    const r = deriveLadderReadiness("N4_BLOCKED_AMBIGUOUS_ARTIFACTS", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "BLOCKED");
  });
  it("all preconditions are VERIFIED when READY", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.ok(r.packagePlan.preconditions.every((p) => p.trust === "VERIFIED" && p.met));
  });
  it("has evidence when evidencePresent is true", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packagePlan.evidencePresent, true);
  });
});

describe("n5ReadinessModel — commit/push/pr always EXECUTION_REQUIRED", () => {
  it("package-commit is EXECUTION_REQUIRED", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packageCommit.readiness, "EXECUTION_REQUIRED");
  });
  it("package-push is EXECUTION_REQUIRED", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packagePush.readiness, "EXECUTION_REQUIRED");
  });
  it("package-pr is EXECUTION_REQUIRED", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packagePr.readiness, "EXECUTION_REQUIRED");
  });
  it("commit/push/pr are EXECUTION_REQUIRED even when package-plan is READY", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packagePlan.readiness, "READY");
    assert.strictEqual(r.packageCommit.readiness, "EXECUTION_REQUIRED");
    assert.strictEqual(r.packagePush.readiness, "EXECUTION_REQUIRED");
    assert.strictEqual(r.packagePr.readiness, "EXECUTION_REQUIRED");
  });
  it("commit/push/pr require operator confirmation", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.strictEqual(r.packageCommit.operatorConfirmationRequired, true);
    assert.strictEqual(r.packagePush.operatorConfirmationRequired, true);
    assert.strictEqual(r.packagePr.operatorConfirmationRequired, true);
  });
  it("EXECUTION_REQUIRED notes mention 'execution lane'", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.ok(r.packageCommit.note.toLowerCase().includes("execution lane"));
    assert.ok(r.packagePush.note.toLowerCase().includes("execution lane"));
    assert.ok(r.packagePr.note.toLowerCase().includes("execution lane"));
  });
  it("PR note mentions 'draft-only' and 'PR-open token'", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.ok(r.packagePr.note.toLowerCase().includes("draft-only"));
    assert.ok(r.packagePr.note.toLowerCase().includes("pr-open token"));
  });
  it("commit/push/pr preconditions are EXECUTION_REQUIRED trust", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    for (const rung of [r.packageCommit, r.packagePush, r.packagePr]) {
      assert.ok(rung.preconditions.every((p) => p.trust === "EXECUTION_REQUIRED"));
    }
  });
});

describe("n5ReadinessModel — apply boundary (never named or run)", () => {
  it("no rung is named 'apply' or 'apply-bundle'", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    for (const rung of [r.packagePlan, r.packageCommit, r.packagePush, r.packagePr]) {
      assert.ok(!rung.rung.includes("apply"), `rung name must not include 'apply': ${rung.rung}`);
    }
  });
  it("exactly four rungs are returned (package-plan/commit/push/pr)", () => {
    const r = deriveLadderReadiness("N4_EVIDENCE_READY", FULL_OPTS);
    assert.deepStrictEqual(
      [r.packagePlan.rung, r.packageCommit.rung, r.packagePush.rung, r.packagePr.rung],
      ["package-plan", "package-commit", "package-push", "package-pr"],
    );
  });
});
