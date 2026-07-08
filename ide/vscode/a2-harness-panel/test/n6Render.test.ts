import * as assert from "assert";
import { renderHtml } from "../src/render";
import { emptyN6SessionState, buildN6View } from "../src/n6View";
import { N6SessionState } from "../src/n6View";
import { emptyInputs } from "../src/render";

// Helper: build a minimal RenderModel with only the n6 field populated.
function modelWithN6(n6State: N6SessionState, n5Ready = false) {
  const n5 = n5Ready
    ? {
        state: "N5_READY_FOR_PACKAGE_PLAN" as const,
        stepLabel: "Ready for package-plan",
        isBlocked: false,
        n4State: "N4_DRAFT_REVIEWED" as const,
        n4StepLabel: "Draft reviewed",
        taskSummary: "test task",
        riskLevel: "low",
        ladder: [
          { rung: "package-plan",   purpose: "p", readiness: "READY",     preconditionLines: [], evidencePresent: true,  operatorConfirmationRequired: false, note: "" },
          { rung: "package-commit", purpose: "p", readiness: "NOT_READY", preconditionLines: [], evidencePresent: false, operatorConfirmationRequired: false, note: "" },
          { rung: "package-push",   purpose: "p", readiness: "NOT_READY", preconditionLines: [], evidencePresent: false, operatorConfirmationRequired: false, note: "" },
          { rung: "package-pr",     purpose: "p", readiness: "NOT_READY", preconditionLines: [], evidencePresent: false, operatorConfirmationRequired: false, note: "" },
        ],
      }
    : null;

  return {
    inputs: emptyInputs(),
    output: null,
    notice: null,
    n5,
    n6: buildN6View(n5, n6State),
  };
}

