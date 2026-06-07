import * as path from "path";
import * as vscode from "vscode";
import { A2HarnessPanel, PanelMessage } from "./panel";
import {
  RenderModel,
  PanelInputs,
  HelperOutput,
  emptyInputs,
  renderHtml,
} from "./render";
import {
  ALLOWED_FLAGS,
  HelperSubcommand,
  HelperInvocation,
  defaultSpawnImpl,
  runHelper,
  ALLOWED_SUBCOMMANDS,
} from "./helperRunner";
import { PANEL_BUTTONS, HelperButton } from "./buttons";

// Maps a helper flag name to the PanelInputs key that supplies its value.
const FLAG_TO_INPUT: Record<string, keyof PanelInputs> = {
  "workspace": "workspace",
  "plan": "plan",
  "preview-bundle": "previewBundle",
  "preview-generator-result": "generatorResult",
  "approval-result": "approvalResult",
  "approval-output": "approvalOutput",
  "apply-bundle": "applyBundle",
  "target": "target",
  "after-sha": "afterSha",
};

interface SessionState {
  panel: A2HarnessPanel | null;
  inputs: PanelInputs;
  output: HelperOutput | null;
  notice: string | null;
}

const session: SessionState = {
  panel: null,
  inputs: emptyInputs(),
  output: null,
  notice: null,
};

function model(): RenderModel {
  return { inputs: session.inputs, output: session.output, notice: session.notice };
}

function rerender(): void {
  if (session.panel) {
    session.panel.show(model());
  }
}

function defaultWorkspace(): string | null {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return null;
  }
  return folders[0].uri.fsPath;
}

// Resolve the helper path from configuration. Relative paths resolve against
// the workspace folder. Returns null if it cannot be resolved.
function resolveHelperPath(): string | null {
  const config = vscode.workspace.getConfiguration("a2HarnessPanel");
  const raw = config.get<string>("helperPath", "scripts/a2-ide-harness.sh");
  const helper = raw && raw.trim().length > 0 ? raw : "scripts/a2-ide-harness.sh";
  if (path.isAbsolute(helper)) {
    return helper;
  }
  const ws = session.inputs.workspace ?? defaultWorkspace();
  if (!ws) {
    return null;
  }
  return path.join(ws, helper);
}

function isHelperSubcommand(s: string): s is HelperSubcommand {
  return (ALLOWED_SUBCOMMANDS as readonly string[]).includes(s);
}

// Build the helper options object for a subcommand from the currently-set
// inputs, restricted to that subcommand's allowed flags.
function optionsFor(sub: HelperSubcommand): { options: Record<string, string>; missing: string[] } {
  const options: Record<string, string> = {};
  const missing: string[] = [];
  const button = PANEL_BUTTONS.find(
    (b): b is HelperButton => b.kind === "helper" && b.subcommand === sub,
  );
  const needs = button ? button.needs : ALLOWED_FLAGS[sub];
  for (const flag of needs) {
    const key = FLAG_TO_INPUT[flag];
    const value = key ? session.inputs[key] : null;
    if (value && value.length > 0) {
      options[flag] = value;
    } else {
      missing.push(flag);
    }
  }
  return { options, missing };
}

async function runSubcommand(sub: string): Promise<void> {
  if (!isHelperSubcommand(sub)) {
    session.notice = `refused: ${sub} is not a read-only/print helper subcommand`;
    rerender();
    return;
  }
  const helperPath = resolveHelperPath();
  if (!helperPath) {
    session.notice = "Set a workspace first (or configure an absolute a2HarnessPanel.helperPath).";
    rerender();
    return;
  }
  const { options, missing } = optionsFor(sub);
  if (missing.length > 0) {
    session.notice = `Set these first for ${sub}: ${missing.join(", ")}`;
    rerender();
    return;
  }

  const inv: HelperInvocation = { helperPath, subcommand: sub, options };
  try {
    const result = await runHelper(inv, defaultSpawnImpl());
    session.output = {
      subcommand: sub,
      exitCode: result.exitCode,
      stdout: result.stdout,
      stderr: result.stderr,
    };
    session.notice = null;
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    session.notice = `helper invocation refused/failed: ${msg}`;
  }
  rerender();
}

async function pickPath(prompt: string, key: keyof PanelInputs): Promise<void> {
  const value = await vscode.window.showInputBox({ prompt, ignoreFocusOut: true });
  if (value !== undefined) {
    session.inputs[key] = value.trim().length > 0 ? value.trim() : null;
    session.notice = null;
    rerender();
  }
}

