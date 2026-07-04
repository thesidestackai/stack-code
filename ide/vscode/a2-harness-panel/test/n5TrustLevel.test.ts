import * as assert from "assert";
import {
  N5_TRUST_LEVELS,
  classifyN5Trust,
  isN5Reviewable,
  isN5Blocked,
  requiresExecutionLane,
} from "../src/n5TrustLevel";

describe("n5TrustLevel — classifyN5Trust fails closed", () => {
  it("BLOCKED wins over everything (present+verified+requiresExecution)", () => {
    assert.strictEqual(
      classifyN5Trust({ present: true, verified: true, blocked: true, requiresExecution: true }),
      "BLOCKED",
    );
    assert.strictEqual(
      classifyN5Trust({ present: true, verified: true, blocked: true, requiresExecution: false }),
      "BLOCKED",
    );
  });
  it("MISSING when absent and not blocked", () => {
    assert.strictEqual(
      classifyN5Trust({ present: false, verified: false, blocked: false, requiresExecution: false }),
      "MISSING",
    );
    assert.strictEqual(
      classifyN5Trust({ present: false, verified: false, blocked: false, requiresExecution: true }),
      "MISSING",
    );
  });
  it("EXECUTION_REQUIRED when present and requiresExecution, not blocked", () => {
    assert.strictEqual(
      classifyN5Trust({ present: true, verified: false, blocked: false, requiresExecution: true }),
      "EXECUTION_REQUIRED",
    );
    assert.strictEqual(
      classifyN5Trust({ present: true, verified: true, blocked: false, requiresExecution: true }),
      "EXECUTION_REQUIRED",
    );
  });
  it("VERIFIED only when present AND verified AND not requiresExecution", () => {
    assert.strictEqual(
      classifyN5Trust({ present: true, verified: true, blocked: false, requiresExecution: false }),
      "VERIFIED",
    );
  });
  it("INFERRED when present but not verified and not requiresExecution", () => {
    assert.strictEqual(
      classifyN5Trust({ present: true, verified: false, blocked: false, requiresExecution: false }),
      "INFERRED",
    );
  });
});

describe("n5TrustLevel — predicates", () => {
  it("only VERIFIED/INFERRED are reviewable", () => {
    assert.strictEqual(isN5Reviewable("VERIFIED"), true);
    assert.strictEqual(isN5Reviewable("INFERRED"), true);
    assert.strictEqual(isN5Reviewable("MISSING"), false);
    assert.strictEqual(isN5Reviewable("BLOCKED"), false);
    assert.strictEqual(isN5Reviewable("EXECUTION_REQUIRED"), false);
  });
  it("EXECUTION_REQUIRED is NOT reviewable (not treated as ready)", () => {
    assert.strictEqual(isN5Reviewable("EXECUTION_REQUIRED"), false);
  });
  it("isN5Blocked is exact", () => {
    assert.strictEqual(isN5Blocked("BLOCKED"), true);
    assert.strictEqual(isN5Blocked("VERIFIED"), false);
    assert.strictEqual(isN5Blocked("EXECUTION_REQUIRED"), false);
  });
  it("requiresExecutionLane is exact", () => {
    assert.strictEqual(requiresExecutionLane("EXECUTION_REQUIRED"), true);
    assert.strictEqual(requiresExecutionLane("VERIFIED"), false);
    assert.strictEqual(requiresExecutionLane("BLOCKED"), false);
    assert.strictEqual(requiresExecutionLane("MISSING"), false);
    assert.strictEqual(requiresExecutionLane("INFERRED"), false);
  });
  it("there are exactly five N5 trust levels (N4 four + EXECUTION_REQUIRED)", () => {
    assert.deepStrictEqual([...N5_TRUST_LEVELS].sort(), [
      "BLOCKED",
      "EXECUTION_REQUIRED",
      "INFERRED",
      "MISSING",
      "VERIFIED",
    ]);
  });
});
