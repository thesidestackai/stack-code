import * as assert from "assert";
import {
  PANEL_BUTTONS,
  FORBIDDEN_BUTTON_LABELS,
  helperButtons,
  allHelperButtonsAreAllowlisted,
  fieldSetterButtons,
  workflowUiButtons,
  isFieldSetterAction,
  FIELD_SETTER_ACTIONS,
} from "../src/buttons";
import { ALLOWED_SUBCOMMANDS, ALLOWED_FLAGS } from "../src/helperRunner";

// The exact UI actions extension.ts handleUiAction implements. A field-setter
// button must map to one of these, so a new button can never reach a missing
// handler. (Kept in sync with extension.ts by review; asserted structurally.)
const HANDLED_UI_ACTIONS = new Set([
  "selectWorkspace",
  "selectPlan",
  "selectPreviewBundle",
  "selectGeneratorResult",
  "selectApprovalResult",
  "selectApprovalOutput",
  "selectApplyBundle",
  "selectTarget",
  "setAfterSha",
  "refreshStatus",
  "openRunbook",
  "exportEvidence",
]);

describe("buttons — only read-only/print helper subcommands", () => {
  it("every helper button maps to an allowlisted subcommand", () => {
    assert.ok(allHelperButtonsAreAllowlisted());
    const allow = new Set<string>(ALLOWED_SUBCOMMANDS);
    for (const b of helperButtons()) {
      assert.ok(allow.has(b.subcommand), `button ${b.id} -> non-allowlisted ${b.subcommand}`);
    }
  });

  it("every helper button's needs are a subset of its subcommand's allowed flags", () => {
    for (const b of helperButtons()) {
      const allowed = new Set(ALLOWED_FLAGS[b.subcommand]);
      for (const n of b.needs) {
        assert.ok(allowed.has(n), `button ${b.id} needs --${n} not allowed for ${b.subcommand}`);
      }
    }
  });
});

describe("buttons — dangerous execution buttons are absent", () => {
  it("no button label is a Run-* execution control", () => {
    const labels = PANEL_BUTTONS.map((b) => b.label);
    for (const forbidden of FORBIDDEN_BUTTON_LABELS) {
      assert.ok(!labels.includes(forbidden), `forbidden button present: ${forbidden}`);
    }
  });

  it("no button id or label starts with 'Run ' / 'run-'", () => {
    for (const b of PANEL_BUTTONS) {
      assert.ok(!/^run-/i.test(b.id), `run-* button id: ${b.id}`);
      assert.ok(!/^run\s/i.test(b.label), `Run button label: ${b.label}`);
    }
  });

  it("no helper button maps to a chain-write executor subcommand", () => {
    for (const b of helperButtons()) {
      assert.ok(
        !/^(run|approve|apply|apply-bundle)$/.test(b.subcommand),
        `executor subcommand on a button: ${b.subcommand}`,
      );
    }
  });
});

describe("buttons — required safe buttons are present", () => {
  const requiredLabels = [
    "Validate Input",
    "Audit Workspace",
    "Find Artifacts",
    "Show/Copy Preview Command",
    "Show/Copy Approval Command",
    "Show/Copy Apply-Bundle Command",
    "Show/Copy Apply Command",
    "Verify Final Target",
    "Open Runbook",
    "Export Evidence Summary",
  ];
  it("includes every required safe button", () => {
    const labels = new Set(PANEL_BUTTONS.map((b) => b.label));
    for (const l of requiredLabels) {
      assert.ok(labels.has(l), `missing required safe button: ${l}`);
    }
  });

  it("approval-related button is print-only (no approval-line composition in catalog)", () => {
    const approval = PANEL_BUTTONS.find((b) => b.id === "show-approval-command");
    assert.ok(approval && approval.kind === "helper");
    if (approval.kind === "helper") {
      assert.strictEqual(approval.subcommand, "print-approval");
    }
    // The catalog carries no literal approval line.
    const blob = JSON.stringify(PANEL_BUTTONS);
    assert.ok(!/apply\s+\$\{.*step.*\}\s+\$\{.*preview.*\}/.test(blob));
  });
});

