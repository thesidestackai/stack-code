import * as assert from "assert";
import {
  classifyWrite,
  validateDeclaredSet,
  normalizeDeclared,
  MutationScopeInput,
} from "../src/mutationScope";

const WT = "/mnt/vast-data/git-worktrees/stack-code-tier3-demo";

function scope(paths: string[]): MutationScopeInput {
  return { worktreeRoot: WT, declaredPaths: paths };
}

describe("mutationScope — declared set", () => {
  it("normalizes and de-duplicates declared paths", () => {
    const out = normalizeDeclared([WT + "/a.ts", WT + "/./a.ts", "", WT + "/b.ts"]);
    assert.deepStrictEqual(out, [WT + "/a.ts", WT + "/b.ts"]);
  });

  it("accepts a declared set inside the worktree", () => {
    assert.deepStrictEqual(validateDeclaredSet(scope([WT + "/src/x.ts"])), []);
  });

  it("rejects an empty declared set", () => {
    assert.ok(validateDeclaredSet(scope([])).some((p) => /no declared/.test(p)));
  });

  it("rejects a declared path outside the worktree", () => {
    const problems = validateDeclaredSet(scope(["/etc/passwd"]));
    assert.ok(problems.some((p) => /outside the disposable worktree/.test(p)));
  });

  it("rejects a declared path under the control checkout", () => {
    const problems = validateDeclaredSet(scope(["/home/suki/stack-code/src/x.ts"]));
    assert.ok(problems.some((p) => /control checkout/.test(p)));
  });
});

describe("mutationScope — classifyWrite (exact-path, deny by default)", () => {
  const s = scope([WT + "/src/x.ts", WT + "/test/x.test.ts"]);

  it("accepts a declared path inside the worktree", () => {
    assert.strictEqual(classifyWrite(WT + "/src/x.ts", s).decision, "accepted");
  });

  it("rejects a path not in the declared set", () => {
    const r = classifyWrite(WT + "/src/y.ts", s);
    assert.strictEqual(r.decision, "rejected");
    assert.ok(/not in the declared/.test(r.reason));
  });

  it("rejects a path outside the worktree", () => {
    const r = classifyWrite("/tmp/evil.ts", s);
    assert.strictEqual(r.decision, "rejected");
    assert.ok(/outside the disposable worktree/.test(r.reason));
  });

  it("rejects a path under the control checkout even via traversal", () => {
    // Four ".." escape the 4-segment worktree root fully to "/", then descend
    // into the real control checkout — must be rejected as control-checkout.
    const r = classifyWrite(WT + "/../../../../home/suki/stack-code/src/x.ts", s);
    assert.strictEqual(r.decision, "rejected");
    assert.ok(/control checkout/.test(r.reason));
  });

  it("rejects a partial traversal that lands outside the worktree", () => {
    // Three ".." land at /mnt/home/... (not the control checkout) — still
    // rejected, as outside the disposable worktree (deny-by-default).
    const r = classifyWrite(WT + "/../../../home/suki/stack-code/src/x.ts", s);
    assert.strictEqual(r.decision, "rejected");
    assert.ok(/outside the disposable worktree/.test(r.reason));
  });

  it("rejects a non-absolute path", () => {
    assert.strictEqual(classifyWrite("src/x.ts", s).decision, "rejected");
  });
});
