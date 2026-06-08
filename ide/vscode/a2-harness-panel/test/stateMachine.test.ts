import * as assert from "assert";
import {
  deriveState,
  nextSafeStep,
  assertSafe,
  stepButtonId,
  stepLabel,
  PANEL_STATES,
  SAFE_NEXT_STEPS,
  PanelState,
  StateInput,
} from "../src/stateMachine";

function input(partial: Partial<StateInput>): StateInput {
  return {
    workspaceDetected: true,
    planKnown: true,
    validated: false,
    chainState: null,
    targetHashChecked: false,
    targetHashMatch: null,
    ...partial,
  };
}

describe("stateMachine — deriveState", () => {
  it("NO_WORKSPACE when no workspace", () => {
    assert.strictEqual(deriveState(input({ workspaceDetected: false })), "NO_WORKSPACE");
  });
  it("WORKSPACE_SELECTED when workspace but no plan", () => {
    assert.strictEqual(deriveState(input({ planKnown: false })), "WORKSPACE_SELECTED");
  });
  it("PLAN_SELECTED when plan known, not validated, no audit", () => {
    assert.strictEqual(deriveState(input({})), "PLAN_SELECTED");
  });
  it("INPUT_VALIDATED when validated and no audit", () => {
    assert.strictEqual(deriveState(input({ validated: true })), "INPUT_VALIDATED");
  });
  it("NO_PREVIEW_ARTIFACTS when validated and chain not-started", () => {
    assert.strictEqual(
      deriveState(input({ validated: true, chainState: "not-started" })),
      "NO_PREVIEW_ARTIFACTS",
    );
  });
  it("PREVIEW_READY from chain preview-ready", () => {
    assert.strictEqual(deriveState(input({ chainState: "preview-ready" })), "PREVIEW_READY");
  });
  it("APPROVAL_RESULT_FOUND from chain approval-ready", () => {
    assert.strictEqual(deriveState(input({ chainState: "approval-ready" })), "APPROVAL_RESULT_FOUND");
  });
  it("APPLY_BUNDLE_FOUND from chain apply-bundle-ready", () => {
    assert.strictEqual(
      deriveState(input({ chainState: "apply-bundle-ready" })),
      "APPLY_BUNDLE_FOUND",
    );
  });
  it("FINAL_VERIFY_READY from chain applied (no hash check yet)", () => {
    assert.strictEqual(deriveState(input({ chainState: "applied" })), "FINAL_VERIFY_READY");
  });
  it("FINAL_MATCH / FINAL_MISMATCH override when target hash was checked", () => {
    assert.strictEqual(
      deriveState(input({ chainState: "applied", targetHashChecked: true, targetHashMatch: true })),
      "FINAL_MATCH",
    );
    assert.strictEqual(
      deriveState(input({ chainState: "applied", targetHashChecked: true, targetHashMatch: false })),
      "FINAL_MISMATCH",
    );
  });
});

describe("stateMachine — nextSafeStep is always a safe, non-executor step", () => {
  it("every panel state maps to a step in SAFE_NEXT_STEPS", () => {
    for (const s of PANEL_STATES) {
      const step = nextSafeStep(s as PanelState);
      assert.ok(SAFE_NEXT_STEPS.includes(step), `state ${s} -> unsafe ${step}`);
      // assertSafe must not throw for any reachable recommendation.
      assert.strictEqual(assertSafe(step), step);
    }
  });

  it("never recommends a chain executor (run/approve/apply-bundle/apply)", () => {
    for (const s of PANEL_STATES) {
      const step = nextSafeStep(s as PanelState);
      assert.ok(!/^(run|execute|approve|applybundle|apply)$/i.test(step), `executor step: ${step}`);
    }
  });

  it("SAFE_NEXT_STEPS itself contains no executor verb", () => {
    for (const step of SAFE_NEXT_STEPS) {
      assert.ok(!/^(run|execute|approve|applybundle|apply)$/i.test(step), `executor in safe set: ${step}`);
    }
  });

  it("specific mappings follow the scope table", () => {
    assert.strictEqual(nextSafeStep("WORKSPACE_SELECTED"), "SelectPlan");
    assert.strictEqual(nextSafeStep("PLAN_SELECTED"), "ValidateInput");
    assert.strictEqual(nextSafeStep("NO_PREVIEW_ARTIFACTS"), "PrintPreviewCommand");
    assert.strictEqual(nextSafeStep("PREVIEW_READY"), "PrintApprovalCommand");
    assert.strictEqual(nextSafeStep("APPLY_BUNDLE_FOUND"), "PrintApplyCommand");
    assert.strictEqual(nextSafeStep("FINAL_VERIFY_READY"), "VerifyFinalTarget");
    assert.strictEqual(nextSafeStep("FINAL_MATCH"), "Done");
    assert.strictEqual(nextSafeStep("FINAL_MISMATCH"), "StopInvestigate");
  });
});

describe("stateMachine — assertSafe rejects an executor step", () => {
  it("throws on a forged 'apply' step", () => {
    assert.throws(() => assertSafe("apply" as never), /unsafe next step/);
  });
  it("throws on a step outside the safe set", () => {
    assert.throws(() => assertSafe("RunPreview" as never), /unsafe next step/);
  });
});

describe("stateMachine — step metadata", () => {
  it("maps action steps to existing safe button ids", () => {
    assert.strictEqual(stepButtonId("ValidateInput"), "validate-input");
    assert.strictEqual(stepButtonId("PrintApplyCommand"), "show-apply-command");
    assert.strictEqual(stepButtonId("VerifyFinalTarget"), "verify-final");
  });
  it("guidance-only steps have no button id", () => {
    assert.strictEqual(stepButtonId("OpenWorkspace"), null);
    assert.strictEqual(stepButtonId("Done"), null);
    assert.strictEqual(stepButtonId("StopInvestigate"), null);
  });
  it("every step has a non-empty label", () => {
    for (const step of SAFE_NEXT_STEPS) {
      assert.ok(stepLabel(step).length > 0);
    }
  });
});
