import * as assert from "assert";
import {
  WorkspaceProbe,
  emptyWorkspaceProbe,
  computeWorkspaceStatusCard,
  renderWorkspaceStatusLines,
} from "../src/workspaceStatus";

function probe(partial: Partial<WorkspaceProbe>): WorkspaceProbe {
  return { ...emptyWorkspaceProbe(), ...partial };
}

describe("workspaceStatus — workspace detection (auto on open)", () => {
  it("not-detected when no root", () => {
    assert.strictEqual(computeWorkspaceStatusCard(emptyWorkspaceProbe()).workspace, "not-detected");
  });
  it("detected when a root is present (trimmed)", () => {
    const card = computeWorkspaceStatusCard(probe({ workspaceRoot: "  /home/suki/stack-code  " }));
    assert.strictEqual(card.workspace, "detected");
    assert.strictEqual(card.workspaceRoot, "/home/suki/stack-code");
  });
  it("whitespace-only root is not-detected", () => {
    assert.strictEqual(computeWorkspaceStatusCard(probe({ workspaceRoot: "   " })).workspace, "not-detected");
  });
});

describe("workspaceStatus — honest git unknowns (never green-by-default)", () => {
  it("branch / cleanliness / freshness are unknown when not probed", () => {
    const card = computeWorkspaceStatusCard(probe({ workspaceRoot: "/ws" }));
    assert.strictEqual(card.branch, "unknown");
    assert.strictEqual(card.branchName, null);
    assert.strictEqual(card.cleanliness, "unknown");
    assert.strictEqual(card.originMainFreshness, "unknown");
    assert.ok(card.gitProbeNote && card.gitProbeNote.length > 0, "expected an honest git-probe note");
  });
  it("reflects probed git facts when supplied", () => {
    const card = computeWorkspaceStatusCard(
      probe({ workspaceRoot: "/ws", branch: "main", worktreeClean: true, originMainFreshness: "current" }),
    );
    assert.strictEqual(card.branch, "known");
    assert.strictEqual(card.branchName, "main");
    assert.strictEqual(card.cleanliness, "clean");
    assert.strictEqual(card.originMainFreshness, "current");
    assert.strictEqual(card.gitProbeNote, null);
  });
  it("dirty worktree reflected honestly", () => {
    const card = computeWorkspaceStatusCard(probe({ workspaceRoot: "/ws", worktreeClean: false }));
    assert.strictEqual(card.cleanliness, "dirty");
  });
});

describe("workspaceStatus — readiness is honest and read-only", () => {
  it("needs-attention when no workspace", () => {
    assert.strictEqual(computeWorkspaceStatusCard(emptyWorkspaceProbe()).readiness, "needs-attention");
  });
  it("unknown when detected but git facts not probed", () => {
    assert.strictEqual(computeWorkspaceStatusCard(probe({ workspaceRoot: "/ws" })).readiness, "unknown");
  });
  it("ready only when detected, clean, and current", () => {
    const card = computeWorkspaceStatusCard(
      probe({ workspaceRoot: "/ws", branch: "main", worktreeClean: true, originMainFreshness: "current" }),
    );
    assert.strictEqual(card.readiness, "ready");
  });
  it("needs-attention when dirty even if current", () => {
    const card = computeWorkspaceStatusCard(
      probe({ workspaceRoot: "/ws", branch: "main", worktreeClean: false, originMainFreshness: "current" }),
    );
    assert.strictEqual(card.readiness, "needs-attention");
  });
  it("needs-attention when behind even if clean", () => {
    const card = computeWorkspaceStatusCard(
      probe({ workspaceRoot: "/ws", branch: "main", worktreeClean: true, originMainFreshness: "behind" }),
    );
    assert.strictEqual(card.readiness, "needs-attention");
  });
});

describe("workspaceStatus — render lines", () => {
  it("produces labelled read-only lines", () => {
    const card = computeWorkspaceStatusCard(
      probe({ workspaceRoot: "/ws", branch: "feat/x", worktreeClean: true, originMainFreshness: "current" }),
    );
    const lines = renderWorkspaceStatusLines(card);
    assert.ok(lines.some((l) => l === "workspace: detected"));
    assert.ok(lines.some((l) => l === "workspace root: /ws"));
    assert.ok(lines.some((l) => l === "branch: feat/x"));
    assert.ok(lines.some((l) => l === "worktree: clean"));
    assert.ok(lines.some((l) => l === "origin/main: current"));
    assert.ok(lines.some((l) => l === "readiness: ready"));
  });
  it("shows unknowns honestly in the lines", () => {
    const lines = renderWorkspaceStatusLines(computeWorkspaceStatusCard(probe({ workspaceRoot: "/ws" })));
    assert.ok(lines.some((l) => l === "branch: unknown"));
    assert.ok(lines.some((l) => l === "worktree: unknown"));
    assert.ok(lines.some((l) => l === "origin/main: unknown"));
  });
});