describe("buttons — artifact field-setter controls (UX polish)", () => {
  const requiredFieldSetters = [
    { label: "Select Workspace", action: "selectWorkspace" },
    { label: "Select Plan", action: "selectPlan" },
    { label: "Select Target", action: "selectTarget" },
    { label: "Set After SHA", action: "setAfterSha" },
    { label: "Select Preview Bundle", action: "selectPreviewBundle" },
    { label: "Select Generator Result", action: "selectGeneratorResult" },
    { label: "Select Approval Result", action: "selectApprovalResult" },
    { label: "Set Approval Output", action: "selectApprovalOutput" },
    { label: "Select Apply Bundle", action: "selectApplyBundle" },
  ];

  it("exposes a visible control for every artifact/hash field", () => {
    const setters = fieldSetterButtons();
    for (const want of requiredFieldSetters) {
      const found = setters.find((b) => b.label === want.label);
      assert.ok(found, `missing field-setter control: ${want.label}`);
      if (found) {
        assert.strictEqual(found.action, want.action);
      }
    }
  });

  it("covers exactly the FIELD_SETTER_ACTIONS set (9 setters)", () => {
    const actions = new Set<string>(fieldSetterButtons().map((b) => b.action));
    assert.strictEqual(actions.size, 9);
    for (const a of FIELD_SETTER_ACTIONS) {
      assert.ok(actions.has(a), `no button for field-setter action ${a}`);
    }
  });

  it("every field-setter button maps to an action extension.ts handles", () => {
    for (const b of fieldSetterButtons()) {
      assert.ok(HANDLED_UI_ACTIONS.has(b.action), `unhandled action: ${b.action}`);
      assert.ok(isFieldSetterAction(b.action));
    }
  });

  it("field setters cover the fields the later-stage buttons need", () => {
    // The flags that previously had no visible control.
    const formerlyUnreachable = [
      "target",
      "after-sha",
      "preview-bundle",
      "preview-generator-result",
      "approval-result",
      "approval-output",
      "apply-bundle",
    ];
    const flagToAction: Record<string, string> = {
      "target": "selectTarget",
      "after-sha": "setAfterSha",
      "preview-bundle": "selectPreviewBundle",
      "preview-generator-result": "selectGeneratorResult",
      "approval-result": "selectApprovalResult",
      "approval-output": "selectApprovalOutput",
      "apply-bundle": "selectApplyBundle",
    };
    const setterActions = new Set<string>(fieldSetterButtons().map((b) => b.action));
    for (const flag of formerlyUnreachable) {
      assert.ok(setterActions.has(flagToAction[flag]), `no control for field ${flag}`);
    }
  });

  it("no field-setter or workflow UI button runs a chain command (still no Run-*)", () => {
    const uiLabels = [...fieldSetterButtons(), ...workflowUiButtons()].map((b) => b.label);
    for (const forbidden of FORBIDDEN_BUTTON_LABELS) {
      assert.ok(!uiLabels.includes(forbidden), `forbidden UI button: ${forbidden}`);
    }
    for (const b of [...fieldSetterButtons(), ...workflowUiButtons()]) {
      assert.ok(!/^run\s/i.test(b.label), `Run button label: ${b.label}`);
    }
  });

  it("workflow UI buttons are Open Runbook + Export Evidence + Refresh Status", () => {
    const labels = workflowUiButtons().map((b) => b.label).sort();
    assert.deepStrictEqual(labels, [
      "Export Evidence Summary",
      "Open Runbook",
      "Refresh Workspace Status",
    ]);
  });

  it("the Refresh Workspace Status button runs no chain command and is not a Run-*", () => {
    const refresh = PANEL_BUTTONS.find((b) => b.id === "refresh-status");
    assert.ok(refresh && refresh.kind === "ui");
    if (refresh && refresh.kind === "ui") {
      assert.strictEqual(refresh.action, "refreshStatus");
    }
    assert.ok(!/^run\s/i.test("Refresh Workspace Status"));
  });
});
