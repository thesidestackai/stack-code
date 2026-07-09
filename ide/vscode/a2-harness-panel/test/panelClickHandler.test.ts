import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { renderHtml, emptyInputs } from "../src/render";
import { emptyN6SessionState, buildN6View } from "../src/n6View";
import { N6SessionState } from "../src/n6View";

// tsconfig.test.json rootDir="."; compiled to out-test/test/ → walk up two dirs to reach src/.
const PANEL_SRC = fs.readFileSync(
  path.join(__dirname, "../../src/panel.ts"),
  "utf8",
);

// Minimal N5 ladder with package-plan READY (matches n6Render.test.ts helper).
function readyN5() {
  return {
    state: "N5_READY_FOR_PACKAGE_PLAN" as const,
    stepLabel: "Ready for package-plan",
    isBlocked: false,
    n4State: "N4_DRAFT_REVIEWED" as const,
    n4StepLabel: "Draft reviewed",
    taskSummary: "test task",
    riskLevel: "low",
    ladder: [
      { rung: "package-plan",   purpose: "p", readiness: "READY",     preconditionLines: [], evidencePresent: true,  operatorConfirmationRequired: false, note: "" },
      { rung: "package-commit", purpose: "p", readiness: "NOT_READY", preconditionLines: [], evidencePresent: false, operatorConfirmationRequired: false, note: "" },
      { rung: "package-push",   purpose: "p", readiness: "NOT_READY", preconditionLines: [], evidencePresent: false, operatorConfirmationRequired: false, note: "" },
      { rung: "package-pr",     purpose: "p", readiness: "NOT_READY", preconditionLines: [], evidencePresent: false, operatorConfirmationRequired: false, note: "" },
    ],
  };
}

function htmlWithN6(session: N6SessionState, n5Ready = false) {
  const n5 = n5Ready ? readyN5() : null;
  return renderHtml({
    inputs: emptyInputs(),
    output: null,
    notice: null,
    n5,
    n6: buildN6View(n5, session),
  });
}

describe("Panel click handler — N6 button coverage (regression guard)", () => {
  // --- Source-level checks (static) ---

  it("panel.ts webview click handler uses button[data-ui-action] selector", () => {
    assert.ok(
      PANEL_SRC.includes("button[data-ui-action]"),
      "panel.ts must use 'button[data-ui-action]' so n6-token-entry and n6-run-btn clicks are routed to the extension",
    );
  });

  it("panel.ts does not use .btn.ui[data-ui-action] as the uiAction handler", () => {
    // The old selector silently dropped N6 button clicks.
    assert.ok(
      !PANEL_SRC.includes("'.btn.ui[data-ui-action]'") &&
        !PANEL_SRC.includes('".btn.ui[data-ui-action]"'),
      ".btn.ui[data-ui-action] must not be the uiAction handler; use button[data-ui-action]",
    );
  });

  // --- Rendered HTML checks: N6 token-entry buttons are <button> elements ---

  it("plan token-entry renders as <button> with data-ui-action (AWAITING_TOKEN)", () => {
    const html = htmlWithN6(emptyN6SessionState(), true);
    const match = html.match(/<button[^>]*data-ui-action="n6ActivatePlanToken"[^>]*>/);
    assert.ok(match, "plan token-entry must be present in HTML");
    assert.ok(
      match![0].startsWith("<button"),
      "plan token-entry must be a <button> element (not div/span), so button[data-ui-action] selector hits it",
    );
  });

  it("commit token-entry renders as <button> with data-ui-action", () => {
    const html = htmlWithN6(emptyN6SessionState(), true);
    const match = html.match(/<button[^>]*data-ui-action="n6ActivateCommitToken"[^>]*>/);
    assert.ok(match, "commit token-entry must be present in HTML");
    assert.ok(match![0].startsWith("<button"), "commit token-entry must be a <button> element");
  });

  // --- Rendered HTML checks: N6 run buttons are <button> elements ---

  it("plan run button renders as <button> with data-ui-action (TOKEN_ACTIVE)", () => {
    const session: N6SessionState = {
      ...emptyN6SessionState(),
      planTokenActive: true,
      planExec: "TOKEN_ACTIVE",
    };
    const html = htmlWithN6(session, true);
    const match = html.match(/<button[^>]*data-ui-action="n6RunPlan"[^>]*>/);
    assert.ok(match, "plan run button must be present when TOKEN_ACTIVE + N5 ready");
    assert.ok(
      match![0].startsWith("<button"),
      "n6RunPlan must be a <button> element so button[data-ui-action] selector hits it",
    );
  });
});
