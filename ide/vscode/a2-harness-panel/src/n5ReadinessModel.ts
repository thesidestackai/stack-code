// Northstar Phase N5 — PACKAGE LADDER READINESS MODEL (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n5-gated-execution-boundary-scope.md §9.
//
// Derives per-rung readiness for the 4-rung package ladder (package-plan →
// package-commit → package-push → package-pr) from read-only N3/N4 state.
// PURE: no fs, no spawn, no network. N5 NEVER runs any rung.
//
// Rung readiness: READY | NOT_READY | BLOCKED | EXECUTION_REQUIRED
//   READY              preconditions VERIFIED; evidence consistent; ready for a separate lane.
//   NOT_READY          one or more preconditions not yet VERIFIED (missing/inferred, not blocked).
//   BLOCKED            a blocking condition holds; fail closed.
//   EXECUTION_REQUIRED cannot be proven from read-only data; separate execution lane required.

import { N4State } from "./n4State";
import { N5TrustLevel } from "./n5TrustLevel";

export type RungReadiness = "READY" | "NOT_READY" | "BLOCKED" | "EXECUTION_REQUIRED";

export interface RungPrecondition {
  label: string;
  trust: N5TrustLevel;
  met: boolean;
}

export interface RungReadinessResult {
  rung: string;
  purpose: string;
  preconditions: RungPrecondition[];
  evidencePresent: boolean;
  operatorConfirmationRequired: boolean;
  readiness: RungReadiness;
  // Honest note shown to the operator.
  note: string;
}

export interface PackageLadderReadiness {
  packagePlan: RungReadinessResult;
  packageCommit: RungReadinessResult;
  packagePush: RungReadinessResult;
  packagePr: RungReadinessResult;
}

export interface LadderReadinessOpts {
  planNonExecutable: boolean;
  targetSafe: boolean;
  evidencePresent: boolean;
  planValidated: boolean;
}

// Derive the package ladder readiness from the current N4 state and read-only
// N3/N4 inputs. No fact is guessed; execution-required facts are labelled
// EXECUTION_REQUIRED, never assumed READY. N5 never runs any rung.
export function deriveLadderReadiness(
  n4State: N4State,
  opts: LadderReadinessOpts,
): PackageLadderReadiness {
  const n4Blocked =
    n4State === "N4_BLOCKED_UNSAFE_TARGET" ||
    n4State === "N4_BLOCKED_EXECUTABLE_STEP" ||
    n4State === "N4_BLOCKED_AMBIGUOUS_ARTIFACTS";
  const n4EvidenceReady = n4State === "N4_EVIDENCE_READY";

  // package-plan: all preconditions are derivable from read-only N3/N4 state.
  const planPreconditions: RungPrecondition[] = [
    {
      label: "N4 preview/diff VERIFIED",
      trust: n4EvidenceReady ? "VERIFIED" : n4Blocked ? "BLOCKED" : "MISSING",
      met: n4EvidenceReady,
    },
    {
      label: "plan non-executable (descriptive only)",
      trust: opts.planNonExecutable ? "VERIFIED" : "BLOCKED",
      met: opts.planNonExecutable,
    },
    {
      label: "target safe (no forbidden family)",
      trust: opts.targetSafe ? "VERIFIED" : "BLOCKED",
      met: opts.targetSafe,
    },
    {
      label: "plan draft validated",
      trust: opts.planValidated ? "VERIFIED" : "MISSING",
      met: opts.planValidated,
    },
  ];

  const planAnyBlocked = planPreconditions.some((p) => p.trust === "BLOCKED");
  const planAllMet = planPreconditions.every((p) => p.met);
  let planReadiness: RungReadiness;
  if (planAnyBlocked) {
    planReadiness = "BLOCKED";
  } else if (!planAllMet) {
    planReadiness = "NOT_READY";
  } else {
    planReadiness = "READY";
  }

  // package-commit through package-pr: preconditions require live execution to
  // verify. N5 shows EXECUTION_REQUIRED — it never runs or guesses.
  const executionRequiredPreconditions: RungPrecondition[] = [
    {
      label: "previous rung completed (requires execution lane to verify)",
      trust: "EXECUTION_REQUIRED",
      met: false,
    },
  ];

  return {
    packagePlan: {
      rung: "package-plan",
      purpose: "assemble the change package from the validated N4-reviewed plan draft",
      preconditions: planPreconditions,
      evidencePresent: opts.evidencePresent,
      operatorConfirmationRequired: false,
      readiness: planReadiness,
      note:
        planReadiness === "READY"
          ? "Ready for a separate approved execution lane. N5 does not run this rung."
          : planReadiness === "BLOCKED"
            ? "BLOCKED — a precondition is unsafe or ambiguous. Fail closed."
            : "NOT_READY — one or more preconditions are not yet VERIFIED.",
    },
    packageCommit: {
      rung: "package-commit",
      purpose: "commit the assembled package (requires package-plan to have run in a separate lane)",
      preconditions: executionRequiredPreconditions,
      evidencePresent: false,
      operatorConfirmationRequired: true,
      readiness: "EXECUTION_REQUIRED",
      note: "EXECUTION_REQUIRED — cannot be proven from read-only data. Requires a separate approved execution lane.",
    },
    packagePush: {
      rung: "package-push",
      purpose: "push the branch (requires package-commit to have run in a separate lane)",
      preconditions: executionRequiredPreconditions,
      evidencePresent: false,
      operatorConfirmationRequired: true,
      readiness: "EXECUTION_REQUIRED",
      note: "EXECUTION_REQUIRED — cannot be proven from read-only data. Requires a separate approved execution lane.",
    },
    packagePr: {
      rung: "package-pr",
      purpose:
        "open the change PR (draft-only intent; requires package-push to have run in a separate lane)",
      preconditions: executionRequiredPreconditions,
      evidencePresent: false,
      operatorConfirmationRequired: true,
      readiness: "EXECUTION_REQUIRED",
      note: "EXECUTION_REQUIRED — cannot be proven from read-only data. Requires a separate approved execution lane (draft-only; PR-open token required).",
    },
  };
}
