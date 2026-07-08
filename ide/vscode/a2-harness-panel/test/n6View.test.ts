import * as assert from "assert";
import {
  N6SessionState,
  N6PanelView,
  N6RungView,
  N5LadderForN6,
  emptyN6SessionState,
  buildN6View,
} from "../src/n6View";
import {
  N6_SUB_TOKEN_PLAN,
  N6_SUB_TOKEN_COMMIT,
  N6_SUB_TOKEN_PUSH,
  N6_SUB_TOKEN_PR,
} from "../src/n6State";

// Minimal N5 ladder fixture with plan READY.
function n5WithPlanReady(): N5LadderForN6 {
  return {
    ladder: [
      { readiness: "READY" },        // plan
      { readiness: "NOT_READY" },    // commit
      { readiness: "NOT_READY" },    // push
      { readiness: "NOT_READY" },    // pr
    ],
  };
}

// Minimal N5 ladder fixture with plan NOT_READY.
function n5WithPlanNotReady(): N5LadderForN6 {
  return {
    ladder: [
      { readiness: "NOT_READY" },
      { readiness: "NOT_READY" },
      { readiness: "NOT_READY" },
      { readiness: "NOT_READY" },
    ],
  };
}

function rungForKey(view: N6PanelView, rung: "plan" | "commit" | "push" | "pr"): N6RungView {
  const r = view.rungs.find((r) => r.rung === rung);
  if (!r) throw new Error(`rung ${rung} not found`);
  return r;
}

