import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel, N4PanelView } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function sampleN4(over: Partial<N4PanelView> = {}): N4PanelView {
  return {
    state: "N4_EVIDENCE_READY",
    stepLabel: "Evidence ready (read-only).",
    isBlocked: false,
    preview: { trust: "VERIFIED", lines: ["task: tidy", "step: Review intent"] },
    diff: { trust: "VERIFIED", lines: ["declared paths: src/a.ts", "not_executable_reason: review artifact only"] },
    evidence: { trust: "VERIFIED", lines: ["evidence: boundary check", "validation: PLAN_DRAFT_VALIDATED"] },
    ...over,
  };
}

describe("n4 render — viewer section", () => {
  it("degrades to a muted hint when no view is present", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="n4-review"'));
    assert.ok(html.includes('data-testid="n4-empty"'));
  });

  it("renders state, next step, and the three trust-labelled facets", () => {
    const html = renderHtml({ ...baseModel(), n4: sampleN4() });
    assert.ok(html.includes('data-testid="n4-state"'));
    assert.ok(html.includes("N4_EVIDENCE_READY"));
    assert.ok(html.includes('data-testid="n4-preview-trust"'));
    assert.ok(html.includes('data-testid="n4-diff-lines"'));
    assert.ok(html.includes("not_executable_reason: review artifact only"));
    assert.ok(html.includes('data-testid="n4-evidence-trust"'));
  });

  it("a BLOCKED facet renders NO content (fail closed)", () => {
    const html = renderHtml({
      ...baseModel(),
      n4: sampleN4({
        state: "N4_BLOCKED_UNSAFE_TARGET",
        isBlocked: true,
        preview: { trust: "BLOCKED", lines: [] },
        diff: { trust: "BLOCKED", lines: [] },
        evidence: { trust: "BLOCKED", lines: [] },
      }),
    });
    assert.ok(html.includes("N4_BLOCKED_UNSAFE_TARGET"));
    assert.ok(html.includes('data-testid="n4-preview-empty"'));
    assert.ok(html.includes('data-testid="n4-evidence-empty"'));
    // No leaked content lines from a blocked facet.
    assert.ok(!html.includes('data-testid="n4-preview-lines"'));
  });

  it("adds NO control / apply / package / PR / run button anywhere in the N4 section", () => {
    const html = renderHtml({ ...baseModel(), n4: sampleN4() });
    // The viewer has no buttons at all in its section markup.
    assert.ok(!/data-ui-action="(n4|run|apply|approve|merge|package)[A-Za-z]*"/i.test(html));
    assert.ok(!/<button[^>]*n4-/i.test(html));
  });
});
