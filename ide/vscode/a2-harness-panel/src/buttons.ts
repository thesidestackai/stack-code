// The panel's button catalog. Each button is either a "helper" button (maps to
// exactly one read-only/print helper subcommand) or a "ui" button (a host-side
// action: pick a path, open the runbook, export evidence, copy text). There are
// NO execution buttons for any chain-write step: Run Preview / Run Approval /
// Run Apply-Bundle / Run Apply are intentionally absent (scope §9).
//
// Tests assert that every helper button's subcommand is in the read-only
// allowlist, that no button references a chain-write command, and that the
// dangerous Run-* buttons are not present.

import { ALLOWED_SUBCOMMANDS, HelperSubcommand } from "./helperRunner";

export type ButtonKind = "helper" | "ui";

export interface HelperButton {
  id: string;
  label: string;
  kind: "helper";
  subcommand: HelperSubcommand;
  // Flags this button needs the operator to have supplied (subset of the
  // subcommand's ALLOWED_FLAGS). Empty for argument-less subcommands.
  needs: readonly string[];
}

export interface UiButton {
  id: string;
  label: string;
  kind: "ui";
  action:
    | "selectWorkspace"
    | "selectPlan"
    | "selectPreviewBundle"
    | "selectGeneratorResult"
    | "selectApprovalResult"
    | "selectApprovalOutput"
    | "selectApplyBundle"
    | "selectTarget"
    | "setAfterSha"
    | "refreshStatus"
    | "openRunbook"
    | "exportEvidence";
}

export type PanelButton = HelperButton | UiButton;

// Labels intentionally say "Generate"/"Print"/"Show/Copy" — never "Run" — for
// the chain-write stages. These produce or copy command TEXT; they never
// execute the command.
export const PANEL_BUTTONS: readonly PanelButton[] = [
  // Field-setter controls. Each sets one session input field via its existing
  // handleUiAction handler in extension.ts (no new handler logic). These are
  // the controls the artifact-field UX polish makes discoverable so later-stage
  // buttons (Verify Final, Show/Copy Approval, Show/Copy Apply) can be satisfied
  // from the GUI. They SET fields only; they never run a chain command.
  { id: "select-workspace", label: "Select Workspace", kind: "ui", action: "selectWorkspace" },
  { id: "select-plan", label: "Select Plan", kind: "ui", action: "selectPlan" },
  { id: "select-target", label: "Select Target", kind: "ui", action: "selectTarget" },
  { id: "set-after-sha", label: "Set After SHA", kind: "ui", action: "setAfterSha" },
  { id: "select-preview-bundle", label: "Select Preview Bundle", kind: "ui", action: "selectPreviewBundle" },
  { id: "select-generator-result", label: "Select Generator Result", kind: "ui", action: "selectGeneratorResult" },
  { id: "select-approval-result", label: "Select Approval Result", kind: "ui", action: "selectApprovalResult" },
  { id: "set-approval-output", label: "Set Approval Output", kind: "ui", action: "selectApprovalOutput" },
  { id: "select-apply-bundle", label: "Select Apply Bundle", kind: "ui", action: "selectApplyBundle" },

  { id: "validate-input", label: "Validate Input", kind: "helper", subcommand: "validate-input", needs: ["workspace", "plan"] },
  { id: "audit-workspace", label: "Audit Workspace", kind: "helper", subcommand: "audit-workspace", needs: ["workspace"] },
  { id: "find-artifacts", label: "Find Artifacts", kind: "helper", subcommand: "find-artifacts", needs: ["workspace"] },

  { id: "show-preview-command", label: "Show/Copy Preview Command", kind: "helper", subcommand: "print-preview", needs: ["workspace", "plan"] },
  { id: "show-approval-command", label: "Show/Copy Approval Command", kind: "helper", subcommand: "print-approval", needs: ["workspace", "preview-bundle", "approval-output"] },
  { id: "show-apply-bundle-command", label: "Show/Copy Apply-Bundle Command", kind: "helper", subcommand: "print-apply-bundle", needs: ["preview-generator-result", "approval-result"] },
  { id: "show-apply-command", label: "Show/Copy Apply Command", kind: "helper", subcommand: "print-apply", needs: ["apply-bundle"] },

  { id: "verify-final", label: "Verify Final Target", kind: "helper", subcommand: "verify-final", needs: ["workspace", "target", "after-sha"] },

  // Workspace-first: one-shot, read-only re-detection of setup status,
  // discovery, and the next safe step. It runs only read-only helper
  // subcommands (audit-workspace / help) + a vscode file search; it sets no
  // input field destructively and runs no chain command.
  { id: "refresh-status", label: "Refresh Workspace Status", kind: "ui", action: "refreshStatus" },

  { id: "open-runbook", label: "Open Runbook", kind: "ui", action: "openRunbook" },
  { id: "export-evidence", label: "Export Evidence Summary", kind: "ui", action: "exportEvidence" },
];

// The dangerous execution buttons that MUST NOT exist on this surface. Tests
// assert no PANEL_BUTTONS entry carries one of these ids or labels.
export const FORBIDDEN_BUTTON_LABELS = [
  "Run Preview",
  "Run Approval",
  "Run Apply-Bundle",
  "Run Apply",
] as const;

// UI actions that SET an input field (vs. workflow UI actions like opening the
// runbook or exporting evidence). Used by render.ts to group the field-setter
// controls next to the field table they populate, improving discoverability.
export const FIELD_SETTER_ACTIONS: ReadonlySet<string> = new Set([
  "selectWorkspace",
  "selectPlan",
  "selectTarget",
  "setAfterSha",
  "selectPreviewBundle",
  "selectGeneratorResult",
  "selectApprovalResult",
  "selectApprovalOutput",
  "selectApplyBundle",
]);

export function isFieldSetterAction(action: string): boolean {
  return FIELD_SETTER_ACTIONS.has(action);
}

export function helperButtons(): HelperButton[] {
  return PANEL_BUTTONS.filter((b): b is HelperButton => b.kind === "helper");
}

// UI buttons that set an input field (Select Workspace/Plan/Target/…, Set After
// SHA, Set Approval Output). Rendered in the inputs section next to the fields.
export function fieldSetterButtons(): UiButton[] {
  return PANEL_BUTTONS.filter(
    (b): b is UiButton => b.kind === "ui" && isFieldSetterAction(b.action),
  );
}

// UI buttons that are workflow actions, not field setters (Open Runbook, Export
// Evidence Summary). Rendered in the actions section.
export function workflowUiButtons(): UiButton[] {
  return PANEL_BUTTONS.filter(
    (b): b is UiButton => b.kind === "ui" && !isFieldSetterAction(b.action),
  );
}

// True only if every helper button maps to a subcommand in the read-only
// allowlist. Used by tests as a structural guard.
export function allHelperButtonsAreAllowlisted(): boolean {
  const allow = new Set<string>(ALLOWED_SUBCOMMANDS);
  return helperButtons().every((b) => allow.has(b.subcommand));
}
