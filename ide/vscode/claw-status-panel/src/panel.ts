import * as vscode from "vscode";
import { RenderModel, renderHtml } from "./render";

export type PanelMessage =
  | { type: "refresh" }
  | { type: "copy"; kind: "next_operator_command" | "evidence_path" | "raw_envelope"; payload: string }
  | { type: "openEvidence"; path: string };

export interface PanelOptions {
  onMessage: (msg: PanelMessage) => Promise<void>;
}

export class ClawStatusPanel {
  private readonly panel: vscode.WebviewPanel;

  constructor(opts: PanelOptions) {
    this.panel = vscode.window.createWebviewPanel(
      "clawStatus",
      "Claw Status",
      vscode.ViewColumn.Beside,
      {
        enableScripts: true,
        // No local resource roots: the webview renders only the
        // RenderModel-derived HTML we hand it. No file content is loaded
        // by the webview from any workspace path.
        localResourceRoots: [],
        retainContextWhenHidden: true,
      },
    );
    this.panel.webview.onDidReceiveMessage(async (msg: PanelMessage) => {
      try {
        await opts.onMessage(msg);
      } catch {
        // Errors are surfaced on the next refresh; no telemetry, no
        // network egress, no STOP downgrade.
      }
    });
  }

  show(model: RenderModel): void {
    const baseHtml = renderHtml(model);
    const wiringScript = `
<script>
(function () {
  const vscode = acquireVsCodeApi();
  function bindAll() {
    document.querySelectorAll('[data-action="refresh"]').forEach(function (el) {
      el.addEventListener('click', function () { vscode.postMessage({ type: 'refresh' }); });
    });
    document.querySelectorAll('[data-copy-target="next_operator_command"]').forEach(function (el) {
      el.addEventListener('click', function () {
        vscode.postMessage({ type: 'copy', kind: 'next_operator_command', payload: el.textContent || '' });
      });
    });
    document.querySelectorAll('[data-copy-evidence]').forEach(function (el) {
      el.addEventListener('click', function () {
        vscode.postMessage({ type: 'copy', kind: 'evidence_path', payload: el.getAttribute('data-copy-evidence') || '' });
      });
    });
    document.querySelectorAll('[data-copy-raw="true"]').forEach(function (el) {
      el.addEventListener('click', function () {
        var pre = document.querySelector('[data-testid="raw-stdout"]');
        vscode.postMessage({ type: 'copy', kind: 'raw_envelope', payload: pre ? pre.textContent || '' : '' });
      });
    });
    document.querySelectorAll('a.evidence-link').forEach(function (el) {
      el.addEventListener('click', function (ev) {
        ev.preventDefault();
        vscode.postMessage({ type: 'openEvidence', path: el.getAttribute('data-evidence-path') || '' });
      });
    });
  }
  bindAll();
}());
</script>
`;
    // Inject the wiring script before </body> without otherwise mutating
    // the renderHtml output (so render.test.ts can assert on the canonical
    // markup unchanged).
    const final = baseHtml.replace("</body>", `${wiringScript}</body>`);
    this.panel.webview.html = final;
    this.panel.reveal(undefined, true);
  }

  dispose(): void {
    this.panel.dispose();
  }
}
