import * as assert from "assert";
import {
  N6TrustLevel,
  classifyN6Trust,
  isN6Reviewable,
  isN6Verified,
} from "../src/n6TrustLevel";

describe("N6TrustLevel — execution trust classification", () => {
  const ALL_LEVELS: N6TrustLevel[] = [
    "VERIFIED",
    "INFERRED",
    "MISSING",
    "BLOCKED",
    "EXECUTION_REQUIRED",
    "EXECUTION_OBSERVED",
    "EXECUTION_FAILED",
  ];

  it("classifyN6Trust returns MISSING for empty input", () => {
    assert.strictEqual(classifyN6Trust([]), "MISSING");
  });

  it("classifyN6Trust returns the single level when given a singleton", () => {
    for (const lvl of ALL_LEVELS) {
      assert.strictEqual(classifyN6Trust([lvl]), lvl, `singleton: ${lvl}`);
    }
  });

  it("BLOCKED wins over all other levels", () => {
    for (const lvl of ALL_LEVELS) {
      if (lvl === "BLOCKED") continue;
      assert.strictEqual(classifyN6Trust(["BLOCKED", lvl]), "BLOCKED");
      assert.strictEqual(classifyN6Trust([lvl, "BLOCKED"]), "BLOCKED");
    }
  });

  it("MISSING beats EXECUTION_REQUIRED, EXECUTION_FAILED, EXECUTION_OBSERVED, INFERRED, VERIFIED", () => {
    const weaker: N6TrustLevel[] = [
      "EXECUTION_REQUIRED",
      "EXECUTION_FAILED",
      "EXECUTION_OBSERVED",
      "INFERRED",
      "VERIFIED",
    ];
    for (const lvl of weaker) {
      assert.strictEqual(classifyN6Trust(["MISSING", lvl]), "MISSING");
    }
  });

  it("EXECUTION_REQUIRED beats EXECUTION_FAILED, EXECUTION_OBSERVED, INFERRED, VERIFIED", () => {
    const weaker: N6TrustLevel[] = [
      "EXECUTION_FAILED",
      "EXECUTION_OBSERVED",
      "INFERRED",
      "VERIFIED",
    ];
    for (const lvl of weaker) {
      assert.strictEqual(classifyN6Trust(["EXECUTION_REQUIRED", lvl]), "EXECUTION_REQUIRED");
    }
  });

  it("EXECUTION_OBSERVED beats INFERRED and VERIFIED", () => {
    assert.strictEqual(classifyN6Trust(["EXECUTION_OBSERVED", "INFERRED"]), "EXECUTION_OBSERVED");
    assert.strictEqual(classifyN6Trust(["EXECUTION_OBSERVED", "VERIFIED"]), "EXECUTION_OBSERVED");
  });

  it("INFERRED beats VERIFIED", () => {
    assert.strictEqual(classifyN6Trust(["INFERRED", "VERIFIED"]), "INFERRED");
  });

  it("VERIFIED+VERIFIED collapses to VERIFIED", () => {
    assert.strictEqual(classifyN6Trust(["VERIFIED", "VERIFIED"]), "VERIFIED");
  });

  it("classifyN6Trust is stable under different ordering", () => {
    const set1: N6TrustLevel[] = ["VERIFIED", "INFERRED", "EXECUTION_OBSERVED"];
    const set2: N6TrustLevel[] = ["EXECUTION_OBSERVED", "VERIFIED", "INFERRED"];
    assert.strictEqual(classifyN6Trust(set1), classifyN6Trust(set2));
  });

  describe("isN6Reviewable", () => {
    it("returns true ONLY for EXECUTION_OBSERVED", () => {
      assert.strictEqual(isN6Reviewable("EXECUTION_OBSERVED"), true);
    });

    it("returns false for all other levels", () => {
      const others: N6TrustLevel[] = [
        "VERIFIED",
        "INFERRED",
        "MISSING",
        "BLOCKED",
        "EXECUTION_REQUIRED",
        "EXECUTION_FAILED",
      ];
      for (const lvl of others) {
        assert.strictEqual(isN6Reviewable(lvl), false, `must be false for ${lvl}`);
      }
    });

    it("does NOT return true for VERIFIED (EXECUTION_OBSERVED ≠ VERIFIED)", () => {
      assert.strictEqual(isN6Reviewable("VERIFIED"), false);
    });
  });

  describe("isN6Verified", () => {
    it("returns true ONLY for VERIFIED", () => {
      assert.strictEqual(isN6Verified("VERIFIED"), true);
    });

    it("returns false for EXECUTION_OBSERVED (not auto-promoted to VERIFIED)", () => {
      assert.strictEqual(isN6Verified("EXECUTION_OBSERVED"), false);
    });

    it("returns false for all other levels", () => {
      const others: N6TrustLevel[] = [
        "INFERRED",
        "MISSING",
        "BLOCKED",
        "EXECUTION_REQUIRED",
        "EXECUTION_FAILED",
        "EXECUTION_OBSERVED",
      ];
      for (const lvl of others) {
        assert.strictEqual(isN6Verified(lvl), false, `must be false for ${lvl}`);
      }
    });
  });
});
