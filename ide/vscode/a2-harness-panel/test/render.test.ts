import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { renderHtml, emptyInputs, RenderModel, HelperOutput } from "../src/render";
import { FIXTURES_DIR } from "./_paths";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

describe("render — structure", () => {
  it("renders the panel shell and the action buttons", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="a2-harness-panel"'));
    assert.ok(html.includes('data-testid="actions"'));
    assert.ok(html.includes('data-subcommand="audit-workspace"'));
    assert.ok(html.includes('data-subcommand="print-approval"'));
    assert.ok(html.includes('data-ui-action="openRunbook"'));
    assert.ok(html.includes('data-ui-action="exportEvidence"'));
  });

  it("always renders the Safety / Stop Gates section", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="safety-gates"'));
    assert.ok(/Safety \/ Stop Gates/.test(html));
  });

  it("has NO Run-* execution button in the markup", () => {
    const html = renderHtml(baseModel());
    assert.ok(!/>\s*Run Preview\s*</.test(html));
    assert.ok(!/>\s*Run Approval\s*</.test(html));
    assert.ok(!/>\s*Run Apply-Bundle\s*</.test(html));
    assert.ok(!/>\s*Run Apply\s*</.test(html));
  });

  it("renders a field-setter control for every artifact/hash field", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="field-setters"'));
    for (const action of [
      "selectTarget",
      "setAfterSha",
      "selectPreviewBundle",
      "selectGeneratorResult",
      "selectApprovalResult",
      "selectApprovalOutput",
      "selectApplyBundle",
    ]) {
      assert.ok(
        html.includes(`data-ui-action="${action}"`),
        `missing field-setter control in markup: ${action}`,
      );
    }
  });

  it("places field-setter controls inside the inputs section (next to fields)", () => {
    const html = renderHtml(baseModel());
    const inputsIdx = html.indexOf('data-testid="inputs"');
    const actionsIdx = html.indexOf('data-testid="actions"');
    const targetIdx = html.indexOf('data-ui-action="selectTarget"');
    assert.ok(inputsIdx >= 0 && actionsIdx >= 0 && targetIdx >= 0);
    // The Select Target control renders within the inputs section, before Actions.
    assert.ok(targetIdx > inputsIdx && targetIdx < actionsIdx);
  });
});

describe("render — helper output", () => {
  it("renders audit-workspace fixture stdout verbatim (escaped)", () => {
    const stdout = fs.readFileSync(
      path.join(FIXTURES_DIR, "audit_workspace_preview_ready.txt"),
      "utf8",
    );
    const output: HelperOutput = {
      subcommand: "audit-workspace",
      exitCode: 0,
      stdout,
      stderr: "",
    };
    const html = renderHtml({ inputs: emptyInputs(), output, notice: null });
    assert.ok(html.includes('data-testid="output-stdout"'));
    assert.ok(html.includes("chain state: preview-ready"));
    assert.ok(html.includes("Next: print-approval"));
    assert.ok(html.includes('data-testid="copy-output"'));
  });

  it("renders a nonzero exit loudly (error styling hook present)", () => {
    const output: HelperOutput = {
      subcommand: "validate-input",
      exitCode: 3,
      stdout: "validate-input: FAILED",
      stderr: "a2-ide-harness.sh: ERROR: absolute after_file path is not allowed",
    };
    const html = renderHtml({ inputs: emptyInputs(), output, notice: null });
    assert.ok(html.includes("exit 3"));
    assert.ok(html.includes('class="output nonzero"'));
    assert.ok(html.includes('data-testid="output-stderr"'));
  });

  it("escapes HTML metacharacters in helper stdout", () => {
    const output: HelperOutput = {
      subcommand: "print-apply",
      exitCode: 0,
      stdout: "<script>alert(1)</script> & 'quote'",
      stderr: "",
    };
    const html = renderHtml({ inputs: emptyInputs(), output, notice: null });
    assert.ok(!html.includes("<script>alert(1)</script>"));
    assert.ok(html.includes("&lt;script&gt;"));
  });
});

describe("render — inputs + notice", () => {
  it("shows (not set) for unset inputs and the value when set", () => {
    const inputs = emptyInputs();
    inputs.workspace = "/disposable/wks";
    const html = renderHtml({ inputs, output: null, notice: null });
    assert.ok(html.includes("/disposable/wks"));
    assert.ok(html.includes("(not set)"));
  });

  it("renders a notice when present without hiding the safety section", () => {
    const html = renderHtml({ inputs: emptyInputs(), output: null, notice: "Set a workspace first." });
    assert.ok(html.includes('data-testid="notice"'));
    assert.ok(html.includes("Set a workspace first."));
    assert.ok(html.includes('data-testid="safety-gates"'));
  });
});
