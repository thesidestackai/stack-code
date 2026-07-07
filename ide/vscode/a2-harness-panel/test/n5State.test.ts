import * as assert from "assert";
import {
  N5_STATES,
  N5_FORBIDDEN_TARGETS,
  deriveN5State,
  assertN5Safe,
  isN5BlockedState,
  n5NextStepLabel,
  N5Inputs,
} from "../src/n5State";
import { N4_FORBIDDEN_TARGETS } from "../src/n4State";

function inputs(over: Partial<N5Inputs>): N5Inputs {
  return {
    n4State: "N4_EVIDENCE_READY",
    packagePlanReadiness: "READY",
    hasEvidenceData: true,
    ...over,
  };
}

describe("n5State — derivation", () => {
  it("NOT_READY when N4 is not ready", () => {
    assert.strictEqual(deriveN5State(inputs({ n4State: "N4_NOT_READY" })), "N5_NOT_READY");
  });
  it("NOT_READY when N4 has draft but is not at evidence level", () => {
    assert.strictEqual(deriveN5State(inputs({ n4State: "N4_PLAN_DRAFT_PRESENT" })), "N5_NOT_READY");
    assert.strictEqual(deriveN5State(inputs({ n4State: "N4_PREVIEW_DATA_MISSING" })), "N5_NOT_READY");
    assert.strictEqual(deriveN5State(inputs({ n4State: "N4_PREVIEW_READY" })), "N5_NOT_READY");
    assert.strictEqual(deriveN5State(inputs({ n4State: "N4_DIFF_READY" })), "N5_NOT_READY");
  });
  it("PACKAGE_PLAN_READY when N4 evidence-ready and package-plan READY", () => {
    assert.strictEqual(deriveN5State(inputs({})), "N5_PACKAGE_PLAN_READY");
  });
  it("REVIEW_READY when N4 evidence-ready but package-plan NOT_READY", () => {
    assert.strictEqual(
      deriveN5State(inputs({ packagePlanReadiness: "NOT_READY" })),
      "N5_REVIEW_READY",
    );
  });
  it("DEFERRED_REQUIRES_EXECUTION_TOKEN when package-plan EXECUTION_REQUIRED", () => {
    assert.strictEqual(
      deriveN5State(inputs({ packagePlanReadiness: "EXECUTION_REQUIRED" })),
      "N5_DEFERRED_REQUIRES_EXECUTION_TOKEN",
    );
  });
  it("BLOCKED_MISSING_EVIDENCE when N4 evidence-ready but no evidence data", () => {
    assert.strictEqual(
      deriveN5State(inputs({ hasEvidenceData: false })),
      "N5_BLOCKED_MISSING_EVIDENCE",
    );
  });
  it("BLOCKED_AMBIGUOUS_ARTIFACTS when package-plan is BLOCKED", () => {
    assert.strictEqual(
      deriveN5State(inputs({ packagePlanReadiness: "BLOCKED" })),
      "N5_BLOCKED_AMBIGUOUS_ARTIFACTS",
    );
  });
});

describe("n5State — SAFETY: fail closed; blocked states win", () => {
  it("BLOCKED_UNSAFE_TARGET inherited from N4_BLOCKED_UNSAFE_TARGET", () => {
    assert.strictEqual(
      deriveN5State(inputs({ n4State: "N4_BLOCKED_UNSAFE_TARGET" })),
      "N5_BLOCKED_UNSAFE_TARGET",
    );
  });
  it("BLOCKED_EXECUTABLE_STEP inherited from N4_BLOCKED_EXECUTABLE_STEP", () => {
    assert.strictEqual(
      deriveN5State(inputs({ n4State: "N4_BLOCKED_EXECUTABLE_STEP" })),
      "N5_BLOCKED_EXECUTABLE_STEP",
    );
  });
  it("BLOCKED_AMBIGUOUS_ARTIFACTS inherited from N4_BLOCKED_AMBIGUOUS_ARTIFACTS", () => {
    assert.strictEqual(
      deriveN5State(inputs({ n4State: "N4_BLOCKED_AMBIGUOUS_ARTIFACTS" })),
      "N5_BLOCKED_AMBIGUOUS_ARTIFACTS",
    );
  });
  it("a blocking condition overrides every ready facet (fail closed)", () => {
    assert.ok(isN5BlockedState(deriveN5State(inputs({ n4State: "N4_BLOCKED_UNSAFE_TARGET" }))));
    assert.ok(isN5BlockedState(deriveN5State(inputs({ packagePlanReadiness: "BLOCKED" }))));
    assert.ok(isN5BlockedState(deriveN5State(inputs({ hasEvidenceData: false }))));
  });
});

