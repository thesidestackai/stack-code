import * as assert from "assert";
import {
  N3_STATES,
  N3_FORBIDDEN_TARGETS,
  deriveN3State,
  assertN3Safe,
  n3NextStepLabel,
  n3ToLadderSignals,
  buildN3View,
} from "../src/n3State";
import { emptyTaskDraft, reduceTaskIntake, TaskDraft, TaskIntakeEvent } from "../src/n3TaskIntake";

const stamp = { now: "2026-06-17T00:00:00Z" };
function run(events: TaskIntakeEvent[]): TaskDraft {
  return events.reduce(reduceTaskIntake, emptyTaskDraft("t1"));
}

describe("n3State — derivation covers every draft status", () => {
  it("maps each draft_status to its N3 state", () => {
    assert.strictEqual(deriveN3State(emptyTaskDraft("t1")), "TASK_INTAKE_EMPTY");
    const described = run([{ type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp }]);
    assert.strictEqual(deriveN3State(described), "TASK_DESCRIBED");
    const validated = run([
      { type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]);
    assert.strictEqual(deriveN3State(validated), "PLAN_DRAFT_VALIDATED");
    const blocked = run([
      { type: "DescribeTask", summary: "x", intent: "touch runtime", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "runtime/x.toml", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]);
    assert.strictEqual(deriveN3State(blocked), "PLAN_DRAFT_BLOCKED");
  });
});

describe("n3State — SAFETY: never targets the apply gate or beyond", () => {
  it("no N3 state is a forbidden (apply-gate+) target", () => {
    for (const s of N3_STATES) {
      assert.ok(!N3_FORBIDDEN_TARGETS.includes(s), `N3 state collides with forbidden target: ${s}`);
      assert.strictEqual(assertN3Safe(s), s); // does not throw
    }
  });
  it("assertN3Safe throws if handed a forbidden apply-gate state", () => {
    for (const f of N3_FORBIDDEN_TARGETS) {
      assert.throws(() => assertN3Safe(f), /apply gate or beyond/);
    }
  });
  it("the forbidden target list contains exactly the apply-gate-and-beyond N2 states", () => {
    assert.deepStrictEqual([...N3_FORBIDDEN_TARGETS].sort(), [
      "APPLIED",
      "AWAITING_APPLY_APPROVAL",
      "COMMITTED",
      "DRAFT_PR_OPEN",
      "PACKAGE_READY",
      "PREVIEW_READY",
      "PUSHED",
    ]);
  });
  it("every N3 state has a non-empty next-step label", () => {
    for (const s of N3_STATES) {
      assert.ok(n3NextStepLabel(s).length > 0);
    }
  });
});

describe("n3State — maps to N2 ladder signals (never past the apply gate)", () => {
  it("empty => nothing observed", () => {
    const sig = n3ToLadderSignals(emptyTaskDraft("t1"));
    assert.deepStrictEqual(sig, { taskDescribed: false, planDrafted: false, planValidated: false });
  });
  it("described => taskDescribed only", () => {
    const sig = n3ToLadderSignals(run([{ type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp }]));
    assert.deepStrictEqual(sig, { taskDescribed: true, planDrafted: false, planValidated: false });
  });
  it("validated => taskDescribed + planDrafted + planValidated, and nothing beyond", () => {
    const sig = n3ToLadderSignals(run([
      { type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]));
    assert.deepStrictEqual(sig, { taskDescribed: true, planDrafted: true, planValidated: true });
    // The slice only carries early-ladder signals — there is no apply/package/PR field to set.
    assert.deepStrictEqual(Object.keys(sig).sort(), ["planDrafted", "planValidated", "taskDescribed"]);
  });
  it("blocked => drafted but not validated", () => {
    const sig = n3ToLadderSignals(run([
      { type: "DescribeTask", summary: "x", intent: "touch runtime", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "runtime/x.toml", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]));
    assert.strictEqual(sig.planDrafted, true);
    assert.strictEqual(sig.planValidated, false);
  });
});

describe("n3State — buildN3View", () => {
  it("flags blocked + terminal correctly", () => {
    const blocked = buildN3View(run([
      { type: "DescribeTask", summary: "x", intent: "touch vault", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "infra/vault/p.hcl", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]));
    assert.strictEqual(blocked.state, "PLAN_DRAFT_BLOCKED");
    assert.strictEqual(blocked.isBlocked, true);
    assert.strictEqual(blocked.isTerminal, true);
  });
});
