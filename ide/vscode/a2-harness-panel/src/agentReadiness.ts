// Agent readiness model (pure, guard-safe) — A2 Local Coding Agent Foundation v0.
//
// Computes an HONEST readiness view for the agent cockpit
// (docs/a2-local-coding-agent-foundation-scope.md §9). This module is PURE: it
// turns optional, already-gathered signals into an explicit per-dimension
// status. It performs NO IO of its own — no `fs`, no process spawn, no network,
// no watcher, no timer.
//
// Guard-safe git probe rule (CRITICAL): the package guards forbid `fs`,
// child_process, watchers, and timers in panel source. v0 therefore does NOT
// wire a git probe. When no git facts are supplied, every git dimension renders
// honestly as "not-checked" with a stated reason — readiness is NEVER
// green-by-default and git state is NEVER fabricated. A future, separately
// approved lane may supply guard-safe git facts (e.g. via the read-only VS Code
// Git API) to this same pure model.

import { TierId } from "./permissionTiers";

export type Tri = "yes" | "no" | "not-checked";

// Optional git facts. When omitted (the v0 default), the git dimensions render
// as "not-checked". A fact is used only when explicitly provided.
export interface GitFacts {
  repoDetected?: boolean;
  gitBranch?: string | null;
  dirty?: boolean;
  staged?: boolean;
  unstaged?: boolean;
  untracked?: boolean;
}

export interface ReadinessInput {
  workspaceRoot: string | null;
  // The current effective tier (0-2 in v0).
  currentTier: TierId;
  // Whether the global denied-command registry is loaded/available.
  deniedRegistryLoaded: boolean;
  // The safe-executor mode label (e.g. "print-validate-only").
  safeExecutorMode: string;
  // Optional git facts; absent in v0 → not-checked.
  git?: GitFacts;
  // The stated reason there is no guard-safe git probe (shown when git facts
  // are absent).
  noGitProbeReason?: string;
}

export interface AgentReadiness {
  workspaceRoot: "detected" | "not-detected";
  repoDetected: Tri;
  gitBranch: string | "not-checked";
  dirtyState: Tri;
  stagedChanges: Tri;
  unstagedChanges: Tri;
  untrackedFiles: Tri;
  currentTier: TierId;
  deniedRegistryLoaded: "yes" | "no";
  safeExecutorMode: string;
  // The reason git readiness is not-checked, when applicable.
  gitProbeNote: string | null;
}

function triFromBool(v: boolean | undefined): Tri {
  if (v === undefined) {
    return "not-checked";
  }
  return v ? "yes" : "no";
}

const DEFAULT_NO_GIT_PROBE_REASON =
  "no guard-safe git probe wired in v0 (panel forbids fs/spawn/watcher; git facts must come from a future, separately approved guard-safe probe)";

export function computeReadiness(input: ReadinessInput): AgentReadiness {
  const git = input.git ?? {};
  const hasAnyGitFact =
    git.repoDetected !== undefined ||
    git.dirty !== undefined ||
    git.staged !== undefined ||
    git.unstaged !== undefined ||
    git.untracked !== undefined ||
    (git.gitBranch !== undefined && git.gitBranch !== null);

  const wsSet = typeof input.workspaceRoot === "string" && input.workspaceRoot.trim().length > 0;

  return {
    workspaceRoot: wsSet ? "detected" : "not-detected",
    repoDetected: triFromBool(git.repoDetected),
    gitBranch:
      typeof git.gitBranch === "string" && git.gitBranch.trim().length > 0
        ? git.gitBranch
        : "not-checked",
    dirtyState: triFromBool(git.dirty),
    stagedChanges: triFromBool(git.staged),
    unstagedChanges: triFromBool(git.unstaged),
    untrackedFiles: triFromBool(git.untracked),
    currentTier: input.currentTier,
    deniedRegistryLoaded: input.deniedRegistryLoaded ? "yes" : "no",
    safeExecutorMode: input.safeExecutorMode,
    gitProbeNote: hasAnyGitFact ? null : (input.noGitProbeReason ?? DEFAULT_NO_GIT_PROBE_REASON),
  };
}

// True when a dirty-checkout WARNING should be shown. v0: only when a real
// dirty fact says so — never a fabricated warning, and never a false all-clear
// (when dirty is not-checked, this returns false but the dirtyState renders as
// "not-checked", so the operator sees the honest unknown).
export function dirtyCheckoutWarning(readiness: AgentReadiness): boolean {
  return readiness.dirtyState === "yes";
}
