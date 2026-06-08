import * as assert from "assert";
import { computeReadiness, dirtyCheckoutWarning } from "../src/agentReadiness";

describe("agentReadiness — guard-safe defaults (no git probe in v0)", () => {
  it("renders git dimensions as not-checked when no git facts are supplied", () => {
    const r = computeReadiness({
      workspaceRoot: "/ws",
      currentTier: 1,
      deniedRegistryLoaded: true,
      safeExecutorMode: "print-validate-only",
    });
    assert.strictEqual(r.repoDetected, "not-checked");
    assert.strictEqual(r.gitBranch, "not-checked");
    assert.strictEqual(r.dirtyState, "not-checked");
    assert.strictEqual(r.stagedChanges, "not-checked");
    assert.strictEqual(r.unstagedChanges, "not-checked");
    assert.strictEqual(r.untrackedFiles, "not-checked");
  });

  it("states a reason for not-checked git readiness and never fabricates green", () => {
    const r = computeReadiness({
      workspaceRoot: "/ws",
      currentTier: 1,
      deniedRegistryLoaded: true,
      safeExecutorMode: "print-validate-only",
    });
    assert.ok(typeof r.gitProbeNote === "string" && r.gitProbeNote.length > 0);
    assert.ok(/guard-safe/.test(r.gitProbeNote as string));
  });

  it("reports workspace + denied registry + executor mode honestly", () => {
    const r = computeReadiness({
      workspaceRoot: "/ws",
      currentTier: 2,
      deniedRegistryLoaded: true,
      safeExecutorMode: "print-validate-only",
    });
    assert.strictEqual(r.workspaceRoot, "detected");
    assert.strictEqual(r.deniedRegistryLoaded, "yes");
    assert.strictEqual(r.safeExecutorMode, "print-validate-only");
    assert.strictEqual(r.currentTier, 2);
  });

  it("reports not-detected when no workspace root is set", () => {
    const r = computeReadiness({
      workspaceRoot: null,
      currentTier: 1,
      deniedRegistryLoaded: false,
      safeExecutorMode: "print-validate-only",
    });
    assert.strictEqual(r.workspaceRoot, "not-detected");
    assert.strictEqual(r.deniedRegistryLoaded, "no");
  });
});

describe("agentReadiness — git facts when supplied", () => {
  it("uses supplied git facts and clears the probe note", () => {
    const r = computeReadiness({
      workspaceRoot: "/ws",
      currentTier: 1,
      deniedRegistryLoaded: true,
      safeExecutorMode: "print-validate-only",
      git: { repoDetected: true, gitBranch: "main", dirty: false, staged: false, unstaged: false, untracked: false },
    });
    assert.strictEqual(r.repoDetected, "yes");
    assert.strictEqual(r.gitBranch, "main");
    assert.strictEqual(r.dirtyState, "no");
    assert.strictEqual(r.gitProbeNote, null);
  });

  it("dirty-checkout warning fires only on a real dirty fact", () => {
    const clean = computeReadiness({
      workspaceRoot: "/ws",
      currentTier: 1,
      deniedRegistryLoaded: true,
      safeExecutorMode: "print-validate-only",
    });
    // not-checked must NOT raise a false warning
    assert.strictEqual(dirtyCheckoutWarning(clean), false);

    const dirty = computeReadiness({
      workspaceRoot: "/ws",
      currentTier: 1,
      deniedRegistryLoaded: true,
      safeExecutorMode: "print-validate-only",
      git: { dirty: true },
    });
    assert.strictEqual(dirty.dirtyState, "yes");
    assert.strictEqual(dirtyCheckoutWarning(dirty), true);
  });
});
