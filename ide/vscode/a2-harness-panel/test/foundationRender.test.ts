import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel, FoundationView } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function sampleFoundation(): FoundationView {
  return {
    currentTier: 1,
    readiness: {
      rows: [
        { label: "workspace root", value: "detected" },
        { label: "repo detected", value: "not-checked" },
        { label: "git branch", value: "not-checked" },
        { label: "dirty checkout", value: "not-checked" },
        { label: "current tier", value: "Tier 1" },
        { label: "denied registry loaded", value: "yes" },
        { label: "safe executor mode", value: "print-validate-only" },
      ],
      dirtyWarning: false,
      gitProbeNote: "no guard-safe git probe wired in v0",
    },
    tiers: [
      { id: 0, name: "Observe Only", current: false, deniedByDefault: false, requiresExplicitApproval: false, summary: "observe" },
      { id: 1, name: "Print Commands Only", current: true, deniedByDefault: false, requiresExplicitApproval: false, summary: "print" },
      { id: 2, name: "Safe Read-Only Execution", current: false, deniedByDefault: false, requiresExplicitApproval: false, summary: "read-only" },
      { id: 3, name: "Disposable Worktree Mutation", current: false, deniedByDefault: false, requiresExplicitApproval: true, summary: "mutate" },
      { id: 4, name: "PR Packaging", current: false, deniedByDefault: false, requiresExplicitApproval: true, summary: "package" },
      { id: 5, name: "Runtime / Model / Service Actions", current: false, deniedByDefault: true, requiresExplicitApproval: true, summary: "external" },
    ],
    deniedFamilies: ["destructive filesystem cleanup", "live A2 chain execution", "model/broker call"],
    ledgerLines: ["[0] Tier 1 session/info: open agent cockpit — read-only"],
    nextLane: {
      name: "A2 Local Coding Agent Foundation v0 Review / Push PR",
      summary: "review then push",
      mutationEnabled: false,
      blocked: ["file editing by the panel", "PR creation by the panel"],
    },
  };
}

describe("foundation render — sections present", () => {
  it("renders all five foundation sections when a foundation view is provided", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    assert.ok(html.includes('data-testid="agent-foundation"'));
    assert.ok(html.includes('data-testid="permission-tier"'));
    assert.ok(html.includes('data-testid="agent-readiness"'));
    assert.ok(html.includes('data-testid="denied-command-registry"'));
    assert.ok(html.includes('data-testid="agent-evidence-ledger"'));
    assert.ok(html.includes('data-testid="proposed-next-agent-lane"'));
  });

  it("includes the required foundation vocabulary", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    assert.ok(/Agent Readiness/.test(html));
    assert.ok(/Permission Tier/.test(html));
    assert.ok(/Denied Command Registry/.test(html));
    assert.ok(/Agent Evidence Ledger/.test(html));
    assert.ok(/Proposed Next Agent Lane/.test(html));
    assert.ok(/Foundation v0/.test(html));
    for (const id of [0, 1, 2, 3, 4, 5]) {
      assert.ok(html.includes(`Tier ${id}`), `missing Tier ${id}`);
    }
  });

  it("marks the current tier and flags Tier 5 denied-by-default", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    assert.ok(html.includes('data-current-tier="true"'));
    assert.ok(/data-tier="5"[\s\S]*denied-by-default/.test(html));
  });

  it("renders git readiness as not-checked rather than fabricating green", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    assert.ok(html.includes('data-testid="git-probe-note"'));
    assert.ok(/Git readiness: not-checked/.test(html));
    assert.ok(/data-readiness-value="dirty checkout"[\s\S]*?not-checked/.test(html));
  });

  it("shows that no mutation lane is enabled in v0", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    assert.ok(html.includes('data-testid="mutation-enabled"'));
    assert.ok(/Mutation enabled:\s*<code>no<\/code>/.test(html));
    assert.ok(/No mutation lane is enabled in v0/.test(html));
    assert.ok(/No autonomous source edits/.test(html));
  });

  it("renders a muted placeholder when no foundation view is provided", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="agent-foundation"'));
    assert.ok(html.includes('data-testid="agent-foundation-empty"'));
  });
});

describe("foundation render — no action controls", () => {
  it("adds NO agent/execute/run/approve/apply action buttons", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    // No new buttons are introduced by the foundation sections.
    assert.ok(!/data-ui-action="runAgent"/.test(html));
    assert.ok(!/data-ui-action="executeAgent"/.test(html));
    assert.ok(!/>\s*Run Agent\s*</.test(html));
    assert.ok(!/>\s*Execute Agent\s*</.test(html));
    assert.ok(!/>\s*Run Preview\s*</.test(html));
    assert.ok(!/>\s*Run Apply\s*</.test(html));
    assert.ok(!/>\s*Approve Now\s*</.test(html));
    assert.ok(!/>\s*Apply Now\s*</.test(html));
  });

  it("keeps the field-setter ordering invariant with the foundation present", () => {
    const html = renderHtml({ ...baseModel(), foundation: sampleFoundation() });
    const inputsIdx = html.indexOf('data-testid="inputs"');
    const actionsIdx = html.indexOf('data-testid="actions"');
    const targetIdx = html.indexOf('data-ui-action="selectTarget"');
    assert.ok(targetIdx > inputsIdx && targetIdx < actionsIdx);
  });
});
