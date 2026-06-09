import * as assert from "assert";
import {
  validateWorktreePlan,
  normalizeAbs,
  isUnder,
  summarizePlan,
  DISPOSABLE_WORKTREE_ROOT,
  CONTROL_CHECKOUT,
} from "../src/disposableWorktreePlan";

describe("disposableWorktreePlan — path helpers", () => {
  it("normalizes . and .. segments without touching the fs", () => {
    assert.strictEqual(normalizeAbs("/a/b/../c/./d"), "/a/c/d");
    assert.strictEqual(normalizeAbs("/a//b/"), "/a/b");
  });

  it("isUnder treats the dir itself and strict children as under", () => {
    assert.strictEqual(isUnder("/a/b", "/a/b"), true);
    assert.strictEqual(isUnder("/a/b", "/a/b/c"), true);
    assert.strictEqual(isUnder("/a/b", "/a/bc"), false);
    assert.strictEqual(isUnder("/a/b", "/a"), false);
  });
});

describe("disposableWorktreePlan — validation", () => {
  const goodPlan = {
    worktreePath: DISPOSABLE_WORKTREE_ROOT + "stack-code-tier3-demo",
    branch: "feat/a2-tier3-demo",
    base: "origin/main",
  };

  it("accepts a well-formed plan under the disposable worktree root", () => {
    const v = validateWorktreePlan(goodPlan);
    assert.strictEqual(v.valid, true);
    assert.deepStrictEqual(v.problems, []);
  });

  it("rejects a null plan", () => {
    const v = validateWorktreePlan(null);
    assert.strictEqual(v.valid, false);
    assert.ok(v.problems.length >= 1);
  });

  it("rejects a worktree path outside the disposable root", () => {
    const v = validateWorktreePlan({ ...goodPlan, worktreePath: "/tmp/somewhere" });
    assert.strictEqual(v.valid, false);
    assert.ok(v.problems.some((p) => /under/.test(p)));
  });

  it("rejects the control checkout as the worktree", () => {
    const v = validateWorktreePlan({ ...goodPlan, worktreePath: CONTROL_CHECKOUT });
    assert.strictEqual(v.valid, false);
    assert.ok(v.problems.some((p) => /control checkout/.test(p)));
  });

  it("rejects a non-origin/main base", () => {
    const v = validateWorktreePlan({ ...goodPlan, base: "main" });
    assert.strictEqual(v.valid, false);
    assert.ok(v.problems.some((p) => /origin\/main/.test(p)));
  });

  it("rejects main/master as the mutation branch", () => {
    assert.strictEqual(validateWorktreePlan({ ...goodPlan, branch: "main" }).valid, false);
    assert.strictEqual(validateWorktreePlan({ ...goodPlan, branch: "master" }).valid, false);
  });

  it("rejects a branch with whitespace", () => {
    const v = validateWorktreePlan({ ...goodPlan, branch: "feat/a b" });
    assert.strictEqual(v.valid, false);
  });

  it("summarizePlan marks creation as not performed", () => {
    const lines = summarizePlan(goodPlan);
    assert.ok(lines.some((l) => /creation: not performed/.test(l)));
  });
});
