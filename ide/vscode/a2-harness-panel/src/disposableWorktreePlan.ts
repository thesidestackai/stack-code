// Disposable worktree plan model (pure) — Tier 3 Foundation v0.
//
// Validates a PROPOSED disposable-worktree plan (intended worktree path +
// mutation branch + base) per docs/a2-tier3-disposable-worktree-mutation-scope.md
// §6. This module is PURE and creates NOTHING: it never runs git, never makes a
// worktree, never writes a file. It only checks that a plan is well-formed and
// safe-by-construction so the panel can show it before any (separately approved,
// later) creation step.

// The canonical roots Tier 3 disposable worktrees must live under, and the
// control checkout that must never be the execution worktree.
export const DISPOSABLE_WORKTREE_ROOT = "/mnt/vast-data/git-worktrees/";
export const CONTROL_CHECKOUT = "/home/suki/stack-code";

export interface WorktreePlan {
  // Absolute path of the intended disposable worktree.
  worktreePath: string;
  // The unique mutation branch name.
  branch: string;
  // The base the worktree is created from (must be origin/main).
  base: string;
}

export interface PlanValidation {
  valid: boolean;
  // Human-readable reasons a plan is invalid (empty when valid).
  problems: string[];
}

function isNonEmpty(s: unknown): s is string {
  return typeof s === "string" && s.trim().length > 0;
}

// Normalize a POSIX-style absolute path string (resolve "." and ".." segments)
// WITHOUT touching the filesystem. Used to reason about containment safely.
export function normalizeAbs(p: string): string {
  if (!isNonEmpty(p) || p[0] !== "/") {
    return p; // not an absolute path; leave as-is for the caller to reject
  }
  const out: string[] = [];
  for (const seg of p.split("/")) {
    if (seg === "" || seg === ".") {
      continue;
    }
    if (seg === "..") {
      out.pop();
      continue;
    }
    out.push(seg);
  }
  return "/" + out.join("/");
}

// True when childPath is the dir itself or strictly inside it (path-string only).
export function isUnder(dir: string, childPath: string): boolean {
  const d = normalizeAbs(dir).replace(/\/+$/, "");
  const c = normalizeAbs(childPath).replace(/\/+$/, "");
  if (c === d) {
    return true;
  }
  return c.startsWith(d + "/");
}

// Validate a worktree plan. Safe-by-construction checks only; creates nothing.
export function validateWorktreePlan(plan: WorktreePlan | null | undefined): PlanValidation {
  const problems: string[] = [];
  if (!plan) {
    return { valid: false, problems: ["no worktree plan provided"] };
  }
  if (!isNonEmpty(plan.worktreePath) || plan.worktreePath[0] !== "/") {
    problems.push("worktree path must be a non-empty absolute path");
  }
  if (!isNonEmpty(plan.branch)) {
    problems.push("mutation branch must be a non-empty name");
  }
  if (!isNonEmpty(plan.base)) {
    problems.push("base must be set");
  } else if (plan.base.trim() !== "origin/main") {
    problems.push("base must be origin/main");
  }

  if (isNonEmpty(plan.worktreePath) && plan.worktreePath[0] === "/") {
    const wt = normalizeAbs(plan.worktreePath);
    // Must live under the disposable worktree root.
    if (!isUnder(DISPOSABLE_WORKTREE_ROOT, wt)) {
      problems.push("worktree path must be under " + DISPOSABLE_WORKTREE_ROOT);
    }
    // Must never be (or be under) the control checkout.
    if (isUnder(CONTROL_CHECKOUT, wt) || isUnder(wt, CONTROL_CHECKOUT)) {
      problems.push("worktree path must not be the control checkout or contain it");
    }
  }

  // The mutation branch must be a dedicated feature/verify-style branch, never
  // main and never an obviously-protected ref.
  if (isNonEmpty(plan.branch)) {
    const b = plan.branch.trim();
    if (b === "main" || b === "master" || b === "origin/main") {
      problems.push("mutation branch must not be main/master");
    }
    if (/\s/.test(b)) {
      problems.push("mutation branch must not contain whitespace");
    }
  }

  return { valid: problems.length === 0, problems };
}

// Render-ready summary lines for a plan (no creation; display only).
export function summarizePlan(plan: WorktreePlan | null | undefined): string[] {
  if (!plan) {
    return ["(no disposable worktree plan)"];
  }
  return [
    "worktree: " + (isNonEmpty(plan.worktreePath) ? plan.worktreePath : "(not set)"),
    "branch: " + (isNonEmpty(plan.branch) ? plan.branch : "(not set)"),
    "base: " + (isNonEmpty(plan.base) ? plan.base : "(not set)"),
    "creation: not performed (plan only in v0)",
  ];
}
