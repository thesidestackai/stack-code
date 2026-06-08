import * as assert from "assert";
import {
  DENIED_FAMILIES,
  classifyCommand,
  evaluate,
  deniedFamilyLabels,
} from "../src/deniedCommands";

describe("deniedCommands — registry shape", () => {
  it("declares the expected denied families", () => {
    const ids = DENIED_FAMILIES.map((f) => f.id).sort();
    assert.ok(ids.includes("destructive-filesystem-cleanup"));
    assert.ok(ids.includes("force-branch-or-worktree-deletion"));
    assert.ok(ids.includes("history-rewrite-or-force-push"));
    assert.ok(ids.includes("model-or-broker-call"));
    assert.ok(ids.includes("raw-app-inference"));
    assert.ok(ids.includes("vault-or-secret-read"));
    assert.ok(ids.includes("live-a2-chain-execution"));
    assert.ok(ids.includes("approval-line-composition"));
    assert.ok(ids.includes("watcher-polling-timer-automation"));
    assert.ok(ids.includes("hidden-execution"));
  });

  it("every family has a label, reason, and at least one pattern", () => {
    for (const f of DENIED_FAMILIES) {
      assert.ok(f.label.length > 0, `${f.id} label`);
      assert.ok(f.reason.length > 0, `${f.id} reason`);
      assert.ok(f.patterns.length > 0, `${f.id} patterns`);
    }
  });

  it("deniedFamilyLabels returns one label per family", () => {
    assert.strictEqual(deniedFamilyLabels().length, DENIED_FAMILIES.length);
  });
});

describe("deniedCommands — classification denies unsafe families", () => {
  const cases: Array<[string, string]> = [
    ["rm -rf /tmp/x", "destructive-filesystem-cleanup"],
    ["git clean -fd", "destructive-filesystem-cleanup"],
    ["git reset --hard HEAD~1", "destructive-filesystem-cleanup"],
    ["git branch -D feature", "force-branch-or-worktree-deletion"],
    ["git worktree remove --force /x", "force-branch-or-worktree-deletion"],
    ["git push --force origin main", "history-rewrite-or-force-push"],
    ["curl http://localhost:11434/api/chat", "raw-app-inference"],
    ["read the vault secret", "vault-or-secret-read"],
    ["claw plan apply bundle.json", "live-a2-chain-execution"],
    ["claw plan run plan.yaml", "live-a2-chain-execution"],
    ["systemctl restart ollama", "service-control"],
  ];

  for (const [cmd, fam] of cases) {
    it(`denies: ${cmd}`, () => {
      const d = classifyCommand(cmd);
      assert.strictEqual(d.denied, true, `expected denied for ${cmd}`);
      assert.ok(d.families.includes(fam as never), `expected family ${fam} for ${cmd}, got ${d.families.join(",")}`);
      assert.ok(typeof d.reason === "string" && d.reason.startsWith("denied"));
    });
  }

  it("does not deny a benign read-only command", () => {
    const d = classifyCommand("validate-input --plan plan.yaml");
    assert.strictEqual(d.denied, false);
    assert.strictEqual(d.families.length, 0);
    assert.strictEqual(d.reason, null);
  });
});

describe("deniedCommands — denials win over allowlist", () => {
  it("denies a denied command even when the allowlist would permit it", () => {
    const allowAll = () => true;
    const res = evaluate("git push --force origin main", allowAll);
    assert.strictEqual(res.decision, "denied");
    assert.ok(res.families.includes("history-rewrite-or-force-push"));
  });

  it("denies a non-denied command that the allowlist rejects", () => {
    const allowNone = () => false;
    const res = evaluate("validate-input", allowNone);
    assert.strictEqual(res.decision, "denied");
    assert.strictEqual(res.families.length, 0);
    assert.ok(/allowlist/.test(res.reason));
  });

  it("allows a non-denied command the allowlist permits", () => {
    const res = evaluate("validate-input", () => true);
    assert.strictEqual(res.decision, "allowed");
  });

  it("treats a non-denied command as allowed when no allowlist is provided", () => {
    const res = evaluate("audit-workspace");
    assert.strictEqual(res.decision, "allowed");
  });
});
