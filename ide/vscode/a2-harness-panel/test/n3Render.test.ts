import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel, N3PanelView } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function sampleN3(over: Partial<N3PanelView> = {}): N3PanelView {
  return {
    state: "PLAN_DRAFT_VALIDATED",
    stepLabel: "STOP — draft validated. A future N4 preview/diff lane comes next.",
    isBlocked: false,
    isTerminal: true,
    riskLevel: "SOURCE_EDIT",
    riskDisposition: "requires-future-lane",
    intakeLines: ["task: tidy", "draft status: validated"],
    planDraftLines: ["draft_id: t1-draft", "not_executable_reason: review artifact only"],
    lintStatus: "PLAN_DRAFT_VALIDATED",
    lintReasons: [],
    ...over,
  };
}

describe("n3 render — task intake section", () => {
  it("degrades to a muted hint with controls when no view is present", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="n3-task-intake"'));
    assert.ok(html.includes('data-testid="n3-empty"'));
    assert.ok(html.includes('data-ui-action="n3DescribeTask"'));
  });

  it("renders the state, risk badge, intake lines, plan draft, and lint result", () => {
    const html = renderHtml({ ...baseModel(), n3: sampleN3() });
    assert.ok(html.includes('data-testid="n3-state"'));
    assert.ok(html.includes("PLAN_DRAFT_VALIDATED"));
    assert.ok(html.includes("SOURCE_EDIT"));
    assert.ok(html.includes('data-testid="n3-plan-draft"'));
    assert.ok(html.includes("not_executable_reason: review artifact only"));
    assert.ok(html.includes('data-testid="n3-lint-status"'));
  });

  it("renders lint reasons when a draft is blocked", () => {
    const html = renderHtml({
      ...baseModel(),
      n3: sampleN3({
        state: "PLAN_DRAFT_BLOCKED",
        isBlocked: true,
        lintStatus: "PLAN_DRAFT_BLOCKED",
        lintReasons: ["risk_level RUNTIME_CONFIG is a STOP"],
      }),
    });
    assert.ok(html.includes("PLAN_DRAFT_BLOCKED"));
    assert.ok(html.includes('data-testid="n3-lint-reasons"'));
    assert.ok(html.includes("RUNTIME_CONFIG is a STOP"));
  });

  it("adds NO apply/package/PR/run/merge control in the N3 section", () => {
    const html = renderHtml({ ...baseModel(), n3: sampleN3() });
    assert.ok(!/data-ui-action="(run|apply|approve|merge|package|packagePr|packageCommit|packagePush|openPr|openDraftPr)"/i.test(html));
    // The only N3 controls are capture/draft/reset.
    for (const a of ["n3DescribeTask", "n3AddDeclaredPath", "n3AddForbiddenPath", "n3DraftPlan", "n3Reset"]) {
      assert.ok(html.includes(`data-ui-action="${a}"`), `missing control ${a}`);
    }
  });

  it("still has no Run-* execution button anywhere with N3 present", () => {
    const html = renderHtml({ ...baseModel(), n3: sampleN3() });
    assert.ok(!/>\s*Run Preview\s*</.test(html));
    assert.ok(!/>\s*Run Apply\s*</.test(html));
  });
});
