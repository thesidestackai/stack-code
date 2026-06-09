import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel, Tier3View } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function sampleTier3(): Tier3View {
  return {
    readinessRows: [
      { label: "control checkout clean", value: "not-checked" },
      { label: "origin/main confirmed", value: "not-checked" },
      { label: "worktree path free", value: "not-checked" },
      { label: "branch name free", value: "not-checked" },
      { label: "operator approved", value: "not-checked" },
      { label: "plan valid", value: "no" },
      { label: "declared scope present", value: "no" },
      { label: "denied registry loaded", value: "yes" },
    ],
    overall: "not-ready",
    dirtyControlCheckoutBlock: false,
    probeNote: "no guard-safe Tier 3 probe wired in v0",
    planLines: ["worktree: (not set)", "branch: (not set)", "base: (not set)", "creation: not performed (plan only in v0)"],
    planValid: false,
    planProblems: ["no worktree plan provided"],
    declaredPaths: [],
    policyInvariant: "Denials win over the Tier-3 allowlist; writes are limited to the declared exact-path set inside the disposable worktree; nothing executes in v0.",
    ledgerLines: ["(no Tier 3 mutation-lane gestures recorded yet)"],
    operatorApproved: false,
  };
}

describe("tier3 render — sections present", () => {
  it("renders all Tier 3 sections when a view is provided", () => {
    const html = renderHtml({ ...baseModel(), tier3: sampleTier3() });
    for (const id of [
      "tier3-foundation",
      "tier3-readiness",
      "tier3-worktree-plan",
      "tier3-declared-files",
      "tier3-approval-gate",
      "tier3-diff-summary",
      "tier3-validation-results",
      "tier3-rollback",
      "tier3-mutation-ledger",
    ]) {
      assert.ok(html.includes(`data-testid="${id}"`), `missing section ${id}`);
    }
  });

  it("includes required Tier 3 vocabulary", () => {
    const html = renderHtml({ ...baseModel(), tier3: sampleTier3() });
    assert.ok(/Tier 3/.test(html));
    assert.ok(/Disposable Worktree/.test(html));
    assert.ok(/Declared Touched Files/.test(html));
    assert.ok(/Mutation Approval Gate/.test(html));
    assert.ok(/Mutation Evidence Ledger/.test(html));
    assert.ok(/Rollback \/ Abandon/.test(html));
  });

  it("renders Tier 3 readiness as not-checked, not-ready (never fabricated green)", () => {
    const html = renderHtml({ ...baseModel(), tier3: sampleTier3() });
    assert.ok(html.includes('data-testid="tier3-probe-note"'));
    assert.ok(/Tier 3 readiness: not-checked/.test(html));
    assert.ok(/Overall:\s*<code>not-ready<\/code>/.test(html));
    assert.ok(/data-tier3-readiness-value="control checkout clean"[\s\S]*?not-checked/.test(html));
  });

  it("shows the plan is not created and operator has not approved", () => {
    const html = renderHtml({ ...baseModel(), tier3: sampleTier3() });
    assert.ok(/creation: not performed/.test(html));
    assert.ok(/Plan valid:\s*<code>no<\/code>/.test(html));
    assert.ok(/Operator approved this exact lane:\s*<code>no<\/code>/.test(html));
  });

  it("renders a muted placeholder when no Tier 3 view is provided", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="tier3-foundation"'));
    assert.ok(html.includes('data-testid="tier3-foundation-empty"'));
  });

  it("shows a hard block when the control checkout is dirty", () => {
    const html = renderHtml({ ...baseModel(), tier3: { ...sampleTier3(), dirtyControlCheckoutBlock: true } });
    assert.ok(html.includes('data-testid="tier3-dirty-block"'));
  });
});

describe("tier3 render — no mutation/executor/create controls", () => {
  it("adds NO write/create/executor/agent-run/apply/approve control", () => {
    const html = renderHtml({ ...baseModel(), tier3: sampleTier3() });
    assert.ok(!/data-ui-action="createWorktree"/.test(html));
    assert.ok(!/data-ui-action="applyMutation"/.test(html));
    assert.ok(!/data-ui-action="runAgent"/.test(html));
    assert.ok(!/>\s*Create Worktree\s*</.test(html));
    assert.ok(!/>\s*Apply Mutation\s*</.test(html));
    assert.ok(!/>\s*Run Agent\s*</.test(html));
    assert.ok(!/>\s*Approve Now\s*</.test(html));
  });

  it("keeps the field-setter ordering invariant with Tier 3 present", () => {
    const html = renderHtml({ ...baseModel(), tier3: sampleTier3() });
    const inputsIdx = html.indexOf('data-testid="inputs"');
    const actionsIdx = html.indexOf('data-testid="actions"');
    const targetIdx = html.indexOf('data-ui-action="selectTarget"');
    assert.ok(targetIdx > inputsIdx && targetIdx < actionsIdx);
  });
});
