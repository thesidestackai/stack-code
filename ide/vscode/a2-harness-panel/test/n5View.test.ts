import * as assert from "assert";
import { buildN5View } from "../src/n5View";
import { emptyTaskDraft, reduceTaskIntake, TaskDraft, TaskIntakeEvent } from "../src/n3TaskIntake";

const stamp = { now: "2026-07-01T00:00:00Z" };
function run(events: TaskIntakeEvent[]): TaskDraft {
  return events.reduce(reduceTaskIntake, emptyTaskDraft("t1"));
}

const VALIDATED_SOURCE = run([
  { type: "DescribeTask", summary: "tidy source", intent: "edit src/a.ts", workspaceRoot: null, stamp },
  { type: "DeclareTarget", path: "src/a.ts", stamp },
  { type: "DraftPlan", stamp },
  { type: "ValidateDraft", stamp },
]);

const BLOCKED_RUNTIME = run([
  { type: "DescribeTask", summary: "touch runtime", intent: "edit runtime config", workspaceRoot: null, stamp },
  { type: "DeclareTarget", path: "runtime/x.toml", stamp },
  { type: "DraftPlan", stamp },
  { type: "ValidateDraft", stamp },
]);

describe("n5View — board derivation from validated N3/N4 state", () => {
  it("empty draft => N5_NOT_READY with package-plan NOT_READY", () => {
    const v = buildN5View(emptyTaskDraft("t1"));
    assert.strictEqual(v.state, "N5_NOT_READY");
    assert.strictEqual(v.isBlocked, false);
    assert.strictEqual(v.ladder[0].rung, "package-plan");
    assert.strictEqual(v.ladder[0].readiness, "NOT_READY");
  });
  it("validated SOURCE draft => N5_PACKAGE_PLAN_READY", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.strictEqual(v.state, "N5_PACKAGE_PLAN_READY");
    assert.strictEqual(v.isBlocked, false);
  });
  it("package-plan is READY for validated source draft", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.strictEqual(v.ladder[0].rung, "package-plan");
    assert.strictEqual(v.ladder[0].readiness, "READY");
  });
  it("commit/push/pr are EXECUTION_REQUIRED even on fully validated draft", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.strictEqual(v.ladder[1].readiness, "EXECUTION_REQUIRED");
    assert.strictEqual(v.ladder[2].readiness, "EXECUTION_REQUIRED");
    assert.strictEqual(v.ladder[3].readiness, "EXECUTION_REQUIRED");
  });
  it("surfaces task summary and risk level from N3 draft", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.ok(v.taskSummary.length > 0);
    assert.ok(v.riskLevel.length > 0);
  });
  it("surfaces N4 state and step label", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.strictEqual(v.n4State, "N4_EVIDENCE_READY");
    assert.ok(v.n4StepLabel.length > 0);
  });
  it("ladder has exactly four rungs in order", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.deepStrictEqual(
      v.ladder.map((r) => r.rung),
      ["package-plan", "package-commit", "package-push", "package-pr"],
    );
  });
  it("package-plan preconditions are all VERIFIED when READY", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    assert.ok(v.ladder[0].preconditionLines.every((l) => l.startsWith("[VERIFIED]")));
  });
});

describe("n5View — SAFETY: fail closed on blocked draft", () => {
  it("runtime target draft => N5_BLOCKED_UNSAFE_TARGET", () => {
    const v = buildN5View(BLOCKED_RUNTIME);
    assert.strictEqual(v.state, "N5_BLOCKED_UNSAFE_TARGET");
    assert.strictEqual(v.isBlocked, true);
  });
  it("blocked view step label contains STOP", () => {
    const v = buildN5View(BLOCKED_RUNTIME);
    assert.ok(v.stepLabel.includes("STOP"));
  });
  it("EXECUTION_REQUIRED rungs are not listed as ready (never guessed)", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    for (const rung of v.ladder.slice(1)) {
      assert.notStrictEqual(rung.readiness, "READY");
      assert.strictEqual(rung.readiness, "EXECUTION_REQUIRED");
    }
  });
});

describe("n5View — package-plan READY note says separate approved lane", () => {
  it("READY note does not say 'run now'", () => {
    const v = buildN5View(VALIDATED_SOURCE);
    const note = v.ladder[0].note.toLowerCase();
    assert.ok(!note.includes("run now"), `note must not say 'run now': ${note}`);
    assert.ok(note.includes("separate approved"));
  });
});
