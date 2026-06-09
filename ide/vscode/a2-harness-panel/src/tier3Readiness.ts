// Tier 3 readiness model (pure, guard-safe) — Tier 3 Foundation v0.
//
// Computes an HONEST readiness view for a Tier 3 (disposable worktree mutation)
// lane, per docs/a2-tier3-disposable-worktree-mutation-scope.md §§5,7,16. This
// module is PURE: it turns optional, already-gathered facts into an explicit
// per-dimension status. It performs NO IO — no fs, no process spawn, no network,
// no watcher, no timer — and it enables NO mutation. It only decides whether a
// Tier 3 lane WOULD be ready, and renders not-checked when a fact is unprobed.
//
// Guard-safe rule (CRITICAL): the panel guards forbid fs/spawn/watcher/timer, so
// v0 wires NO git probe. When a fact is not supplied, its dimension renders
// "not-checked" with a stated reason — readiness is NEVER green-by-default and
// git/worktree state is NEVER fabricated. A future, separately approved lane may
// supply guard-safe facts to this same pure model.

export type Tri = "yes" | "no" | "not-checked";

// Optional, already-gathered facts. Absent in v0 → not-checked.
export interface Tier3Facts {
  // Is the control checkout (/home/suki/stack-code) clean (no staged/unstaged/
  // untracked tracked changes)?
  controlCheckoutClean?: boolean;
  // Has origin/main been fetched and confirmed as the base?
  originMainConfirmed?: boolean;
  // Does the intended disposable worktree path NOT already exist?
  worktreePathFree?: boolean;
  // Does the intended mutation branch NOT already exist?
  branchNameFree?: boolean;
  // Has the operator explicitly approved this exact Tier 3 lane?
  operatorApproved?: boolean;
}

export interface Tier3ReadinessInput {
  facts?: Tier3Facts;
  // Whether the disposable worktree plan validated (see disposableWorktreePlan).
  planValid: boolean;
  // Whether at least one declared touched-file path is set.
  declaredScopePresent: boolean;
  // Whether the denied-command registry is loaded/available.
  deniedRegistryLoaded: boolean;
  // The reason there is no guard-safe probe (shown when facts are absent).
  noProbeReason?: string;
}

export interface Tier3Readiness {
  controlCheckoutClean: Tri;
  originMainConfirmed: Tri;
  worktreePathFree: Tri;
  branchNameFree: Tri;
  operatorApproved: Tri;
  planValid: "yes" | "no";
  declaredScopePresent: "yes" | "no";
  deniedRegistryLoaded: "yes" | "no";
  // The single honest verdict: ready ONLY when every gate is affirmatively yes.
  // Any not-checked or no keeps it not-ready (never ready-by-default).
  overall: "ready" | "not-ready";
  // The reason git/worktree readiness is not-checked, when applicable.
  probeNote: string | null;
}

function tri(v: boolean | undefined): Tri {
  if (v === undefined) {
    return "not-checked";
  }
  return v ? "yes" : "no";
}

const DEFAULT_NO_PROBE_REASON =
  "no guard-safe Tier 3 probe wired in v0 (panel forbids fs/spawn/watcher/timer; control-checkout/origin/worktree/branch facts must come from a future, separately approved guard-safe probe)";

export function computeTier3Readiness(input: Tier3ReadinessInput): Tier3Readiness {
  const f = input.facts ?? {};
  const dims: Tri[] = [
    tri(f.controlCheckoutClean),
    tri(f.originMainConfirmed),
    tri(f.worktreePathFree),
    tri(f.branchNameFree),
    tri(f.operatorApproved),
  ];
  const hasAnyFact =
    f.controlCheckoutClean !== undefined ||
    f.originMainConfirmed !== undefined ||
    f.worktreePathFree !== undefined ||
    f.branchNameFree !== undefined ||
    f.operatorApproved !== undefined;

  // Ready ONLY when every gated dimension is affirmatively yes AND the plan is
  // valid, a declared scope is present, and the denied registry is loaded.
  const allFactsYes = dims.every((d) => d === "yes");
  const overall: "ready" | "not-ready" =
    allFactsYes && input.planValid && input.declaredScopePresent && input.deniedRegistryLoaded
      ? "ready"
      : "not-ready";

  return {
    controlCheckoutClean: dims[0],
    originMainConfirmed: dims[1],
    worktreePathFree: dims[2],
    branchNameFree: dims[3],
    operatorApproved: dims[4],
    planValid: input.planValid ? "yes" : "no",
    declaredScopePresent: input.declaredScopePresent ? "yes" : "no",
    deniedRegistryLoaded: input.deniedRegistryLoaded ? "yes" : "no",
    overall,
    probeNote: hasAnyFact ? null : (input.noProbeReason ?? DEFAULT_NO_PROBE_REASON),
  };
}

// A dirty control checkout is a hard not-ready and should surface prominently.
export function dirtyControlCheckoutBlock(readiness: Tier3Readiness): boolean {
  return readiness.controlCheckoutClean === "no";
}