describe("N6Render — HTML output (D7 HTML-level check)", () => {
  describe("n6 section always present", () => {
    it("renders the n6 section when n6 is null (degraded hint)", () => {
      const model = { inputs: emptyInputs(), output: null, notice: null };
      const html = renderHtml(model);
      assert.ok(html.includes('data-testid="n6-execution-boundary"'), "n6 section must always render");
    });

    it("renders the n6 section with no tokens (empty session)", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(html.includes('data-testid="n6-execution-boundary"'));
    });
  });

  describe("token entry buttons (AWAITING_TOKEN state)", () => {
    it("shows token-entry button for plan when no token active", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(
        html.includes('data-ui-action="n6ActivatePlanToken"'),
        "plan token-entry button must be present when AWAITING_TOKEN",
      );
    });

    it("shows token-entry buttons for all 4 rungs in empty state", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(html.includes('data-ui-action="n6ActivatePlanToken"'));
      assert.ok(html.includes('data-ui-action="n6ActivateCommitToken"'));
      assert.ok(html.includes('data-ui-action="n6ActivatePushToken"'));
      assert.ok(html.includes('data-ui-action="n6ActivatePrToken"'));
    });
  });

  describe("D7=C: run buttons must carry data-n6-token-required", () => {
    it("plan run button carries data-n6-token-required='true'", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planTokenActive: true,
        planExec: "TOKEN_ACTIVE",
      };
      const html = renderHtml(modelWithN6(s, true)); // n5 plan READY
      assert.ok(
        html.includes('data-ui-action="n6RunPlan"'),
        "plan run button must be present",
      );
      // D7 assertion: every n6RunPlan button must have data-n6-token-required="true".
      const allRun = [...html.matchAll(/data-ui-action="n6RunPlan"/g)];
      for (const match of allRun) {
        const context = html.slice(
          Math.max(0, match.index! - 50),
          Math.min(html.length, match.index! + 200),
        );
        assert.ok(
          context.includes('data-n6-token-required="true"'),
          `n6RunPlan button at offset ${match.index} missing data-n6-token-required="true"`,
        );
      }
    });

    it("commit run button carries data-n6-token-required='true'", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitTokenActive: true,
        commitExec: "TOKEN_ACTIVE",
      };
      const html = renderHtml(modelWithN6(s));
      assert.ok(html.includes('data-ui-action="n6RunCommit"'), "commit run button must be present");
      const allRun = [...html.matchAll(/data-ui-action="n6RunCommit"/g)];
      for (const match of allRun) {
        const context = html.slice(
          Math.max(0, match.index! - 50),
          Math.min(html.length, match.index! + 200),
        );
        assert.ok(
          context.includes('data-n6-token-required="true"'),
          `n6RunCommit button missing data-n6-token-required`,
        );
      }
    });

    it("push run button carries data-n6-token-required='true'", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitExec: "DONE",
        pushTokenActive: true,
        pushExec: "TOKEN_ACTIVE",
      };
      const html = renderHtml(modelWithN6(s));
      assert.ok(html.includes('data-ui-action="n6RunPush"'), "push run button must be present");
      const allRun = [...html.matchAll(/data-ui-action="n6RunPush"/g)];
      for (const match of allRun) {
        const context = html.slice(
          Math.max(0, match.index! - 50),
          Math.min(html.length, match.index! + 200),
        );
        assert.ok(
          context.includes('data-n6-token-required="true"'),
          "n6RunPush button missing data-n6-token-required",
        );
      }
    });

    it("pr run button carries data-n6-token-required='true'", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        commitExec: "DONE",
        pushExec: "DONE",
        prTokenActive: true,
        prExec: "TOKEN_ACTIVE",
      };
      const html = renderHtml(modelWithN6(s));
      assert.ok(html.includes('data-ui-action="n6RunPr"'), "pr run button must be present");
      const allRun = [...html.matchAll(/data-ui-action="n6RunPr"/g)];
      for (const match of allRun) {
        const context = html.slice(
          Math.max(0, match.index! - 50),
          Math.min(html.length, match.index! + 200),
        );
        assert.ok(
          context.includes('data-n6-token-required="true"'),
          "n6RunPr button missing data-n6-token-required",
        );
      }
    });
  });

  describe("run buttons absent when preconditions not met", () => {
    it("plan run button absent when token not active (even if N5 READY)", () => {
      const s: N6SessionState = { ...emptyN6SessionState(), planExec: "AWAITING_TOKEN" };
      const html = renderHtml(modelWithN6(s, true));
      assert.ok(!html.includes('data-ui-action="n6RunPlan"'), "run button must be hidden without token");
    });

    it("plan run button absent when N5 plan NOT_READY (even if token active)", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planTokenActive: true,
        planExec: "TOKEN_ACTIVE",
      };
      const html = renderHtml(modelWithN6(s, false)); // n5 plan not ready
      assert.ok(!html.includes('data-ui-action="n6RunPlan"'), "run button must be hidden without N5 READY");
    });

    it("commit run button absent when plan not DONE", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "TOKEN_ACTIVE", // NOT done
        commitTokenActive: true,
        commitExec: "TOKEN_ACTIVE",
      };
      const html = renderHtml(modelWithN6(s));
      assert.ok(!html.includes('data-ui-action="n6RunCommit"'), "commit button hidden if plan not DONE");
    });
  });

  describe("exec state classes in HTML", () => {
    it("renders rung exec state in a data-testid span", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(html.includes('data-testid="n6-rung-plan-state"'));
      assert.ok(html.includes('data-testid="n6-rung-commit-state"'));
      assert.ok(html.includes('data-testid="n6-rung-push-state"'));
      assert.ok(html.includes('data-testid="n6-rung-pr-state"'));
    });

    it("renders AWAITING_TOKEN state for all rungs in empty session", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(html.includes("[AWAITING_TOKEN]"), "state label must appear");
    });
  });

  describe("output block", () => {
    it("renders output pre-block after rung DONE", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "DONE",
        planOutput: "plan output here",
        planExitCode: 0,
      };
      const html = renderHtml(modelWithN6(s));
      assert.ok(html.includes('data-testid="n6-output-plan"'), "plan output block must appear");
      assert.ok(html.includes("plan output here"), "output content must appear");
      assert.ok(html.includes('data-testid="n6-exit-plan"'), "exit code block must appear");
    });

    it("does not render output block before rung runs", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(!html.includes('data-testid="n6-output-plan"'), "output block absent before run");
    });
  });

  describe("n6 section renders BELOW n5 section (D3=B)", () => {
    it("n6 section appears after n5 section in the output", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      const n5Pos = html.indexOf('data-testid="n5-readiness-board"');
      const n6Pos = html.indexOf('data-testid="n6-execution-boundary"');
      assert.ok(n5Pos >= 0, "n5 section must be present");
      assert.ok(n6Pos >= 0, "n6 section must be present");
      assert.ok(n6Pos > n5Pos, "n6 section must appear AFTER n5 section (D3=B)");
    });
  });

  describe("rung note text", () => {
    it("renders the step note in the data-testid note span", () => {
      const html = renderHtml(modelWithN6(emptyN6SessionState()));
      assert.ok(html.includes('data-testid="n6-rung-plan-note"'), "plan note span present");
    });

    it("FAILED state note mentions token cleared (D4=B)", () => {
      const s: N6SessionState = {
        ...emptyN6SessionState(),
        planExec: "FAILED",
        planOutput: "non-zero exit",
        planExitCode: 1,
      };
      const html = renderHtml(modelWithN6(s));
      // The note text must mention token clearing (D4=B).
      const noteMatch = html.match(/data-testid="n6-rung-plan-note">([^<]*)</);
      assert.ok(noteMatch, "plan note span must be in HTML");
      const noteText = noteMatch![1].toLowerCase();
      assert.ok(
        noteText.includes("token cleared") || noteText.includes("d4-b"),
        `FAILED note must mention D4=B token clearing: "${noteMatch![1]}"`,
      );
    });
  });
});
