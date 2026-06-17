// Read-only WORKSPACE STATUS CARD model (pure) — Phase N2.
//
// Source of truth: docs/stack-code-northstar-ux-gap-scope-2026-06-17.md §11
// (workspace status card) + §15.
//
// The card is the Northstar workspace-first surface: on panel open the
// extension auto-detects the workspace ROOT (via vscode.workspace folders — no
// node fs, no spawn) and renders this card immediately. Deeper git facts
// (branch, clean/dirty, origin/main freshness) require a read-only git probe
// that the print/validate-only helper does not yet emit; until that probe is
// wired in a later phase, those fields are reported HONESTLY as "unknown" —
// never green-by-default. This mirrors the existing setupStatus / foundation
// "not-checked" honesty (FoundationReadinessView.gitProbeNote).
//
// This module is PURE: no fs, no spawn, no network, no watcher, no polling. It
// only shapes already-gathered read-only inputs into a display model.

export type Cleanliness = "clean" | "dirty" | "unknown";
export type Freshness = "current" | "behind" | "ahead" | "diverged" | "unknown";

// Read-only probe the extension assembles. workspaceRoot comes from the vscode
// workspace folder (available on open). The git fields are optional: a later-
// phase read-only git probe fills them; absent → unknown.
export interface WorkspaceProbe {
  workspaceRoot: string | null;
  branch: string | null;
  // null => the panel has no guard-safe way to know yet (honest unknown).
  worktreeClean: boolean | null;
  originMainFreshness: Freshness | null;
}

export function emptyWorkspaceProbe(): WorkspaceProbe {
  return {
    workspaceRoot: null,
    branch: null,
    worktreeClean: null,
    originMainFreshness: null,
  };
}

export type Readiness = "ready" | "needs-attention" | "unknown";

export interface WorkspaceStatusCard {
  workspace: "detected" | "not-detected";
  workspaceRoot: string | null;
  branch: "known" | "unknown";
  branchName: string | null;
  cleanliness: Cleanliness;
  originMainFreshness: Freshness;
  // Overall, honest read-only readiness to START a lane here.
  readiness: Readiness;
  // Why git facts may be unknown — shown to the operator (never green-by-default).
  gitProbeNote: string | null;
}

function isSet(v: string | null | undefined): boolean {
  return typeof v === "string" && v.trim().length > 0;
}

function cleanliness(probe: WorkspaceProbe): Cleanliness {
  if (probe.worktreeClean === null) {
    return "unknown";
  }
  return probe.worktreeClean ? "clean" : "dirty";
}

function freshness(probe: WorkspaceProbe): Freshness {
  return probe.originMainFreshness ?? "unknown";
}

// Honest, read-only readiness:
//   - no workspace            -> needs-attention (cannot start a lane).
//   - detected, clean, current-> ready.
//   - detected, dirty or not  -> needs-attention.
//     current/ahead/behind/diverged
//   - detected but git facts  -> unknown (we will not claim ready without proof).
//     not yet probed
function computeReadiness(
  workspaceDetected: boolean,
  clean: Cleanliness,
  fresh: Freshness,
): Readiness {
  if (!workspaceDetected) {
    return "needs-attention";
  }
  if (clean === "unknown" || fresh === "unknown") {
    return "unknown";
  }
  if (clean === "clean" && fresh === "current") {
    return "ready";
  }
  return "needs-attention";
}

export function computeWorkspaceStatusCard(probe: WorkspaceProbe): WorkspaceStatusCard {
  const detected = isSet(probe.workspaceRoot);
  const clean = cleanliness(probe);
  const fresh = freshness(probe);
  const branchKnown = isSet(probe.branch);
  const gitUnknown = clean === "unknown" || fresh === "unknown" || !branchKnown;
  return {
    workspace: detected ? "detected" : "not-detected",
    workspaceRoot: isSet(probe.workspaceRoot) ? (probe.workspaceRoot as string).trim() : null,
    branch: branchKnown ? "known" : "unknown",
    branchName: branchKnown ? (probe.branch as string).trim() : null,
    cleanliness: clean,
    originMainFreshness: fresh,
    readiness: computeReadiness(detected, clean, fresh),
    gitProbeNote: gitUnknown
      ? "branch / clean-dirty / origin-main freshness need a read-only git probe the print/validate-only helper does not yet emit (a later Northstar phase wires it)"
      : null,
  };
}

// Pre-format the card as read-only display lines (label : value). The render
// layer can show these verbatim; tests assert on them. No fs, no spawn.
export function renderWorkspaceStatusLines(card: WorkspaceStatusCard): string[] {
  return [
    `workspace: ${card.workspace}`,
    `workspace root: ${card.workspaceRoot ?? "(none)"}`,
    `branch: ${card.branch === "known" ? (card.branchName as string) : "unknown"}`,
    `worktree: ${card.cleanliness}`,
    `origin/main: ${card.originMainFreshness}`,
    `readiness: ${card.readiness}`,
  ];
}
