import * as assert from "assert";
import {
  N6_FORBIDDEN_TARGETS,
  N6_STATES,
  N6_SUB_TOKEN_PLAN,
  N6_SUB_TOKEN_COMMIT,
  N6_SUB_TOKEN_PUSH,
  N6_SUB_TOKEN_PR,
  assertN6Safe,
  deriveN6RungStateName,
} from "../src/n6State";
import { N5_FORBIDDEN_TARGETS } from "../src/n5State";
import { buildN6View, emptyN6SessionState, N6SessionState } from "../src/n6View";

describe("N6ExecutionModel — boundary invariants", () => {
  describe("forbidden-target superset chain", () => {
    it("N6_FORBIDDEN_TARGETS ⊃ N5_FORBIDDEN_TARGETS (strict superset)", () => {
      for (const t of N5_FORBIDDEN_TARGETS) {
        assert.ok(N6_FORBIDDEN_TARGETS.includes(t), `N6 must include N5 forbidden: ${t}`);
      }
      const n6Only = N6_FORBIDDEN_TARGETS.filter((t) => !N5_FORBIDDEN_TARGETS.includes(t));
      assert.ok(n6Only.length > 0, "N6 must add at least one new forbidden target");
    });

    it("N6-specific forbidden targets include execution-and-beyond states", () => {
      // Execution-capability states that N6 blocks:
      const mustBlock = [
        // Apply gate and beyond
        "APPLY_EXECUTING", "APPLY_APPROVED", "APPLY_DONE",
        // PR states beyond draft
        "PR_APPROVED", "PR_MERGED", "MERGED",
        // Hidden execution patterns
        "AUTO_APPROVED", "HIDDEN_APPLY",
        // Force-push
        "PUSH_FORCE",
        // PR automation
        "PR_MARK_READY",
        // Runtime/model/broker/Vault calls (N6 introduces none)
        "MODEL_CALL_EXECUTING", "BROKER_CALL_EXECUTING", "VAULT_READ_EXECUTING",
      ];
      for (const t of mustBlock) {
        assert.ok(N6_FORBIDDEN_TARGETS.includes(t), `N6 must block: ${t}`);
      }
    });
  });

  describe("sub-token invariants (D2=A: in-memory only)", () => {
    it("all 4 sub-tokens are non-empty and distinct", () => {
      const tokens = [N6_SUB_TOKEN_PLAN, N6_SUB_TOKEN_COMMIT, N6_SUB_TOKEN_PUSH, N6_SUB_TOKEN_PR];
      for (const t of tokens) {
        assert.ok(typeof t === "string" && t.length > 0, `token must be non-empty: ${t}`);
      }
      assert.strictEqual(new Set(tokens).size, 4, "all 4 tokens must be distinct");
    });

    it("no sub-token is a prefix or suffix of another", () => {
      const tokens = [N6_SUB_TOKEN_PLAN, N6_SUB_TOKEN_COMMIT, N6_SUB_TOKEN_PUSH, N6_SUB_TOKEN_PR];
      for (let i = 0; i < tokens.length; i++) {
        for (let j = 0; j < tokens.length; j++) {
          if (i === j) continue;
          assert.ok(
            !tokens[i].startsWith(tokens[j]) && !tokens[i].endsWith(tokens[j]),
            `sub-tokens must not be prefixes/suffixes of each other: [${i}] ${tokens[i]} vs [${j}] ${tokens[j]}`,
          );
        }
      }
    });
  });

  describe("assertN6Safe: forbidden-target denial", () => {
    it("denies every N6_FORBIDDEN_TARGET string", () => {
      for (const t of N6_FORBIDDEN_TARGETS) {
        assert.throws(
          () => assertN6Safe(t),
          Error,
          `assertN6Safe must deny forbidden: ${t}`,
        );
      }
    });

    it("never denies valid N6 states", () => {
      for (const s of N6_STATES) {
        assert.doesNotThrow(() => assertN6Safe(s), `must allow valid N6 state: ${s}`);
      }
    });

    it("denies ambiguous near-miss state names", () => {
      // These are NOT valid N6 states and must not accidentally pass.
      const nearmiss = [
        "N6_APPLY",            // no apply in N6
        "N6_MERGED",           // forbidden (merge is human-only)
        "N6_PUSH_FORCE",       // forbidden (force-push blocked)
        "N6_PR_APPROVED",      // forbidden (draft-only)
        "N6_MODEL_CALL",       // forbidden (N6 introduces no model call)
        "PACKAGE_PLAN_RUNNING", // missing N6_ prefix
        "",                    // empty
      ];
      for (const s of nearmiss) {
        assert.throws(
          () => assertN6Safe(s),
          Error,
          `must reject near-miss: "${s}"`,
        );
      }
    });
  });

  describe("deriveN6RungStateName: consistent mapping", () => {
    it("plan AWAITING_TOKEN is distinct from pr AWAITING_TOKEN", () => {
      const planAwaiting = deriveN6RungStateName("plan", "AWAITING_TOKEN");
      const prAwaiting   = deriveN6RungStateName("pr", "AWAITING_TOKEN");
      assert.notStrictEqual(planAwaiting, prAwaiting);
      assert.strictEqual(planAwaiting, "N6_AWAITING_PACKAGE_PLAN_TOKEN");
      assert.strictEqual(prAwaiting,   "N6_AWAITING_DRAFT_PR_TOKEN");
    });

    it("covers exactly 20 valid rung+exec combinations (4 rungs × 5 exec states)", () => {
      const rungs = ["plan", "commit", "push", "pr"] as const;
      const execs = ["AWAITING_TOKEN", "TOKEN_ACTIVE", "RUNNING", "DONE", "FAILED"] as const;
      const seen = new Set<string>();
      for (const rung of rungs) {
        for (const exec of execs) {
          seen.add(deriveN6RungStateName(rung, exec));
        }
      }
      assert.strictEqual(seen.size, 20, "must produce 20 distinct states (no collision)");
    });
  });

  describe("buildN6View state safety: assertN6Safe called on every rung state", () => {
    it("buildN6View never throws for any valid rung exec combination", () => {
      const rungs = ["plan", "commit", "push", "pr"] as const;
      const execs = ["AWAITING_TOKEN", "TOKEN_ACTIVE", "RUNNING", "DONE", "FAILED"] as const;
      for (const rung of rungs) {
        for (const exec of execs) {
          const key = `${rung}Exec` as keyof N6SessionState;
          const s: N6SessionState = { ...emptyN6SessionState(), [key]: exec };
          assert.doesNotThrow(
            () => buildN6View(null, s),
            `must not throw for rung=${rung} exec=${exec}`,
          );
        }
      }
    });
  });

  describe("D3=B: N6 renders below N5 (render ordering invariant)", () => {
    it("N6PanelView.rungs has exactly 4 elements in all states", () => {
      const view = buildN6View(null, emptyN6SessionState());
      assert.strictEqual(view.rungs.length, 4);
    });

    it("rungs are ordered: plan, commit, push, pr", () => {
      const view = buildN6View(null, emptyN6SessionState());
      assert.strictEqual(view.rungs[0].rung, "plan");
      assert.strictEqual(view.rungs[1].rung, "commit");
      assert.strictEqual(view.rungs[2].rung, "push");
      assert.strictEqual(view.rungs[3].rung, "pr");
    });
  });

  describe("D4=B: FAILED rung does NOT show run button", () => {
    const rungs = ["plan", "commit", "push", "pr"] as const;
    for (const rung of rungs) {
      it(`${rung} rung: showRunButton = false when exec = FAILED (regardless of token state)`, () => {
        const key = `${rung}Exec` as keyof N6SessionState;
        const tokenKey = `${rung}TokenActive` as keyof N6SessionState;
        const s: N6SessionState = {
          ...emptyN6SessionState(),
          [key]: "FAILED",
          [tokenKey]: true,  // token is still "active" — D4=B must suppress the button
        };
        // For commit/push/pr, also set prior rungs DONE so isReady is true.
        const fullS: N6SessionState = {
          ...s,
          planExec:   rung !== "plan"   ? "DONE" : s.planExec,
          commitExec: rung === "push" || rung === "pr" ? "DONE" : s.commitExec,
          pushExec:   rung === "pr"     ? "DONE" : s.pushExec,
        };
        // Override the target rung back to FAILED (was clobbered by spread above for push/pr).
        const finalS: N6SessionState = { ...fullS, [key]: "FAILED", [tokenKey]: true };
        const view = buildN6View({ ladder: [{ readiness: "READY" }] }, finalS);
        const rungView = view.rungs.find((r) => r.rung === rung)!;
        assert.strictEqual(
          rungView.showRunButton,
          false,
          `FAILED rung ${rung} must not show run button (D4=B)`,
        );
      });
    }
  });

  describe("single-spawn-boundary: helperRunner.ts is the only spawn site", () => {
    it("N6 state/view/trust modules contain no spawn/exec/child_process patterns in source", () => {
      // This is a belt-and-braces code-pattern check against the N6 modules.
      // The canonical check is run-guards.js; this test verifies the same at
      // unit-test time so the CI lane catches it under `npm test`.
      const forbidden = ["child_process", "spawn(", "exec(", "eval("];
      const n6Modules = [
        require.resolve("../src/n6State"),
        require.resolve("../src/n6TrustLevel"),
        require.resolve("../src/n6View"),
      ];
      const fs = require("fs") as typeof import("fs");
      for (const modPath of n6Modules) {
        // Find the corresponding .ts file (module resolves to .js in test, but
        // the guard is on .ts source — so we read the .ts twin).
        const tsSrc = modPath.replace(/\.js$/, ".ts").replace(/\/out\//, "/src/").replace(/\\out\\/, "\\src\\");
        // If the .ts path doesn't exist, skip (we're running under ts-node or already .ts).
        const srcPath = tsSrc.endsWith(".ts") ? tsSrc : modPath;
        if (fs.existsSync(srcPath)) {
          const src = fs.readFileSync(srcPath, "utf8");
          for (const pat of forbidden) {
            // Strip comments and strings first for a fair check.
            // (Simplistic strip: just check that none appear outside of comment blocks.)
            const noLineComments = src.replace(/\/\/[^\n]*/g, "");
            const noBlockComments = noLineComments.replace(/\/\*[\s\S]*?\*\//g, "");
            assert.ok(
              !noBlockComments.includes(pat),
              `N6 module ${srcPath} must not reference "${pat}" in live code`,
            );
          }
        }
      }
    });
  });
});
