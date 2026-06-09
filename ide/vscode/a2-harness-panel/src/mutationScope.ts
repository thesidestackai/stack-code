// Declared mutation scope model (pure) — Tier 3 Foundation v0.
//
// Decides whether a candidate write path is in the operator-declared exact-path
// set AND safely inside the disposable worktree, per
// docs/a2-tier3-disposable-worktree-mutation-scope.md §8. This module is PURE and
// performs NO writes: it only classifies a path as accepted or rejected so the
// panel (and a future, separately approved executor) can enforce exact-path
// scoping. It never touches the filesystem.

import { normalizeAbs, isUnder, CONTROL_CHECKOUT } from "./disposableWorktreePlan";

export interface MutationScopeInput {
  // The disposable worktree root for this lane (mutation may only occur inside).
  worktreeRoot: string;
  // The exact declared touched-file paths (absolute, inside the worktree).
  // Immutable for the lane once approved.
  declaredPaths: string[];
}

export type ScopeDecision = "accepted" | "rejected";

export interface ScopeResult {
  decision: ScopeDecision;
  reason: string;
}

function isNonEmpty(s: unknown): s is string {
  return typeof s === "string" && s.trim().length > 0;
}

// Normalize + de-duplicate the declared set. Drops empties; does NOT validate
// containment here (validateDeclaredSet does that).
export function normalizeDeclared(paths: ReadonlyArray<string>): string[] {
  const out: string[] = [];
  for (const p of paths || []) {
    if (isNonEmpty(p)) {
      const n = normalizeAbs(p.trim());
      if (!out.includes(n)) {
        out.push(n);
      }
    }
  }
  return out;
}

// Validate that every declared path is absolute, inside the worktree root, and
// never under the control checkout. Returns the problems (empty when valid).
export function validateDeclaredSet(input: MutationScopeInput): string[] {
  const problems: string[] = [];
  if (!isNonEmpty(input.worktreeRoot) || input.worktreeRoot[0] !== "/") {
    problems.push("worktree root must be a non-empty absolute path");
    return problems;
  }
  const declared = normalizeDeclared(input.declaredPaths);
  if (declared.length === 0) {
    problems.push("no declared touched files");
  }
  for (const p of declared) {
    if (p[0] !== "/") {
      problems.push("declared path must be absolute: " + p);
      continue;
    }
    if (!isUnder(input.worktreeRoot, p)) {
      problems.push("declared path is outside the disposable worktree: " + p);
    }
    if (isUnder(CONTROL_CHECKOUT, p)) {
      problems.push("declared path resolves under the control checkout: " + p);
    }
  }
  return problems;
}

// Classify a candidate write path. Accepted ONLY when the path is in the declared
// set, inside the worktree root, and not under the control checkout. Everything
// else is rejected (deny-by-default). Pure string reasoning; no fs.
export function classifyWrite(candidate: string, input: MutationScopeInput): ScopeResult {
  if (!isNonEmpty(candidate) || candidate[0] !== "/") {
    return { decision: "rejected", reason: "rejected: write path must be a non-empty absolute path" };
  }
  const c = normalizeAbs(candidate);
  if (isUnder(CONTROL_CHECKOUT, c)) {
    return { decision: "rejected", reason: "rejected: path resolves under the control checkout" };
  }
  if (!isNonEmpty(input.worktreeRoot) || !isUnder(input.worktreeRoot, c)) {
    return { decision: "rejected", reason: "rejected: path is outside the disposable worktree" };
  }
  const declared = normalizeDeclared(input.declaredPaths);
  if (!declared.includes(c)) {
    return { decision: "rejected", reason: "rejected: path is not in the declared touched-file set" };
  }
  return { decision: "accepted", reason: "accepted: in declared set, inside the disposable worktree" };
}