describe("n5State — SAFETY: never routes to execution or apply gate", () => {
  it("no N5 state collides with a forbidden target", () => {
    for (const s of N5_STATES) {
      assert.ok(!N5_FORBIDDEN_TARGETS.includes(s), `N5 state collides with forbidden: ${s}`);
      assert.strictEqual(assertN5Safe(s), s);
    }
  });
  it("assertN5Safe throws on forbidden targets", () => {
    for (const f of N5_FORBIDDEN_TARGETS) {
      assert.throws(() => assertN5Safe(f), /execution|apply gate/i);
    }
  });
  it("assertN5Safe throws on unknown state", () => {
    assert.throws(() => assertN5Safe("UNKNOWN_STATE"), /unknown N5 state/);
  });
  it("N5_FORBIDDEN_TARGETS is a strict superset of N4_FORBIDDEN_TARGETS", () => {
    for (const t of N4_FORBIDDEN_TARGETS) {
      assert.ok(
        N5_FORBIDDEN_TARGETS.includes(t),
        `N5_FORBIDDEN_TARGETS must include N4 target: ${t}`,
      );
    }
    assert.ok(
      N5_FORBIDDEN_TARGETS.length > N4_FORBIDDEN_TARGETS.length,
      "N5 must add at least one target beyond N4",
    );
  });
  it("N5_FORBIDDEN_TARGETS includes all N4 apply-gate states", () => {
    for (const t of N4_FORBIDDEN_TARGETS) {
      assert.ok(N5_FORBIDDEN_TARGETS.includes(t));
    }
  });
  it("N5_FORBIDDEN_TARGETS includes execution-side states not in N4", () => {
    assert.ok(N5_FORBIDDEN_TARGETS.includes("PACKAGE_PLAN_EXECUTING"));
    assert.ok(N5_FORBIDDEN_TARGETS.includes("PACKAGE_COMMIT_EXECUTING"));
    assert.ok(N5_FORBIDDEN_TARGETS.includes("PACKAGE_PUSH_EXECUTING"));
    assert.ok(N5_FORBIDDEN_TARGETS.includes("PACKAGE_PR_EXECUTING"));
    assert.ok(N5_FORBIDDEN_TARGETS.includes("EXECUTION_APPROVED"));
  });
  it("assertN5Safe throws on package-ladder execution states", () => {
    assert.throws(() => assertN5Safe("PACKAGE_PLAN_EXECUTING"), /execution|apply gate/i);
    assert.throws(() => assertN5Safe("PACKAGE_COMMIT_EXECUTING"), /execution|apply gate/i);
    assert.throws(() => assertN5Safe("PACKAGE_PUSH_EXECUTING"), /execution|apply gate/i);
    assert.throws(() => assertN5Safe("PACKAGE_PR_EXECUTING"), /execution|apply gate/i);
    assert.throws(() => assertN5Safe("EXECUTION_APPROVED"), /execution|apply gate/i);
  });
  it("assertN5Safe throws on all N4 forbidden targets", () => {
    for (const f of N4_FORBIDDEN_TARGETS) {
      assert.throws(() => assertN5Safe(f), /execution|apply gate/i);
    }
  });
  it("every N5 state has a non-empty next-step label", () => {
    for (const s of N5_STATES) {
      assert.ok(n5NextStepLabel(s).length > 0, `empty label for state: ${s}`);
    }
  });
  it("READY labels all say 'separately-approved' (not 'run now')", () => {
    const readyStates: string[] = [
      "N5_PACKAGE_PLAN_READY",
      "N5_PACKAGE_COMMIT_READY",
      "N5_PACKAGE_PUSH_READY",
      "N5_PACKAGE_PR_READY",
    ];
    for (const s of readyStates) {
      const label = n5NextStepLabel(s as Parameters<typeof n5NextStepLabel>[0]);
      assert.ok(
        label.toLowerCase().includes("separately-approved") ||
          label.toLowerCase().includes("separate"),
        `READY state label must mention separate approved lane, got: ${label}`,
      );
    }
  });
});