describe("N6View — buildN6View (pure)", () => {
  describe("emptyN6SessionState", () => {
    it("returns all tokens inactive and all exec states AWAITING_TOKEN", () => {
      const s = emptyN6SessionState();
      assert.strictEqual(s.planTokenActive, false);
      assert.strictEqual(s.commitTokenActive, false);
      assert.strictEqual(s.pushTokenActive, false);
      assert.strictEqual(s.prTokenActive, false);
      assert.strictEqual(s.planExec, "AWAITING_TOKEN");
      assert.strictEqual(s.commitExec, "AWAITING_TOKEN");
      assert.strictEqual(s.pushExec, "AWAITING_TOKEN");
      assert.strictEqual(s.prExec, "AWAITING_TOKEN");
      assert.strictEqual(s.planOutput, null);
      assert.strictEqual(s.planExitCode, null);
    });
  });

  describe("initial state (no tokens, no n5)", () => {
    const view = buildN6View(null, emptyN6SessionState());

    it("produces 4 rungs", () => {
      assert.strictEqual(view.rungs.length, 4);
    });

    it("anyActivity is false when all exec states are AWAITING_TOKEN", () => {
      assert.strictEqual(view.anyActivity, false);
    });

    it("no run button shown when token not active", () => {
      for (const rung of view.rungs) {
        assert.strictEqual(rung.showRunButton, false, `${rung.rung}: showRunButton must be false`);
      }
    });

    it("no run button shown even if n5 plan is READY (token required)", () => {
      const v2 = buildN6View(n5WithPlanReady(), emptyN6SessionState());
      assert.strictEqual(rungForKey(v2, "plan").showRunButton, false);
    });

    it("each rung has the correct expectedToken", () => {
      const v = buildN6View(n5WithPlanReady(), emptyN6SessionState());
      assert.strictEqual(rungForKey(v, "plan").expectedToken,   N6_SUB_TOKEN_PLAN);
      assert.strictEqual(rungForKey(v, "commit").expectedToken, N6_SUB_TOKEN_COMMIT);
      assert.strictEqual(rungForKey(v, "push").expectedToken,   N6_SUB_TOKEN_PUSH);
      assert.strictEqual(rungForKey(v, "pr").expectedToken,     N6_SUB_TOKEN_PR);
    });

    it("each rung has the correct uiAction", () => {
      const v = buildN6View(null, emptyN6SessionState());
      assert.strictEqual(rungForKey(v, "plan").uiAction,   "n6RunPlan");
      assert.strictEqual(rungForKey(v, "commit").uiAction, "n6RunCommit");
      assert.strictEqual(rungForKey(v, "push").uiAction,   "n6RunPush");
      assert.strictEqual(rungForKey(v, "pr").uiAction,     "n6RunPr");
    });

    it("each rung has the correct tokenAction", () => {
      const v = buildN6View(null, emptyN6SessionState());
      assert.strictEqual(rungForKey(v, "plan").tokenAction,   "n6ActivatePlanToken");
      assert.strictEqual(rungForKey(v, "commit").tokenAction, "n6ActivateCommitToken");
      assert.strictEqual(rungForKey(v, "push").tokenAction,   "n6ActivatePushToken");
      assert.strictEqual(rungForKey(v, "pr").tokenAction,     "n6ActivatePrToken");
    });
  });

  describe("precondition: package-plan requires N5 ladder[0] READY", () => {
    it("plan isReady = false when n5 is null", () => {
      const s: N6SessionState = { ...emptyN6SessionState(), planTokenActive: true, planExec: "TOKEN_ACTIVE" };
      const v = buildN6View(null, s);
      assert.strictEqual(rungForKey(v, "plan").isReady, false);
      assert.strictEqual(rungForKey(v, "plan").showRunButton, false, "no button without precondition");
    });

    it("plan isReady = false when N5 ladder[0].readiness != READY", () => {
      const s: N6SessionState = { ...emptyN6SessionState(), planTokenActive: true, planExec: "TOKEN_ACTIVE" };
      const v = buildN6View(n5WithPlanNotReady(), s);
      assert.strictEqual(rungForKey(v, "plan").isReady, false);
      assert.strictEqual(rungForKey(v, "plan").showRunButton, false);
    });

    it("plan showRunButton = true when token active AND N5 plan READY", () => {
      const s: N6SessionState = { ...emptyN6SessionState(), planTokenActive: true, planExec: "TOKEN_ACTIVE" };
      const v = buildN6View(n5WithPlanReady(), s);
      assert.strictEqual(rungForKey(v, "plan").isReady, true);
      assert.strictEqual(rungForKey(v, "plan").showRunButton, true);
    });
  });

  describe("precondition chain: commit/push/pr require prior rung DONE", () => {
    it("commit isReady = false when planExec != DONE", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "TOKEN_ACTIVE", // not DONE
        commitTokenActive: true,
        commitExec: "TOKEN_ACTIVE",
      };
      const v = buildN6View(n5WithPlanReady(), s);
      assert.strictEqual(rungForKey(v, "commit").isReady, false);
      assert.strictEqual(rungForKey(v, "commit").showRunButton, false);
    });

    it("commit showRunButton = true when planExec = DONE AND commit token active", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitTokenActive: true,
        commitExec: "TOKEN_ACTIVE",
      };
      const v = buildN6View(n5WithPlanReady(), s);
      assert.strictEqual(rungForKey(v, "commit").isReady, true);
      assert.strictEqual(rungForKey(v, "commit").showRunButton, true);
    });

    it("push requires commitExec = DONE", () => {
      const sBlocked: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitExec: "TOKEN_ACTIVE", // not DONE
        pushTokenActive: true,
        pushExec: "TOKEN_ACTIVE",
      };
      assert.strictEqual(rungForKey(buildN6View(null, sBlocked), "push").showRunButton, false);

      const sReady: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitExec: "DONE",
        pushTokenActive: true,
        pushExec: "TOKEN_ACTIVE",
      };
      assert.strictEqual(rungForKey(buildN6View(null, sReady), "push").showRunButton, true);
    });

    it("pr requires pushExec = DONE", () => {
      const sBlocked: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitExec: "DONE",
        pushExec: "TOKEN_ACTIVE", // not DONE
        prTokenActive: true,
        prExec: "TOKEN_ACTIVE",
      };
      assert.strictEqual(rungForKey(buildN6View(null, sBlocked), "pr").showRunButton, false);

      const sReady: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitExec: "DONE",
        pushExec: "DONE",
        prTokenActive: true,
        prExec: "TOKEN_ACTIVE",
      };
      assert.strictEqual(rungForKey(buildN6View(null, sReady), "pr").showRunButton, true);
    });
  });

  describe("D4=B: RUNNING/DONE/FAILED suppress the run button", () => {
    const suppressStates = ["RUNNING", "DONE", "FAILED"] as const;
    for (const execState of suppressStates) {
      it(`plan showRunButton = false when planExec = ${execState}`, () => {
        const s: N6SessionState = {
          ...emptyN6SessionState(),
          planTokenActive: true,
          planExec: execState,
        };
        const v = buildN6View(n5WithPlanReady(), s);
        assert.strictEqual(rungForKey(v, "plan").showRunButton, false);
      });
    }
  });

  describe("anyActivity flag", () => {
    it("is false with all AWAITING_TOKEN and no active tokens", () => {
      assert.strictEqual(buildN6View(null, emptyN6SessionState()).anyActivity, false);
    });

    it("is true when planTokenActive = true", () => {
      const s: N6SessionState = { ...emptyN6SessionState(), planTokenActive: true };
      assert.strictEqual(buildN6View(null, s).anyActivity, true);
    });

    it("is true when any exec state != AWAITING_TOKEN", () => {
      const s: N6SessionState = { ...emptyN6SessionState(), planExec: "DONE" };
      assert.strictEqual(buildN6View(null, s).anyActivity, true);
    });
  });

  describe("output and exit-code passthrough", () => {
    it("passes planOutput and planExitCode to the rung view", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        planOutput: "plan output text",
        planExitCode: 0,
      };
      const v = buildN6View(null, s);
      assert.strictEqual(rungForKey(v, "plan").output, "plan output text");
      assert.strictEqual(rungForKey(v, "plan").exitCode, 0);
    });

    it("passes null output before rung runs", () => {
      const v = buildN6View(null, emptyN6SessionState());
      assert.strictEqual(rungForKey(v, "plan").output, null);
      assert.strictEqual(rungForKey(v, "plan").exitCode, null);
    });
  });

  describe("stateName validation via assertN6Safe", () => {
    it("all rungs have valid N6 state names in all exec states", () => {
      const rungs = ["plan", "commit", "push", "pr"] as const;
      const execs = ["AWAITING_TOKEN", "TOKEN_ACTIVE", "RUNNING", "DONE", "FAILED"] as const;
      for (const rung of rungs) {
        for (const exec of execs) {
          const sessionKey = `${rung}Exec` as keyof N6SessionState;
          const s: N6SessionState = { ...emptyN6SessionState(), [sessionKey]: exec };
          // buildN6View calls assertN6Safe internally; must not throw.
          assert.doesNotThrow(() => buildN6View(null, s), `rung=${rung} exec=${exec}`);
        }
      }
    });
  });
});
