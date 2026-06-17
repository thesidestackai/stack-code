import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

describe("northstar render — workspace status card (Phase N2)", () => {
  it("degrades to a muted hint when no card view is present", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="workspace-card"'));
    assert.ok(html.includes('data-testid="workspace-card-empty"'));
  });

  it("renders the auto-detected card lines and an honest git-probe note", () => {
    const html = renderHtml({
      ...baseModel(),
      workspaceCard: {
        lines: ["workspace: detected", "workspace root: /ws", "branch: unknown", "readiness: unknown"],
        gitProbeNote: "branch / clean-dirty / origin-main freshness need a read-only git probe",
      },
    });
    assert.ok(html.includes("workspace: detected"));
    assert.ok(html.includes("workspace root: /ws"));
    assert.ok(html.includes('data-testid="workspace-card-git-note"'));
    assert.ok(/not-checked/.test(html));
  });
});

describe("northstar render — state-model view (Phase N2)", () => {
  it("is absent (no section) when no view is provided", () => {
    const html = renderHtml(baseModel());
    assert.ok(!html.includes('data-testid="northstar-state"'));
  });

  it("renders the state + the single recommended next safe step", () => {
    const html = renderHtml({
      ...baseModel(),
      northstar: {
        state: "AWAITING_APPLY_APPROVAL",
        stateClass: "read-only",
        stepLabel: "Approve the apply at a REAL terminal (human-typed)",
        stepKind: "human-gated",
        automatable: false,
        requiresRealTty: true,
      },
    });
    assert.ok(html.includes('data-testid="northstar-state"'));
    assert.ok(html.includes("AWAITING_APPLY_APPROVAL"));
    assert.ok(html.includes('data-testid="northstar-next-step"'));
    assert.ok(html.includes("automatable=no"));
    assert.ok(html.includes('data-testid="northstar-gate"'));
  });

  it("adds NO executor / run / apply / merge control in either Northstar section", () => {
    const html = renderHtml({
      ...baseModel(),
      workspaceCard: { lines: ["workspace: detected"], gitProbeNote: null },
      northstar: {
        state: "PUSHED",
        stateClass: "human-gated",
        stepLabel: "Open exactly one DRAFT PR (human-gated)",
        stepKind: "human-gated",
        automatable: false,
        requiresRealTty: false,
      },
    });
    // No button/control elements introduced by the Northstar sections.
    assert.ok(!/data-ui-action="(run|apply|approve|merge|packageCommit|packagePush|packagePr)"/i.test(html));
    assert.ok(!/<button[^>]*northstar/i.test(html));
    assert.ok(!/<button[^>]*workspace-card/i.test(html));
  });
});
