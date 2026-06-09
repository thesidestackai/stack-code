import * as assert from "assert";
import { computeTier3Readiness, dirtyControlCheckoutBlock } from "../src/tier3Readiness";

describe("tier3Readiness — guard-safe defaults (no probe in v0)", () => {
  it("renders gated dimensions as not-checked when no facts supplied", () => {
    const r = computeTier3Readiness({
      planValid: false,
      declaredScopePresent: false,
      deniedRegistryLoaded: true,
    });
    assert.strictEqual(r.controlCheckoutClean, "not-checked");
    assert.strictEqual(r.originMainConfirmed, "not-checked");
    assert.strictEqual(r.worktreePathFree, "not-checked");
    assert.strictEqual(r.branchNameFree, "not-checked");
    assert.strictEqual(r.operatorApproved, "not-checked");
    assert.ok(typeof r.probeNote === "string" && /guard-safe/.test(r.probeNote as string));
  });

  it("is not-ready by default and never ready-by-default", () => {
    const r = computeTier3Readiness({
      planValid: false,
      declaredScopePresent: false,
      deniedRegistryLoaded: true,
    });
    assert.strictEqual(r.overall, "not-ready");
  });

  it("dirty control checkout is a hard block surfaced honestly", () => {
    const r = computeTier3Readiness({
      facts: { controlCheckoutClean: false },
      planValid: true,
      declaredScopePresent: true,
      deniedRegistryLoaded: true,
    });
    assert.strictEqual(r.controlCheckoutClean, "no");
    assert.strictEqual(dirtyControlCheckoutBlock(r), true);
    assert.strictEqual(r.overall, "not-ready");
  });

  it("not-checked never raises a false dirty block", () => {
    const r = computeTier3Readiness({
      planValid: false,
      declaredScopePresent: false,
      deniedRegistryLoaded: true,
    });
    assert.strictEqual(dirtyControlCheckoutBlock(r), false);
  });
});

describe("tier3Readiness — ready only when every gate is yes", () => {
  it("is ready when all facts are yes + plan valid + scope present + registry loaded", () => {
    const r = computeTier3Readiness({
      facts: {
        controlCheckoutClean: true,
        originMainConfirmed: true,
        worktreePathFree: true,
        branchNameFree: true,
        operatorApproved: true,
      },
      planValid: true,
      declaredScopePresent: true,
      deniedRegistryLoaded: true,
    });
    assert.strictEqual(r.overall, "ready");
    assert.strictEqual(r.probeNote, null);
  });

  it("a single not-yes gate keeps it not-ready", () => {
    const r = computeTier3Readiness({
      facts: {
        controlCheckoutClean: true,
        originMainConfirmed: true,
        worktreePathFree: true,
        branchNameFree: true,
        operatorApproved: false, // not approved
      },
      planValid: true,
      declaredScopePresent: true,
      deniedRegistryLoaded: true,
    });
    assert.strictEqual(r.operatorApproved, "no");
    assert.strictEqual(r.overall, "not-ready");
  });
});
