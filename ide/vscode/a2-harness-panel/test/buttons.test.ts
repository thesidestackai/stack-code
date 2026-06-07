import * as assert from "assert";
import {
  PANEL_BUTTONS,
  FORBIDDEN_BUTTON_LABELS,
  helperButtons,
  allHelperButtonsAreAllowlisted,
} from "../src/buttons";
import { ALLOWED_SUBCOMMANDS, ALLOWED_FLAGS } from "../src/helperRunner";

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
