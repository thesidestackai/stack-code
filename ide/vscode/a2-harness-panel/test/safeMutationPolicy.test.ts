import * as assert from "assert";
import {
  tier3Allowlist,
  evaluateTier3Command,
  evaluateTier3Write,
  policyInvariant,
} from "../src/safeMutationPolicy";
import { MutationScopeInput } from "../src/mutationScope";

const WT = "/mnt/vast-data/git-worktrees/stack-code-tier3-demo";

describe("safeMutationPolicy — Tier-3 command allowlist", () => {
  it("allows approved read-only/validation commands", () => {
    assert.strictEqual(tier3Allowlist("validate-input --plan plan.yaml"), true);
    assert.strictEqual(tier3Allowlist("npm install --ignore-scripts"), true);
    assert.strictEqual(tier3Allowlist("npm run lint"), true);
    assert.strictEqual(tier3Allowlist("npm run compile"), true);
    assert.strictEqual(tier3Allowlist("npm test"), true);
  });

  it("does not allowlist an arbitrary command", () => {
    assert.strictEqual(tier3Allowlist("echo hi"), false);
    assert.strictEqual(tier3Allowlist("npm publish"), false);
  });
});

describe("safeMutationPolicy — denials win over the Tier-3 allowlist", () => {
  it("denies a denied-registry command even if it looked allowlisted", () => {
    // force-push is on the denied registry; deny regardless of allowlist.
    const r = evaluateTier3Command("git push --force origin main");
    assert.strictEqual(r.decision, "denied");
  });

  it("denies a destructive cleanup command", () => {
    assert.strictEqual(evaluateTier3Command("git clean -fd").decision, "denied");
    assert.strictEqual(evaluateTier3Command("rm -rf /x").decision, "denied");
  });

  it("denies live A2 chain execution", () => {
    assert.strictEqual(evaluateTier3Command("claw plan apply bundle.json").decision, "denied");
  });

  it("allows an approved, non-denied validation command", () => {
    assert.strictEqual(evaluateTier3Command("npm run lint").decision, "allowed");
  });

  it("denies a non-denied command that is not on the Tier-3 allowlist", () => {
    assert.strictEqual(evaluateTier3Command("echo hi").decision, "denied");
  });
});

describe("safeMutationPolicy — writes gated by declared scope", () => {
  const scope: MutationScopeInput = { worktreeRoot: WT, declaredPaths: [WT + "/src/x.ts"] };

  it("allows a write to a declared path inside the worktree", () => {
    assert.strictEqual(evaluateTier3Write(WT + "/src/x.ts", scope).decision, "allowed");
  });

  it("denies a write outside the declared set", () => {
    assert.strictEqual(evaluateTier3Write(WT + "/src/y.ts", scope).decision, "denied");
  });

  it("denies a write under the control checkout", () => {
    assert.strictEqual(
      evaluateTier3Write("/home/suki/stack-code/src/x.ts", scope).decision,
      "denied",
    );
  });
});

describe("safeMutationPolicy — invariant statement", () => {
  it("states denials win + exact-path + nothing executes", () => {
    const s = policyInvariant();
    assert.ok(/Denials win/i.test(s));
    assert.ok(/exact-path/i.test(s));
    assert.ok(/nothing executes/i.test(s));
  });
});
