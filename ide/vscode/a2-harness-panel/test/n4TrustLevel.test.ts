import * as assert from "assert";
import {
  TRUST_LEVELS,
  classifyTrust,
  isReviewable,
  isBlocked,
  mustLabelInferred,
} from "../src/n4TrustLevel";

describe("n4TrustLevel — classifyTrust fails closed", () => {
  it("BLOCKED wins over everything (even present+verified)", () => {
    assert.strictEqual(classifyTrust({ present: true, verified: true, blocked: true }), "BLOCKED");
  });
  it("MISSING when absent and not blocked", () => {
    assert.strictEqual(classifyTrust({ present: false, verified: false, blocked: false }), "MISSING");
    assert.strictEqual(classifyTrust({ present: false, verified: true, blocked: false }), "MISSING");
  });
  it("VERIFIED only when present AND verified", () => {
    assert.strictEqual(classifyTrust({ present: true, verified: true, blocked: false }), "VERIFIED");
  });
  it("INFERRED when present but not independently verified", () => {
    assert.strictEqual(classifyTrust({ present: true, verified: false, blocked: false }), "INFERRED");
  });
});

describe("n4TrustLevel — predicates", () => {
  it("only VERIFIED/INFERRED are reviewable", () => {
    assert.strictEqual(isReviewable("VERIFIED"), true);
    assert.strictEqual(isReviewable("INFERRED"), true);
    assert.strictEqual(isReviewable("MISSING"), false);
    assert.strictEqual(isReviewable("BLOCKED"), false);
  });
  it("isBlocked is exact", () => {
    assert.strictEqual(isBlocked("BLOCKED"), true);
    assert.strictEqual(isBlocked("VERIFIED"), false);
  });
  it("INFERRED must be labelled inferred (never shown as verified)", () => {
    assert.strictEqual(mustLabelInferred("INFERRED"), true);
    assert.strictEqual(mustLabelInferred("VERIFIED"), false);
  });
  it("there are exactly four trust levels", () => {
    assert.deepStrictEqual([...TRUST_LEVELS].sort(), ["BLOCKED", "INFERRED", "MISSING", "VERIFIED"]);
  });
});
