import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel, ExecutorDryRunView } from "../src/render";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function sampleView(): ExecutorDryRunView {
  return {
    printedCommand: "a2-mutation-executor --dry-run --approved-lane <approved-lane.json>  # external; operator-run; NO worktree creation, NO writes",
    resultLines: [
      "ready: no",
      "readiness: not-ready",
      "plan valid: no",
      "would create worktree: no",
      "would write files: no",
    ],
    summary: "dry-run: lane is NOT ready to proceed — NO creation, NO writes performed",
    wouldCreateWorktree: false,
    wouldWriteFiles: false,
    evidenceLines: ["[0] Tier 3 decision/info: dry-run computed (no approved lane) [printed-not-run] — ..."],
  };
}

describe("executorDryRun render — section present + read-only", () => {
  it("renders the Proposed Executor Plan section with command + result + evidence", () => {
    const html = renderHtml({ ...baseModel(), executorDryRun: sampleView() });
    assert.ok(html.includes('data-testid="executor-dryrun"'));
    assert.ok(html.includes('data-testid="executor-dryrun-command"'));
    assert.ok(html.includes('data-testid="executor-dryrun-result"'));
    assert.ok(html.includes('data-testid="executor-dryrun-evidence"'));
    assert.ok(/Proposed Executor Plan/.test(html));
    assert.ok(/--dry-run/.test(html));
  });

  it("shows would-create/would-write as no (dry-run creates/writes nothing)", () => {
    const html = renderHtml({ ...baseModel(), executorDryRun: sampleView() });
    assert.ok(/would create worktree:\s*<code>no<\/code>/.test(html));
    assert.ok(/would write files:\s*<code>no<\/code>/.test(html));
  });

  it("renders a muted placeholder when no dry-run view is provided", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="executor-dryrun"'));
    assert.ok(html.includes('data-testid="executor-dryrun-empty"'));
  });

  it("adds NO executor/create/write/apply/approve action control", () => {
    const html = renderHtml({ ...baseModel(), executorDryRun: sampleView() });
    assert.ok(!/data-ui-action="runExecutor"/.test(html));
    assert.ok(!/data-ui-action="createWorktree"/.test(html));
    assert.ok(!/data-ui-action="applyMutation"/.test(html));
    assert.ok(!/>\s*Run Executor\s*</.test(html));
    assert.ok(!/>\s*Create Worktree\s*</.test(html));
    assert.ok(!/>\s*Apply\s*</.test(html));
  });

  it("keeps the field-setter ordering invariant with the dry-run section present", () => {
    const html = renderHtml({ ...baseModel(), executorDryRun: sampleView() });
    const inputsIdx = html.indexOf('data-testid="inputs"');
    const actionsIdx = html.indexOf('data-testid="actions"');
    const targetIdx = html.indexOf('data-ui-action="selectTarget"');
    assert.ok(targetIdx > inputsIdx && targetIdx < actionsIdx);
  });
});
