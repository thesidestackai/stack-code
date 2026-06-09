import * as assert from "assert";
import {
  ApprovedLane,
  computeDryRun,
  dryRunCommand,
  summarizeDryRun,
} from "../src/executorDryRun";
import { DISPOSABLE_WORKTREE_ROOT } from "../src/disposableWorktreePlan";

const WT = DISPOSABLE_WORKTREE_ROOT + "stack-code-tier3-demo";

function approvedLane(over: Partial<ApprovedLane> = {}): ApprovedLane {
  return {
    objective: "demo",
    worktreePlan: { worktreePath: WT, branch: "feat/a2-tier3-demo", base: "origin/main" },
    declaredPaths: [WT + "/src/x.ts"],
    proposedWrites: [WT + "/src/x.ts"],
    proposedCommands: ["npm run lint"],
    operatorApproved: true,
    ...over,
  };
}

const READY_FACTS = {
  controlCheckoutClean: true,
  originMainConfirmed: true,
  worktreePathFree: true,
  branchNameFree: true,
  operatorApproved: true,
};

describe("executorDryRun — never creates or writes", () => {
  it("wouldCreateWorktree and wouldWriteFiles are always false", () => {
    const r = computeDryRun(approvedLane(), READY_FACTS);
    assert.strictEqual(r.wouldCreateWorktree, false);
    assert.strictEqual(r.wouldWriteFiles, false);
  });

  it("the printed command is describe-only (external; operator-run; no creation/writes)", () => {
    assert.ok(/--dry-run/.test(dryRunCommand()));
    assert.ok(/NO worktree creation/.test(dryRunCommand()));
    assert.ok(/NO writes/.test(dryRunCommand()));
  });
});

describe("executorDryRun — readiness gate", () => {
  it("not ready by default (no facts, not approved)", () => {
    const r = computeDryRun({
      objective: null,
      worktreePlan: null,
      declaredPaths: [],
      proposedWrites: [],
      proposedCommands: [],
      operatorApproved: false,
    });
    assert.strictEqual(r.ready, false);
    assert.strictEqual(r.readinessOverall, "not-ready");
    assert.strictEqual(r.planValid, false);
  });

  it("ready only when readiness facts + plan + scope + approval all hold", () => {
    const r = computeDryRun(approvedLane(), READY_FACTS);
    assert.strictEqual(r.ready, true);
    assert.strictEqual(r.readinessOverall, "ready");
  });

  it("not ready if operator has not approved", () => {
    const r = computeDryRun(approvedLane({ operatorApproved: false }), {
      ...READY_FACTS,
      operatorApproved: false,
    });
    assert.strictEqual(r.ready, false);
  });
});

describe("executorDryRun — per-step classification (denials win, exact-path)", () => {
  it("a declared in-worktree write would-accept", () => {
    const r = computeDryRun(approvedLane(), READY_FACTS);
    const w = r.steps.find((s) => s.kind === "write");
    assert.ok(w && w.decision === "would-accept");
  });

  it("a write outside the declared set would-reject", () => {
    const r = computeDryRun(approvedLane({ proposedWrites: [WT + "/src/other.ts"] }), READY_FACTS);
    const w = r.steps.find((s) => s.kind === "write");
    assert.ok(w && w.decision === "would-reject");
    assert.ok(/declared/.test(w.reason));
  });

  it("a write under the control checkout would-reject", () => {
    const r = computeDryRun(
      approvedLane({ proposedWrites: ["/home/suki/stack-code/src/x.ts"] }),
      READY_FACTS,
    );
    const w = r.steps.find((s) => s.kind === "write");
    assert.ok(w && w.decision === "would-reject");
  });

  it("a denied-registry command would-reject (denials win)", () => {
    const r = computeDryRun(
      approvedLane({ proposedCommands: ["git push --force origin main"] }),
      READY_FACTS,
    );
    const c = r.steps.find((s) => s.kind === "command");
    assert.ok(c && c.decision === "would-reject");
  });

  it("an approved validation command would-accept", () => {
    const r = computeDryRun(approvedLane({ proposedCommands: ["npm test"] }), READY_FACTS);
    const c = r.steps.find((s) => s.kind === "command");
    assert.ok(c && c.decision === "would-accept");
  });

  it("a non-allowlisted command would-reject", () => {
    const r = computeDryRun(approvedLane({ proposedCommands: ["echo hi"] }), READY_FACTS);
    const c = r.steps.find((s) => s.kind === "command");
    assert.ok(c && c.decision === "would-reject");
  });
});

describe("executorDryRun — summary", () => {
  it("summary never claims a creation or write happened", () => {
    const r = computeDryRun(approvedLane(), READY_FACTS);
    const lines = summarizeDryRun(r);
    assert.ok(lines.some((l) => /would create worktree: no/.test(l)));
    assert.ok(lines.some((l) => /would write files: no/.test(l)));
    assert.ok(/NO creation, NO writes/.test(r.summary));
  });
});
