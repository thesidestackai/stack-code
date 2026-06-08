import * as assert from "assert";
import {
  PERMISSION_TIERS,
  PermissionTier,
  tierById,
  defaultEffectiveTier,
  assertEffectiveTierSafe,
  TierId,
} from "../src/permissionTiers";

describe("permissionTiers — model shape", () => {
  it("defines exactly Tiers 0 through 5", () => {
    const ids = PERMISSION_TIERS.map((t) => t.id);
    assert.deepStrictEqual(ids, [0, 1, 2, 3, 4, 5]);
  });

  it("every tier exposes the required descriptive fields", () => {
    for (const t of PERMISSION_TIERS) {
      assert.ok(typeof t.name === "string" && t.name.length > 0, `tier ${t.id} name`);
      assert.ok(typeof t.summary === "string" && t.summary.length > 0, `tier ${t.id} summary`);
      assert.ok(Array.isArray(t.allowedActions), `tier ${t.id} allowedActions`);
      assert.ok(Array.isArray(t.deniedActions), `tier ${t.id} deniedActions`);
      assert.ok(Array.isArray(t.requiredGates), `tier ${t.id} requiredGates`);
      assert.ok(Array.isArray(t.evidenceRequired), `tier ${t.id} evidenceRequired`);
      assert.strictEqual(typeof t.deniedByDefault, "boolean");
      assert.strictEqual(typeof t.requiresExplicitApproval, "boolean");
    }
  });

  it("tierById returns the matching tier and throws on unknown", () => {
    const t: PermissionTier = tierById(3);
    assert.strictEqual(t.id, 3);
    assert.strictEqual(t.name, "Disposable Worktree Mutation");
    assert.throws(() => tierById(9 as unknown as TierId));
  });
});

describe("permissionTiers — safety invariants", () => {
  it("Tier 5 is denied by default and has no allowed actions", () => {
    const t5 = tierById(5);
    assert.strictEqual(t5.deniedByDefault, true);
    assert.strictEqual(t5.allowedActions.length, 0);
  });

  it("Tiers 3 and 4 require explicit approval", () => {
    assert.strictEqual(tierById(3).requiresExplicitApproval, true);
    assert.strictEqual(tierById(4).requiresExplicitApproval, true);
  });

  it("read-only tiers (0-2) are neither denied-by-default nor approval-gated", () => {
    for (const id of [0, 1, 2] as TierId[]) {
      const t = tierById(id);
      assert.strictEqual(t.deniedByDefault, false, `tier ${id} deniedByDefault`);
      assert.strictEqual(t.requiresExplicitApproval, false, `tier ${id} requiresExplicitApproval`);
    }
  });

  it("defaultEffectiveTier is Tier 1 without a read-only helper call, Tier 2 with one", () => {
    assert.strictEqual(defaultEffectiveTier(false), 1);
    assert.strictEqual(defaultEffectiveTier(true), 2);
  });

  it("assertEffectiveTierSafe accepts only read-only tiers (0-2)", () => {
    for (const id of [0, 1, 2] as TierId[]) {
      assert.strictEqual(assertEffectiveTierSafe(id), id);
    }
    for (const id of [3, 4, 5] as TierId[]) {
      assert.throws(() => assertEffectiveTierSafe(id), new RegExp("unsafe effective tier"), `tier ${id} must be rejected`);
    }
  });
});
