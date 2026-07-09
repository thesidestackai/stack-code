import * as vscode from "vscode";
import { RenderModel, renderHtml } from "./render";

export type PanelMessage =
  | { type: "runSubcommand"; subcommand: string }
  | { type: "uiAction"; action: string }
  | { type: "copyOutput" };

export interface PanelOptions {
  onMessage: (msg: PanelMessage) => Promise<void>;
}

export class A2HarnessPanel {
  private readonly panel: vscode.WebviewPanel;

  constructor(opts: PanelOptions) {
    this.panel = vscode.window.createWebviewPanel(
      "a2HarnessPanel",
      "A2 Harness Panel",
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        // No local resource roots: the webview renders only the
        // RenderModel-derived HTML we hand it. It loads no file content from
        // any workspace path and reaches no network.
        localResourceRoots: [],
        retainContextWhenHidden: true,
      },
    );
    this.panel.webview.onDidReceiveMessage(async (msg: PanelMessage) => {
      try {
        await opts.onMessage(msg);
      } catch {
        // Surfaced on the next render; no telemetry, no network egress.
      }
    });
  }

  show(model: RenderModel): void {
    const baseHtml = renderHtml(model);
    const wiringScript = `
<script>
(function () {
  const vscode = acquireVsCodeApi();
  document.querySelectorAll('.btn.helper[data-subcommand]').forEach(function (el) {
    el.addEventListener('click', function () {
      vscode.postMessage({ type: 'runSubcommand', subcommand: el.getAttribute('data-subcommand') || '' });
    });
  });
  document.querySelectorAll('button[data-ui-action]').forEach(function (el) {
    el.addEventListener('click', function () {
      vscode.postMessage({ type: 'uiAction', action: el.getAttribute('data-ui-action') || '' });
    });
  });
  document.querySelectorAll('[data-copy-output="true"]').forEach(function (el) {
    el.addEventListener('click', function () {
      vscode.postMessage({ type: 'copyOutput' });
    });
  });
}());
</script>
`;
    this.panel.webview.html = baseHtml.replace("</body>", `${wiringScript}</body>`);
    this.panel.reveal(undefined, true);
  }

  dispose(): void {
    this.panel.dispose();
  }
}
