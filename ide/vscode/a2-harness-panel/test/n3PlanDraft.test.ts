import * as assert from "assert";
import {
  PlanDraft,
  validatePlanDraft,
  planDraftIsNonExecutable,
  renderPlanDraftLines,
  DEFAULT_NOT_EXECUTABLE_REASON,
} from "../src/n3PlanDraft";
import { defaultForbiddenPaths } from "../src/n3RiskClassifier";

function goodDraft(over: Partial<PlanDraft> = {}): PlanDraft {
  return {
    draft_id: "t1-draft",
    task_id: "t1",
    candidate_steps: ["Review intent", "Confirm declared paths are exact"],
    declared_paths: ["src/a.ts"],
    forbidden_paths: defaultForbiddenPaths(),
    expected_outputs: ["a reviewed draft"],
    risk_notes: "risk_level=SOURCE_EDIT",
    required_evidence: ["boundary check"],
    stop_gates: ["STOP before preview/apply"],
    not_executable_reason: DEFAULT_NOT_EXECUTABLE_REASON,
    risk_level: "SOURCE_EDIT",
    ...over,
  };
}

describe("n3PlanDraft — validator validates a clean draft", () => {
  it("VALIDATED for an exact-path, non-STOP, non-executable draft", () => {
    const v = validatePlanDraft(goodDraft());
    assert.strictEqual(v.status, "PLAN_DRAFT_VALIDATED", JSON.stringify(v.reasons));
    assert.deepStrictEqual(v.reasons, []);
  });
});

describe("n3PlanDraft — validator fails closed", () => {
  it("BLOCKED when not_executable_reason is empty", () => {
    const v = validatePlanDraft(goodDraft({ not_executable_reason: "  " }));
    assert.strictEqual(v.status, "PLAN_DRAFT_BLOCKED");
    assert.ok(v.reasons.some((r) => r.includes("not_executable_reason")));
  });
  it("BLOCKED on a glob declared path", () => {
    const v = validatePlanDraft(goodDraft({ declared_paths: ["src/**"] }));
    assert.strictEqual(v.status, "PLAN_DRAFT_BLOCKED");
  });
  it("BLOCKED on a declared path in a forbidden family", () => {
    const v = validatePlanDraft(goodDraft({ declared_paths: ["services/x.py"] }));
    assert.strictEqual(v.status, "PLAN_DRAFT_BLOCKED");
  });
  it("BLOCKED when forbidden_paths is missing an always-denied family", () => {
    const v = validatePlanDraft(goodDraft({ forbidden_paths: ["runtime"] }));
    assert.strictEqual(v.status, "PLAN_DRAFT_BLOCKED");
  });
  it("BLOCKED on a STOP risk level", () => {
    for (const r of ["RUNTIME_CONFIG", "SECRETS_OR_VAULT", "DESTRUCTIVE_OR_FORCE", "UNKNOWN"] as const) {
      assert.strictEqual(validatePlanDraft(goodDraft({ risk_level: r })).status, "PLAN_DRAFT_BLOCKED", r);
    }
  });
  it("BLOCKED when a candidate step looks executable", () => {
    for (const step of ["claw plan run", "git push origin main", "echo hi && rm x", "$(whoami)", "run: package-pr"]) {
      const v = validatePlanDraft(goodDraft({ candidate_steps: ["ok", step] }));
      assert.strictEqual(v.status, "PLAN_DRAFT_BLOCKED", step);
      assert.ok(v.reasons.some((x) => x.includes("executable")));
    }
  });
});

describe("n3PlanDraft — non-executable guarantee", () => {
  it("a clean draft is provably non-executable", () => {
    assert.strictEqual(planDraftIsNonExecutable(goodDraft()), true);
  });
  it("a draft with an executable-looking step is not non-executable", () => {
    assert.strictEqual(planDraftIsNonExecutable(goodDraft({ candidate_steps: ["claw plan apply"] })), false);
  });
  it("a draft missing not_executable_reason is not non-executable", () => {
    assert.strictEqual(planDraftIsNonExecutable(goodDraft({ not_executable_reason: "" })), false);
  });
});

describe("n3PlanDraft — render lines", () => {
  it("surfaces the not_executable_reason and risk level", () => {
    const lines = renderPlanDraftLines(goodDraft());
    assert.ok(lines.some((l) => l.startsWith("not_executable_reason:")));
    assert.ok(lines.some((l) => l === "risk_level: SOURCE_EDIT"));
  });
});
