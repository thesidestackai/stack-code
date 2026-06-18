import * as assert from "assert";
import {
  TaskDraft,
  TaskIntakeEvent,
  emptyTaskDraft,
  reduceTaskIntake,
  renderTaskIntakeLines,
} from "../src/n3TaskIntake";

const stamp = { now: "2026-06-17T00:00:00Z" };

function start(): TaskDraft {
  return emptyTaskDraft("t1");
}

function dispatch(state: TaskDraft, events: TaskIntakeEvent[]): TaskDraft {
  return events.reduce(reduceTaskIntake, state);
}

describe("n3TaskIntake — empty draft", () => {
  it("starts empty with the always-denied forbidden families seeded", () => {
    const s = start();
    assert.strictEqual(s.draft_status, "empty");
    assert.ok(s.forbidden_paths.includes("vault"));
    assert.ok(s.forbidden_paths.includes("secret"));
    assert.strictEqual(s.risk_level, null);
  });
});

describe("n3TaskIntake — reducer is total and ordered", () => {
  it("DescribeTask moves empty -> described", () => {
    const s = reduceTaskIntake(start(), { type: "DescribeTask", summary: "tidy", intent: "edit a source file", workspaceRoot: "/ws", stamp });
    assert.strictEqual(s.draft_status, "described");
    assert.strictEqual(s.task_summary, "tidy");
    assert.strictEqual(s.workspace_root, "/ws");
    assert.strictEqual(s.created_at, stamp.now);
  });

  it("DeclareTarget before DescribeTask is a no-op (describe first)", () => {
    const s = reduceTaskIntake(start(), { type: "DeclareTarget", path: "src/a.ts", stamp });
    assert.strictEqual(s.draft_status, "empty");
    assert.deepStrictEqual(s.declared_target_paths, []);
  });

  it("DeclareTarget dedups and advances to targets-declared", () => {
    const s = dispatch(start(), [
      { type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
    ]);
    assert.strictEqual(s.draft_status, "targets-declared");
    assert.deepStrictEqual(s.declared_target_paths, ["src/a.ts"]);
  });

  it("ClassifyRisk sets risk + approval flags", () => {
    const s = dispatch(start(), [
      { type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
      { type: "ClassifyRisk", stamp },
    ]);
    assert.strictEqual(s.draft_status, "risk-classified");
    assert.strictEqual(s.risk_level, "SOURCE_EDIT");
    assert.strictEqual(s.requires_real_tty, true);
    assert.strictEqual(s.requires_human_approval, true);
  });

  it("DraftPlan then ValidateDraft validates a clean source-edit draft", () => {
    const s = dispatch(start(), [
      { type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]);
    assert.ok(s.plan_draft !== null);
    assert.strictEqual(s.draft_status, "validated");
    assert.strictEqual(s.plan_validation && s.plan_validation.status, "PLAN_DRAFT_VALIDATED");
  });

  it("a STOP risk routes ValidateDraft to blocked", () => {
    const s = dispatch(start(), [
      { type: "DescribeTask", summary: "x", intent: "touch runtime", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "runtime/x.toml", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ]);
    assert.strictEqual(s.risk_level, "RUNTIME_CONFIG");
    assert.strictEqual(s.draft_status, "blocked");
    assert.strictEqual(s.plan_validation && s.plan_validation.status, "PLAN_DRAFT_BLOCKED");
  });

  it("READ_ONLY (no declared paths) needs no approval", () => {
    const s = dispatch(start(), [
      { type: "DescribeTask", summary: "look around", intent: "read only", workspaceRoot: null, stamp },
      { type: "ClassifyRisk", stamp },
    ]);
    assert.strictEqual(s.risk_level, "READ_ONLY");
    assert.strictEqual(s.requires_human_approval, false);
    assert.strictEqual(s.requires_real_tty, false);
  });

  it("Reset returns to empty for a new task id", () => {
    const s = dispatch(start(), [
      { type: "DescribeTask", summary: "x", intent: "y", workspaceRoot: null, stamp },
      { type: "Reset", taskId: "t2", stamp },
    ]);
    assert.strictEqual(s.draft_status, "empty");
    assert.strictEqual(s.task_id, "t2");
  });
});

describe("n3TaskIntake — SAFETY: no event reaches preview/apply/package/PR", () => {
  it("draft_status never leaves the N3 vocabulary regardless of event sequence", () => {
    const allowed = new Set(["empty", "described", "targets-declared", "risk-classified", "drafted", "validated", "blocked"]);
    const events: TaskIntakeEvent[] = [
      { type: "DescribeTask", summary: "x", intent: "edit src", workspaceRoot: null, stamp },
      { type: "DeclareTarget", path: "src/a.ts", stamp },
      { type: "DeclareForbidden", path: "extra/secret-thing", stamp },
      { type: "ClassifyRisk", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
      { type: "DraftPlan", stamp },
      { type: "ValidateDraft", stamp },
    ];
    let s = start();
    for (const e of events) {
      s = reduceTaskIntake(s, e);
      assert.ok(allowed.has(s.draft_status), `escaped N3 vocabulary: ${s.draft_status}`);
    }
  });
});

describe("n3TaskIntake — render lines", () => {
  it("renders the intake summary lines", () => {
    const s = reduceTaskIntake(start(), { type: "DescribeTask", summary: "tidy", intent: "edit", workspaceRoot: "/ws", stamp });
    const lines = renderTaskIntakeLines(s);
    assert.ok(lines.some((l) => l === "task: tidy"));
    assert.ok(lines.some((l) => l.startsWith("draft status:")));
  });
});
