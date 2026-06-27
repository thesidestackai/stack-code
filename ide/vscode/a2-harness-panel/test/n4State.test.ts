import * as assert from "assert";
import {
  N4_STATES,
  N4_FORBIDDEN_TARGETS,
  N4Inputs,
  deriveN4State,
  assertN4Safe,
  isBlockedState,
  n4NextStepLabel,
} from "../src/n4State";

function inputs(over: Partial<N4Inputs>): N4Inputs {
  return {
    hasPlanDraft: true,
    riskLevel: "SOURCE_EDIT",
    hasForbiddenFamilyTarget: false,
    planNonExecutable: true,
    validationStatus: "PLAN_DRAFT_VALIDATED",
    hasPreviewData: true,
    hasDiffData: true,
    hasEvidenceData: true,
    ...over,
  };
}

describe("n4State — derivation", () => {
  it("NOT_READY when no plan draft", () => {
    assert.strictEqual(deriveN4State(inputs({ hasPlanDraft: false })), "N4_NOT_READY");
  });
  it("EVIDENCE_READY for a fully-present validated draft", () => {
    assert.strictEqual(deriveN4State(inputs({})), "N4_EVIDENCE_READY");
  });
  it("DIFF_READY when evidence absent but diff present", () => {
    assert.strictEqual(deriveN4State(inputs({ hasEvidenceData: false })), "N4_DIFF_READY");
  });
  it("PREVIEW_READY when only preview present", () => {
    assert.strictEqual(
      deriveN4State(inputs({ hasEvidenceData: false, hasDiffData: false })),
      "N4_PREVIEW_READY",
    );
  });
  it("PREVIEW_DATA_MISSING when plan present but no facet data", () => {
    assert.strictEqual(
      deriveN4State(inputs({ hasEvidenceData: false, hasDiffData: false, hasPreviewData: false })),
      "N4_PREVIEW_DATA_MISSING",
    );
  });
});

describe("n4State — SAFETY: fail closed; blocked states win", () => {
  it("BLOCKED_UNSAFE_TARGET on a forbidden-family declared target", () => {
    assert.strictEqual(deriveN4State(inputs({ hasForbiddenFamilyTarget: true })), "N4_BLOCKED_UNSAFE_TARGET");
  });
  it("BLOCKED_UNSAFE_TARGET on secrets/runtime risk", () => {
    assert.strictEqual(deriveN4State(inputs({ riskLevel: "SECRETS_OR_VAULT" })), "N4_BLOCKED_UNSAFE_TARGET");
    assert.strictEqual(deriveN4State(inputs({ riskLevel: "RUNTIME_CONFIG" })), "N4_BLOCKED_UNSAFE_TARGET");
  });
  it("BLOCKED_EXECUTABLE_STEP when the plan is not provably non-executable", () => {
    assert.strictEqual(deriveN4State(inputs({ planNonExecutable: false })), "N4_BLOCKED_EXECUTABLE_STEP");
  });
  it("BLOCKED_AMBIGUOUS_ARTIFACTS on blocked validation / unknown / destructive / null risk", () => {
    assert.strictEqual(deriveN4State(inputs({ validationStatus: "PLAN_DRAFT_BLOCKED" })), "N4_BLOCKED_AMBIGUOUS_ARTIFACTS");
    assert.strictEqual(deriveN4State(inputs({ riskLevel: "UNKNOWN" })), "N4_BLOCKED_AMBIGUOUS_ARTIFACTS");
    assert.strictEqual(deriveN4State(inputs({ riskLevel: "DESTRUCTIVE_OR_FORCE" })), "N4_BLOCKED_AMBIGUOUS_ARTIFACTS");
    assert.strictEqual(deriveN4State(inputs({ riskLevel: null })), "N4_BLOCKED_AMBIGUOUS_ARTIFACTS");
  });
  it("a blocking condition overrides every ready facet (fail closed)", () => {
    // All facets present + validated, but an unsafe target -> still blocked.
    assert.ok(isBlockedState(deriveN4State(inputs({ hasForbiddenFamilyTarget: true }))));
  });
});

describe("n4State — SAFETY: never routes to the apply gate or beyond", () => {
  it("no N4 state collides with a forbidden apply-gate+ target", () => {
    for (const s of N4_STATES) {
      assert.ok(!N4_FORBIDDEN_TARGETS.includes(s), `N4 state collides: ${s}`);
      assert.strictEqual(assertN4Safe(s), s);
    }
  });
  it("assertN4Safe throws if handed a forbidden apply-gate state", () => {
    for (const f of N4_FORBIDDEN_TARGETS) {
      assert.throws(() => assertN4Safe(f), /apply gate or beyond/);
    }
  });
  it("the forbidden target list is exactly the apply-gate-and-beyond N2 states", () => {
    assert.deepStrictEqual([...N4_FORBIDDEN_TARGETS].sort(), [
      "APPLIED",
      "AWAITING_APPLY_APPROVAL",
      "COMMITTED",
      "DRAFT_PR_OPEN",
      "PACKAGE_READY",
      "PREVIEW_READY",
      "PUSHED",
    ]);
  });
  it("every N4 state has a non-empty next-step label", () => {
    for (const s of N4_STATES) {
      assert.ok(n4NextStepLabel(s).length > 0);
    }
  });
});
