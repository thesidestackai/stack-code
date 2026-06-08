// Workspace-first SETUP STATUS model (pure).
//
// Given a one-shot, read-only probe of the workspace (all gathered by the
// extension through the helper's read-only subcommands + vscode.workspace
// .findFiles — never node `fs`, never a watcher, never a claw spawn), this
// module computes an honest status for each setup dimension. Every dimension is
// a positive, a negative, or an explicit "not-checked" — status is never
// green-by-default.
//
// Honesty note on claw: the panel cannot verify the claw BINARY exists without
// fs/spawn (both forbidden by the package guards). So clawBinary is reported as
// "configured" (a path was parsed from the helper's usage output) or "unknown"
// — never "found", because existence is not something this safe layer proves.
// The operator runs claw themselves at a real terminal; the panel never needs it.

import { AuditParse, ChainState, auditPathFor, selectCandidate } from "./discovery";

// Whether a single read-only helper probe was attempted and what happened.
export type HelperProbe = "ran" | "spawn-error" | "not-run";

export type Tri = "found" | "missing" | "not-checked";
export type Presence = "found" | "not-found" | "not-checked";

export interface SetupInputs {
  workspace: string | null;
  plan: string | null;
  target: string | null;
  afterSha: string | null;
  previewBundle: string | null;
  generatorResult: string | null;
  approvalResult: string | null;
  applyBundle: string | null;
}

export interface SetupProbe {
  helperProbe: HelperProbe;
  // Configured claw path parsed from the helper's usage output (if any).
  clawPath: string | null;
  // Workspace root the extension detected (vscode folder or operator-set).
  workspaceRoot: string | null;
  inputs: SetupInputs;
  // Parsed audit-workspace result, or null if it was not run (e.g. no workspace).
  audit: AuditParse | null;
  // plan.yaml candidates discovered via vscode.workspace.findFiles (read-only).
  planCandidates: string[];
}

export interface SetupStatus {
  helperPath: Tri;
  clawBinary: "configured" | "unknown";
  workspaceRoot: "detected" | "not-detected";
  plan: "found" | "select-needed" | "unknown";
  target: "known" | "unknown";
  afterSha: "known" | "unknown";
  previewBundle: Presence;
  approvalResult: Presence;
  applyBundle: Presence;
  finalVerification: "match" | "mismatch" | "not-checked";
}

function isSet(v: string | null | undefined): boolean {
  return typeof v === "string" && v.trim().length > 0;
}

function helperPathStatus(probe: HelperProbe): Tri {
  switch (probe) {
    case "ran":
      return "found";
    case "spawn-error":
      return "missing";
    case "not-run":
      return "not-checked";
    default:
      return "not-checked";
  }
}

function presence(audit: AuditParse | null, name: Parameters<typeof auditPathFor>[1], inputSet: boolean): Presence {
  if (inputSet) {
    return "found";
  }
  if (!audit) {
    return "not-checked";
  }
  return auditPathFor(audit, name) ? "found" : "not-found";
}

function planStatus(inputs: SetupInputs, planCandidates: string[]): "found" | "select-needed" | "unknown" {
  if (isSet(inputs.plan)) {
    return "found";
  }
  const sel = selectCandidate(planCandidates);
  if (sel.mode === "auto") {
    return "found";
  }
  if (sel.mode === "select-needed") {
    return "select-needed";
  }
  return "unknown";
}

export function computeSetupStatus(probe: SetupProbe): SetupStatus {
  const audit = probe.audit;
  return {
    helperPath: helperPathStatus(probe.helperProbe),
    clawBinary: isSet(probe.clawPath) ? "configured" : "unknown",
    workspaceRoot: isSet(probe.workspaceRoot) ? "detected" : "not-detected",
    plan: planStatus(probe.inputs, probe.planCandidates),
    target: isSet(probe.inputs.target) ? "known" : "unknown",
    afterSha: isSet(probe.inputs.afterSha) ? "known" : "unknown",
    previewBundle: presence(audit, "preview-bundle.json", isSet(probe.inputs.previewBundle)),
    approvalResult: presence(audit, "approval-result.json", isSet(probe.inputs.approvalResult)),
    applyBundle: presence(audit, "apply-bundle.json", isSet(probe.inputs.applyBundle)),
    finalVerification:
      audit && audit.targetHash.checked
        ? audit.targetHash.match
          ? "match"
          : "mismatch"
        : "not-checked",
  };
}

// Convenience for the state machine: the chain state the audit reported, or
// null when no audit ran.
export function auditChainState(audit: AuditParse | null): ChainState | null {
  return audit ? audit.chainState : null;
}
