import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel, N5PanelView, N5RungPanelView } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function sampleRung(over: Partial<N5RungPanelView> = {}): N5RungPanelView {
  return {
    rung: "package-plan",
    purpose: "assemble the change package",
    readiness: "READY",
    preconditionLines: ["[VERIFIED] N4 preview/diff VERIFIED: met", "[VERIFIED] target safe: met"],
    evidencePresent: true,
    operatorConfirmationRequired: false,
    note: "Ready for a separate approved execution lane. N5 does not run this rung.",
    ...over,
  };
}

function sampleN5(over: Partial<N5PanelView> = {}): N5PanelView {
  return {
    state: "N5_PACKAGE_PLAN_READY",
    stepLabel:
      "package-plan is READY (all preconditions VERIFIED). Ready for a separately-approved execution lane. N5 does not run it.",
    isBlocked: false,
    n4State: "N4_EVIDENCE_READY",
    n4StepLabel: "Evidence ready (read-only). A future, separately-approved N5 lane handles gated execution.",
    taskSummary: "tidy source",
    riskLevel: "SOURCE_EDIT",
    ladder: [
      sampleRung(),
      sampleRung({
        rung: "package-commit",
        purpose: "commit the assembled package",
        readiness: "EXECUTION_REQUIRED",
        preconditionLines: ["[EXECUTION_REQUIRED] previous rung completed: not met"],
        operatorConfirmationRequired: true,
        note: "EXECUTION_REQUIRED — cannot be proven from read-only data. Requires a separate approved execution lane.",
      }),
      sampleRung({
        rung: "package-push",
        purpose: "push the branch",
        readiness: "EXECUTION_REQUIRED",
        preconditionLines: ["[EXECUTION_REQUIRED] previous rung completed: not met"],
        operatorConfirmationRequired: true,
        note: "EXECUTION_REQUIRED — cannot be proven from read-only data. Requires a separate approved execution lane.",
      }),
      sampleRung({
        rung: "package-pr",
        purpose: "open the change PR",
        readiness: "EXECUTION_REQUIRED",
        preconditionLines: ["[EXECUTION_REQUIRED] previous rung completed: not met"],
        operatorConfirmationRequired: true,
        note: "EXECUTION_REQUIRED — requires a separate approved execution lane.",
      }),
    ],
    ...over,
  };
}

describe("n5 render — readiness board section", () => {
  it("degrades to a muted hint when no view is present", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="n5-readiness-board"'));
    assert.ok(html.includes('data-testid="n5-empty"'));
  });

  it("renders state, next step, and per-rung sections", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes('data-testid="n5-readiness-board"'));
    assert.ok(html.includes('data-testid="n5-state"'));
    assert.ok(html.includes("N5_PACKAGE_PLAN_READY"));
    assert.ok(html.includes('data-testid="n5-next-step"'));
    assert.ok(html.includes('data-testid="n5-rung-package-plan"'));
    assert.ok(html.includes('data-testid="n5-rung-package-commit"'));
    assert.ok(html.includes('data-testid="n5-rung-package-push"'));
    assert.ok(html.includes('data-testid="n5-rung-package-pr"'));
  });

  it("renders READY rung with readiness label", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes('data-testid="n5-rung-package-plan-readiness"'));
    assert.ok(html.includes("[READY]"));
  });

  it("renders EXECUTION_REQUIRED rungs clearly", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes("[EXECUTION_REQUIRED]"));
    assert.ok(html.includes('data-testid="n5-rung-package-commit-readiness"'));
  });

  it("renders N4 state context", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes('data-testid="n5-n4-state"'));
    assert.ok(html.includes("N4_EVIDENCE_READY"));
  });

  it("renders task summary and risk level", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes('data-testid="n5-context"'));
    assert.ok(html.includes("tidy source"));
    assert.ok(html.includes("SOURCE_EDIT"));
  });

  it("muted footer says 'separately-approved execution lane' and 'runs no package'", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.toLowerCase().includes("separately-approved execution lane"));
    assert.ok(html.toLowerCase().includes("runs no package-plan"));
  });

  it("READY rung note says 'separate approved' (never 'run now')", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.toLowerCase().includes("separate approved"));
    assert.ok(!html.toLowerCase().includes("run now"));
  });

  it("adds NO control / apply / package / run / PR button anywhere in the N5 section", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(!/data-ui-action="(n5|run|apply|approve|merge|package)[A-Za-z]*"/i.test(html));
    assert.ok(!/<button[^>]*n5-/i.test(html));
  });

  it("a BLOCKED view renders blocked state without actionable content", () => {
    const blocked: N5PanelView = sampleN5({
      state: "N5_BLOCKED_UNSAFE_TARGET",
      stepLabel: "STOP — a declared target is unsafe. Fail closed.",
      isBlocked: true,
    });
    const html = renderHtml({ ...baseModel(), n5: blocked });
    assert.ok(html.includes("N5_BLOCKED_UNSAFE_TARGET"));
    assert.ok(html.includes("STOP"));
    assert.ok(!/<button[^>]*n5-/i.test(html));
  });

  it("renders all four rung purpose lines", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes('data-testid="n5-rung-package-plan-purpose"'));
    assert.ok(html.includes("assemble the change package"));
    assert.ok(html.includes('data-testid="n5-rung-package-commit-purpose"'));
  });

  it("renders precondition lines with trust labels", () => {
    const html = renderHtml({ ...baseModel(), n5: sampleN5() });
    assert.ok(html.includes("[VERIFIED] N4 preview/diff VERIFIED: met"));
    assert.ok(html.includes("[EXECUTION_REQUIRED] previous rung completed: not met"));
  });
});
