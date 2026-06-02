import * as vscode from "vscode";
import { ClawStatusPanel, PanelMessage } from "./panel";
import { parseStatus } from "./parser";
import { buildRenderModel } from "./render";
import { classifyAll } from "./evidence_path";
import {
  ClawStatusInvocation,
  defaultSpawnImpl,
  runClawStatus,
} from "./subprocess";

interface SessionState {
  panel: ClawStatusPanel | null;
  workspace: string | null;
  approvalResultPath: string | undefined;
}

const session: SessionState = {
  panel: null,
  workspace: null,
  approvalResultPath: undefined,
};

async function resolveWorkspace(): Promise<string | null> {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return null;
  }
  if (folders.length === 1) {
    return folders[0].uri.fsPath;
  }
  const items = folders.map((f) => f.uri.fsPath);
  const picked = await vscode.window.showQuickPick(items, {
    placeHolder: "Select workspace folder for Claw Status",
  });
  return picked ?? null;
}

function getBinaryPath(): string {
  const config = vscode.workspace.getConfiguration("clawStatus");
  const raw = config.get<string>("binaryPath", "claw");
  return raw && raw.trim().length > 0 ? raw : "claw";
}

async function runOnce(): Promise<void> {
  const workspace = await resolveWorkspace();
  if (!workspace) {
    vscode.window.showWarningMessage(
      "Claw Status: open a workspace folder before refreshing.",
    );
    return;
  }
  session.workspace = workspace;

  const inv: ClawStatusInvocation = {
    binary: getBinaryPath(),
    workspace,
    approvalResultPath: session.approvalResultPath,
  };

  let result;
  try {
    result = await runClawStatus(inv, defaultSpawnImpl());
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    vscode.window.showErrorMessage(`Claw Status: subprocess error: ${msg}`);
    return;
  }

  const parsed = parseStatus(result.stdout, result.exitCode);
  const evidencePathStrings = parsed.envelope?.evidence_paths ?? [];
  const evidence = classifyAll(evidencePathStrings, workspace);
  const model = buildRenderModel(parsed, evidence);

  if (!session.panel) {
    session.panel = new ClawStatusPanel({
      onMessage: handleMessage,
    });
  }
  session.panel.show(model);
}

async function handleMessage(msg: PanelMessage): Promise<void> {
  if (msg.type === "refresh") {
    await runOnce();
    return;
  }
  if (msg.type === "copy") {
    await vscode.env.clipboard.writeText(msg.payload);
    vscode.window.setStatusBarMessage(
      `Claw Status: copied ${msg.kind} to clipboard`,
      2000,
    );
    return;
  }
  if (msg.type === "openEvidence") {
    try {
      const uri = vscode.Uri.file(msg.path);
      await vscode.window.showTextDocument(uri, { preview: true });
    } catch {
      vscode.window.showWarningMessage(
        `Claw Status: could not open evidence path ${msg.path}`,
      );
    }
    return;
  }
}

export function activate(context: vscode.ExtensionContext): void {
  const disposable = vscode.commands.registerCommand(
    "clawStatus.refresh",
    runOnce,
  );
  context.subscriptions.push(disposable);
}

export function deactivate(): void {
  if (session.panel) {
    session.panel.dispose();
    session.panel = null;
  }
}
