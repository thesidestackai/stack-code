// Pure HTML renderer for the A2 Harness Panel. Given the current panel state
// (operator-selected inputs + the most recent helper output), it returns the
// full webview HTML. It renders ONLY the safe button catalog, an output area
// that shows helper stdout verbatim, and an always-visible Safety / Stop Gates
// section. It computes no chain state of its own — the helper's stdout (e.g.
// audit-workspace's next-step hint) is the state surface, rendered as text.

import {
  PANEL_BUTTONS,
  PanelButton,
  HelperButton,
} from "./buttons";

export interface PanelInputs {
  workspace: string | null;
  plan: string | null;
  previewBundle: string | null;
  generatorResult: string | null;
  approvalResult: string | null;
  approvalOutput: string | null;
  applyBundle: string | null;
  target: string | null;
  afterSha: string | null;
}

export interface HelperOutput {
  subcommand: string;
  exitCode: number;
  stdout: string;
  stderr: string;
}

export interface RenderModel {
  inputs: PanelInputs;
  output: HelperOutput | null;
  // A non-load-bearing status line (e.g. "refused: workspace not set"). Never
  // a substitute for the helper stdout or the Safety section.
  notice: string | null;
}

export function emptyInputs(): PanelInputs {
  return {
    workspace: null,
    plan: null,
    previewBundle: null,
    generatorResult: null,
    approvalResult: null,
    approvalOutput: null,
    applyBundle: null,
    target: null,
    afterSha: null,
  };
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function inputRow(label: string, key: string, value: string | null): string {
  const shown = value === null || value === "" ? "(not set)" : value;
  return `    <tr data-input="${escapeHtml(key)}"><th>${escapeHtml(label)}</th><td data-input-value="${escapeHtml(key)}">${escapeHtml(shown)}</td></tr>`;
}

function buttonHtml(b: PanelButton): string {
  if (b.kind === "ui") {
    return `    <button class="btn ui" data-ui-action="${escapeHtml(b.action)}" data-button-id="${escapeHtml(b.id)}">${escapeHtml(b.label)}</button>`;
  }
  const hb = b as HelperButton;
  return `    <button class="btn helper" data-subcommand="${escapeHtml(hb.subcommand)}" data-button-id="${escapeHtml(hb.id)}">${escapeHtml(hb.label)}</button>`;
}

function outputBlock(output: HelperOutput | null): string {
  if (output === null) {
    return `<section class="output" data-testid="output">
  <h3>Helper output</h3>
  <p class="muted" data-testid="output-empty">No helper subcommand has been run yet. Every action runs one read-only/print helper subcommand.</p>
</section>`;
  }
  const cls = output.exitCode === 0 ? "ok" : "nonzero";
  return `<section class="output ${cls}" data-testid="output">
  <h3>Helper output — <code>${escapeHtml(output.subcommand)}</code> (exit ${escapeHtml(String(output.exitCode))})</h3>
  <button class="btn copy" data-copy-output="true" data-testid="copy-output">Copy helper output</button>
  <pre data-testid="output-stdout">${escapeHtml(output.stdout)}</pre>
${output.stderr.trim().length > 0 ? `  <details><summary>stderr</summary><pre data-testid="output-stderr">${escapeHtml(output.stderr)}</pre></details>` : ""}
</section>`;
}

// The Safety / Stop Gates section is always rendered and never collapsed. It
// states the invariants the panel holds and the conditions under which the
// operator must stop. It is the panel's loud, persistent safety surface.
function safetyBlock(): string {
  return `<section class="safety" data-testid="safety-gates">
  <h3>Safety / Stop Gates (always on)</h3>
  <ul>
    <li>This panel only runs read-only / print helper subcommands. It never runs <code>claw plan run / approve / apply-bundle / apply</code>.</li>
    <li>No Run Preview / Run Approval / Run Apply-Bundle / Run Apply button exists. The panel shows/copies commands; it does not execute them.</li>
    <li>Approval is human, at a REAL terminal. The panel never composes the approval line and never captures it from this webview.</li>
    <li>apply-bundle is the generator (writes no target). <code>claw plan apply</code> is the only command that writes the target, run once, by you, at your terminal.</li>
    <li>No auto-approval. No hidden apply. No model / broker / runtime / :11434 call. No secrets. No filesystem watching or polling.</li>
    <li>STOP if: a preview/approval-result is missing, a hash mismatches, the target drifted, a prior apply marker exists, or an after_file is absolute / unreviewed.</li>
  </ul>
</section>`;
}

export function renderHtml(model: RenderModel): string {
  const i = model.inputs;
  const inputRows = [
    inputRow("workspace", "workspace", i.workspace),
    inputRow("plan", "plan", i.plan),
    inputRow("preview-bundle", "preview-bundle", i.previewBundle),
    inputRow("generator-result", "preview-generator-result", i.generatorResult),
    inputRow("approval-result", "approval-result", i.approvalResult),
    inputRow("approval-output", "approval-output", i.approvalOutput),
    inputRow("apply-bundle", "apply-bundle", i.applyBundle),
    inputRow("target", "target", i.target),
    inputRow("after-sha", "after-sha", i.afterSha),
  ].join("\n");

  const buttons = PANEL_BUTTONS.map(buttonHtml).join("\n");
  const notice = model.notice
    ? `<section class="notice" data-testid="notice"><p>${escapeHtml(model.notice)}</p></section>`
    : "";

  return `<!doctype html>
<html><head><meta charset="utf-8"><title>A2 Harness Panel</title>
<style>
  body { font-family: var(--vscode-font-family, sans-serif); padding: 1rem; }
  h2 { margin-top: 0; }
  .safety { background: var(--vscode-inputValidation-warningBackground, #4d3800); color: var(--vscode-inputValidation-warningForeground, #fff); border: 2px solid var(--vscode-inputValidation-warningBorder, #b89500); padding: 0.75rem 1rem; font-weight: 500; }
  .safety code { font-family: monospace; }
  .inputs table { border-collapse: collapse; }
  .inputs th { text-align: left; padding-right: 1rem; font-family: monospace; }
  .inputs td { font-family: monospace; user-select: all; }
  .output pre { background: var(--vscode-textCodeBlock-background, #1e1e1e); padding: 0.75rem; white-space: pre-wrap; word-break: break-word; }
  .output.nonzero h3 { color: var(--vscode-editorError-foreground, #f48771); }
  .muted { color: var(--vscode-descriptionForeground, #999); }
  .btn { margin: 0.2rem; }
  .actions { margin: 0.5rem 0; }
</style>
</head><body data-testid="a2-harness-panel">
<h2>A2 Harness Panel</h2>
<p class="muted">Visual driver for the print/validate-only A2 IDE harness v0. Each button runs one read-only/print helper subcommand or copies its printed command — nothing executes the A2 chain.</p>
${safetyBlock()}
<section class="inputs" data-testid="inputs">
  <h3>Workspace / Plan / Artifact selection</h3>
  <table>
${inputRows}
  </table>
</section>
<section class="actions" data-testid="actions">
  <h3>Actions</h3>
${buttons}
</section>
${notice}
${outputBlock(model.output)}
</body></html>`;
}
