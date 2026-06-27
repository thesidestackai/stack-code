import * as assert from "assert";
import { buildN4Inputs, buildN4View, renderN4SummaryLines } from "../src/n4View";
import { emptyTaskDraft, reduceTaskIntake, TaskDraft, TaskIntakeEvent } from "../src/n3TaskIntake";

const stamp = { now: "2026-06-20T00:00:00Z" };
function run(events: TaskIntakeEvent[]): TaskDraft {
  return events.reduce(reduceTaskIntake, emptyTaskDraft("t1"));
}

const VALIDATED_SOURCE = run([
  { type: "DescribeTask", summary: "tidy", intent: "edit src", workspaceRoot: null, stamp },
  { type: "DeclareTarget", path: "src/a.ts", stamp },
  { type: "DraftPlan", stamp },
  { type: "ValidateDraft", stamp },
]);

const BLOCKED_RUNTIME = run([
  { type: "DescribeTask", summary: "x", intent: "touch runtime", workspaceRoot: null, stamp },
  { type: "DeclareTarget", path: "runtime/x.toml", stamp },
  { type: "DraftPlan", stamp },
  { type: "ValidateDraft", stamp },
]);

describe("n4View — inputs from the N3 draft", () => {
  it("empty draft => not ready", () => {
    const inputs = buildN4Inputs(emptyTaskDraft("t1"));
    assert.strictEqual(inputs.hasPlanDraft, false);
  });
  it("validated source draft => full facets present, validated", () => {
    const inputs = buildN4Inputs(VALIDATED_SOURCE);
    assert.strictEqual(inputs.hasPlanDraft, true);
    assert.strictEqual(inputs.validationStatus, "PLAN_DRAFT_VALIDATED");
    assert.strictEqual(inputs.hasPreviewData, true);
    assert.strictEqual(inputs.hasDiffData, true);
    assert.strictEqual(inputs.hasEvidenceData, true);
    assert.strictEqual(inputs.hasForbiddenFamilyTarget, false);
  });
});

describe("n4View — read-only viewer over a validated draft", () => {
  it("EVIDENCE_READY with VERIFIED facets and rendered content", () => {
    const v = buildN4View(VALIDATED_SOURCE);
    assert.strictEqual(v.state, "N4_EVIDENCE_READY");
    assert.strictEqual(v.isBlocked, false);
    assert.strictEqual(v.preview.trust, "VERIFIED");
    assert.strictEqual(v.diff.trust, "VERIFIED");
    assert.strictEqual(v.evidence.trust, "VERIFIED");
    assert.ok(v.preview.lines.length > 0);
    assert.ok(v.diff.lines.some((l) => l.startsWith("not_executable_reason:")));
    assert.ok(v.evidence.lines.some((l) => l.startsWith("evidence:")));
  });
  it("not-ready draft yields NOT_READY with empty facets", () => {
    const v = buildN4View(emptyTaskDraft("t1"));
    assert.strictEqual(v.state, "N4_NOT_READY");
    assert.deepStrictEqual(v.preview.lines, []);
    assert.deepStrictEqual(v.diff.lines, []);
    assert.deepStrictEqual(v.evidence.lines, []);
  });
});

describe("n4View — SAFETY: fail closed on blocked, render nothing as verified", () => {
  it("a runtime-config draft is BLOCKED with all facets BLOCKED and EMPTY", () => {
    const v = buildN4View(BLOCKED_RUNTIME);
    assert.ok(v.isBlocked);
    assert.strictEqual(v.state, "N4_BLOCKED_UNSAFE_TARGET");
    for (const f of [v.preview, v.diff, v.evidence]) {
      assert.strictEqual(f.trust, "BLOCKED");
      assert.deepStrictEqual(f.lines, [], "blocked facet must render no content");
    }
  });
});

describe("n4View — summary lines", () => {
  it("surfaces state + per-facet trust", () => {
    const lines = renderN4SummaryLines(buildN4View(VALIDATED_SOURCE));
    assert.ok(lines.some((l) => l === "state: N4_EVIDENCE_READY"));
    assert.ok(lines.some((l) => l === "preview: VERIFIED"));
    assert.ok(lines.some((l) => l === "evidence: VERIFIED"));
  });
});