async function handleUiAction(action: string): Promise<void> {
  switch (action) {
    case "selectWorkspace": {
      const ws = defaultWorkspace();
      if (ws && !session.inputs.workspace) {
        session.inputs.workspace = ws;
        rerender();
        return;
      }
      await pickPath("A2 workspace root (contains .claw and the target)", "workspace");
      return;
    }
    case "selectPlan":
      await pickPath("Path to plan.yaml (after_file must be relative)", "plan");
      return;
    case "selectPreviewBundle":
      await pickPath("Path to preview-bundle.json", "previewBundle");
      return;
    case "selectGeneratorResult":
      await pickPath("Path to preview-generator-result.json", "generatorResult");
      return;
    case "selectApprovalResult":
      await pickPath("Path to the persisted approval-result.json", "approvalResult");
      return;
    case "selectApprovalOutput":
      await pickPath("Path to write the new approval-result.json (must not exist)", "approvalOutput");
      return;
    case "selectApplyBundle":
      await pickPath("Path to apply-bundle.json", "applyBundle");
      return;
    case "selectTarget":
      await pickPath("Path to the target file written by plan apply", "target");
      return;
    case "setAfterSha":
      await pickPath("Expected after_sha256 of the target", "afterSha");
      return;
    case "openRunbook":
      await openRunbook();
      return;
    case "exportEvidence":
      await exportEvidence();
      return;
    default:
      session.notice = `unknown action: ${action}`;
      rerender();
      return;
  }
}

async function openRunbook(): Promise<void> {
  const ws = session.inputs.workspace ?? defaultWorkspace();
  const candidate = ws
    ? path.join(ws, "docs", "runbooks", "a2-ide-harness-workflow.md")
    : null;
  if (!candidate) {
    session.notice = "Set a workspace to locate the runbook.";
    rerender();
    return;
  }
  try {
    const uri = vscode.Uri.file(candidate);
    await vscode.window.showTextDocument(uri, { preview: true });
  } catch {
    session.notice = `Could not open runbook at ${candidate}`;
    rerender();
  }
}

// Export a read-only evidence summary into an UNSAVED untitled document. The
// panel writes no file to disk; the operator saves it themselves if they wish.
async function exportEvidence(): Promise<void> {
  const i = session.inputs;
  const o = session.output;
  const lines = [
    "# A2 Harness Panel — Evidence Summary",
    "",
    "## Inputs",
    `- workspace: ${i.workspace ?? "(not set)"}`,
    `- plan: ${i.plan ?? "(not set)"}`,
    `- preview-bundle: ${i.previewBundle ?? "(not set)"}`,
    `- preview-generator-result: ${i.generatorResult ?? "(not set)"}`,
    `- approval-result: ${i.approvalResult ?? "(not set)"}`,
    `- approval-output: ${i.approvalOutput ?? "(not set)"}`,
    `- apply-bundle: ${i.applyBundle ?? "(not set)"}`,
    `- target: ${i.target ?? "(not set)"}`,
    `- after-sha: ${i.afterSha ?? "(not set)"}`,
    "",
    "## Last helper output",
    o
      ? `- subcommand: ${o.subcommand} (exit ${o.exitCode})`
      : "- (no helper subcommand has been run yet)",
    "",
    "```text",
    o ? o.stdout : "",
    "```",
    "",
    "_This summary is read-only. The panel ran no A2 chain command, made no model/broker call, and wrote no target._",
  ];
  const doc = await vscode.workspace.openTextDocument({
    content: lines.join("\n"),
    language: "markdown",
  });
  await vscode.window.showTextDocument(doc, { preview: false });
}

async function handleMessage(msg: PanelMessage): Promise<void> {
  if (msg.type === "runSubcommand") {
    await runSubcommand(msg.subcommand);
    return;
  }
  if (msg.type === "uiAction") {
    await handleUiAction(msg.action);
    return;
  }
  if (msg.type === "copyOutput") {
    const payload = session.output ? session.output.stdout : "";
    await vscode.env.clipboard.writeText(payload);
    vscode.window.setStatusBarMessage("A2 Harness: copied helper output to clipboard", 2000);
    return;
  }
}

function openPanel(): void {
  if (!session.panel) {
    session.panel = new A2HarnessPanel({ onMessage: handleMessage });
  }
  session.panel.show(model());
}

export function activate(context: vscode.ExtensionContext): void {
  const disposable = vscode.commands.registerCommand("a2HarnessPanel.open", openPanel);
  context.subscriptions.push(disposable);
}

export function deactivate(): void {
  if (session.panel) {
    session.panel.dispose();
    session.panel = null;
  }
}

// Exported for type-checking parity; renderHtml is the single render entry.
export { renderHtml };
