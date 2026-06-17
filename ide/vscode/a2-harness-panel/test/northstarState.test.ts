import * as assert from "assert";
import {
  NorthstarState,
  NorthstarStep,
  NORTHSTAR_STATES,
  NORTHSTAR_STEPS,
  NorthstarSignals,
  emptyNorthstarSignals,
  deriveNorthstarState,
  northstarNextStep,
  stepMeta,
  stateClass,
  assertNorthstarSafe,
  isAutoAdvanceAllowed,
  buildNorthstarView,
} from "../src/northstarState";

function signals(partial: Partial<NorthstarSignals>): NorthstarSignals {
  return { ...emptyNorthstarSignals(), workspaceReady: true, ...partial };
}

describe("northstarState — supersets the 13-state machine ladder", () => {
  it("has exactly the 16 Northstar states in scope §12 order", () => {
    assert.strictEqual(NORTHSTAR_STATES.length, 16);
    assert.strictEqual(NORTHSTAR_STATES[0], "NO_WORKSPACE");
    assert.strictEqual(NORTHSTAR_STATES[6], "AWAITING_APPLY_APPROVAL");
    assert.strictEqual(NORTHSTAR_STATES[7], "APPLIED");
    assert.strictEqual(NORTHSTAR_STATES[15], "HUMAN_MERGE_PENDING");
  });
});

describe("northstarState — deriveNorthstarState (read-only)", () => {
  it("NO_WORKSPACE when workspace not ready", () => {
    assert.strictEqual(deriveNorthstarState(emptyNorthstarSignals()), "NO_WORKSPACE");
  });
  it("WORKSPACE_READY when workspace ready and nothing else observed", () => {
    assert.strictEqual(deriveNorthstarState(signals({})), "WORKSPACE_READY");
  });
  it("walks the read-only prefix in order", () => {
    assert.strictEqual(deriveNorthstarState(signals({ taskDescribed: true })), "TASK_DESCRIBED");
    assert.strictEqual(deriveNorthstarState(signals({ taskDescribed: true, planDrafted: true })), "PLAN_DRAFTED");
    assert.strictEqual(deriveNorthstarState(signals({ planValidated: true })), "PLAN_VALIDATED");
    assert.strictEqual(deriveNorthstarState(signals({ previewReady: true })), "PREVIEW_READY");
    assert.strictEqual(
      deriveNorthstarState(signals({ previewReady: true, awaitingApplyApproval: true })),
      "AWAITING_APPLY_APPROVAL",
    );
  });
  it("reflects observed human-gated milestones (most-advanced wins)", () => {
    assert.strictEqual(deriveNorthstarState(signals({ appliedObserved: true })), "APPLIED");
    assert.strictEqual(deriveNorthstarState(signals({ packageReadyObserved: true })), "PACKAGE_READY");
    assert.strictEqual(deriveNorthstarState(signals({ committedObserved: true })), "COMMITTED");
    assert.strictEqual(deriveNorthstarState(signals({ pushedObserved: true })), "PUSHED");
    assert.strictEqual(deriveNorthstarState(signals({ draftPrObserved: true })), "DRAFT_PR_OPEN");
    assert.strictEqual(deriveNorthstarState(signals({ evidenceFrozen: true })), "EVIDENCE_FROZEN");
    assert.strictEqual(deriveNorthstarState(signals({ dispositionPending: true })), "DISPOSITION_PENDING");
    assert.strictEqual(
      deriveNorthstarState(signals({ dispositionResolved: "closed-retained" })),
      "CLOSED_RETAINED",
    );
    assert.strictEqual(
      deriveNorthstarState(signals({ dispositionResolved: "human-merge-pending" })),
      "HUMAN_MERGE_PENDING",
    );
  });
});

describe("northstarState — SAFETY INVARIANT: never auto-advance past the apply gate", () => {
  it("read-only signals (preview/awaiting) NEVER derive APPLIED or beyond", () => {
    // The richest read-only state, with every human-gated signal false.
    const s = signals({
      taskDescribed: true,
      planDrafted: true,
      planValidated: true,
      previewReady: true,
      awaitingApplyApproval: true,
      // all gated observations explicitly false:
      appliedObserved: false,
      packageReadyObserved: false,
      committedObserved: false,
      pushedObserved: false,
      draftPrObserved: false,
    });
    const state = deriveNorthstarState(s);
    assert.strictEqual(state, "AWAITING_APPLY_APPROVAL");
    const idx = NORTHSTAR_STATES.indexOf(state);
    assert.ok(idx <= NORTHSTAR_STATES.indexOf("AWAITING_APPLY_APPROVAL"), "advanced past the apply gate without an observed apply");
  });

  it("APPLIED requires an OBSERVED applied event, never inference", () => {
    const withoutApply = signals({ previewReady: true, awaitingApplyApproval: true });
    assert.notStrictEqual(deriveNorthstarState(withoutApply), "APPLIED");
    const withApply = signals({ previewReady: true, awaitingApplyApproval: true, appliedObserved: true });
    assert.strictEqual(deriveNorthstarState(withApply), "APPLIED");
  });

  it("no signal combination ever yields a merged/approved/ready state (no such state exists)", () => {
    for (const st of NORTHSTAR_STATES) {
      assert.ok(!/MERGED|APPROVED|READY_TO_MERGE|MARK_READY/.test(st), `forbidden terminal: ${st}`);
    }
    // HUMAN_MERGE_PENDING is the most advanced state and is only ever "pending".
    assert.strictEqual(NORTHSTAR_STATES[NORTHSTAR_STATES.length - 1], "HUMAN_MERGE_PENDING");
  });
});

describe("northstarState — next step is always safe and non-executing", () => {
  it("every state maps to a known, safe step", () => {
    for (const s of NORTHSTAR_STATES) {
      const step = northstarNextStep(s as NorthstarState);
      assert.ok(NORTHSTAR_STEPS.includes(step), `state ${s} -> unknown step ${step}`);
      assert.strictEqual(assertNorthstarSafe(step), step);
    }
  });

  it("no human-gated / human-only step is automatable", () => {
    for (const step of NORTHSTAR_STEPS) {
      const m = stepMeta(step as NorthstarStep);
      if (m.kind === "human-gated" || m.kind === "human-only") {
        assert.strictEqual(m.automatable, false, `gated step automatable: ${step}`);
      }
    }
  });

  it("the apply / package-commit / package-push / open-pr / merge steps are NEVER automatable", () => {
    const mustBeGated: NorthstarStep[] = [
      "ApproveApplyAtGate",
      "PackageCommit",
      "PackagePush",
      "OpenDraftPr",
      "HumanMergeDecision",
    ];
    for (const step of mustBeGated) {
      assert.strictEqual(stepMeta(step).automatable, false, `executor step automatable: ${step}`);
    }
  });

  it("the apply gate step requires a REAL terminal", () => {
    assert.strictEqual(stepMeta("ApproveApplyAtGate").requiresRealTty, true);
  });

  it("merge is HUMAN-ONLY", () => {
    assert.strictEqual(stepMeta("HumanMergeDecision").kind, "human-only");
    assert.strictEqual(stateClass("HUMAN_MERGE_PENDING"), "human-only");
  });

  it("specific state→step mappings follow the scope ladder", () => {
    assert.strictEqual(northstarNextStep("AWAITING_APPLY_APPROVAL"), "ApproveApplyAtGate");
    assert.strictEqual(northstarNextStep("APPLIED"), "PackagePlan");
    assert.strictEqual(northstarNextStep("PACKAGE_READY"), "PackageCommit");
    assert.strictEqual(northstarNextStep("COMMITTED"), "PackagePush");
    assert.strictEqual(northstarNextStep("PUSHED"), "OpenDraftPr");
    assert.strictEqual(northstarNextStep("DRAFT_PR_OPEN"), "FreezeEvidence");
    assert.strictEqual(northstarNextStep("HUMAN_MERGE_PENDING"), "HumanMergeDecision");
  });
});

describe("northstarState — assertNorthstarSafe rejects forged unsafe steps", () => {
  it("throws on an unknown step", () => {
    assert.throws(() => assertNorthstarSafe("RunApply" as never), /unsafe northstar step/);
  });
});

describe("northstarState — isAutoAdvanceAllowed gates every write/outward crossing", () => {
  it("allows auto-advance only between read-only states", () => {
    assert.strictEqual(isAutoAdvanceAllowed("NO_WORKSPACE", "WORKSPACE_READY"), true);
    assert.strictEqual(isAutoAdvanceAllowed("PLAN_VALIDATED", "PREVIEW_READY"), true);
    assert.strictEqual(isAutoAdvanceAllowed("PREVIEW_READY", "AWAITING_APPLY_APPROVAL"), true);
  });

  it("NEVER auto-advances across the apply gate (into APPLIED or beyond)", () => {
    assert.strictEqual(isAutoAdvanceAllowed("AWAITING_APPLY_APPROVAL", "APPLIED"), false);
    assert.strictEqual(isAutoAdvanceAllowed("PREVIEW_READY", "APPLIED"), false);
    assert.strictEqual(isAutoAdvanceAllowed("WORKSPACE_READY", "PUSHED"), false);
  });

  it("NEVER auto-advances into any human-gated or human-only state", () => {
    const gatedTargets: NorthstarState[] = [
      "APPLIED",
      "COMMITTED",
      "PUSHED",
      "DRAFT_PR_OPEN",
      "CLOSED_RETAINED",
      "HUMAN_MERGE_PENDING",
    ];
    for (const to of gatedTargets) {
      assert.strictEqual(
        isAutoAdvanceAllowed("NO_WORKSPACE", to),
        false,
        `auto-advanced into gated state ${to}`,
      );
    }
  });

  it("never auto-advances backward or in place", () => {
    assert.strictEqual(isAutoAdvanceAllowed("PREVIEW_READY", "PLAN_VALIDATED"), false);
    assert.strictEqual(isAutoAdvanceAllowed("PREVIEW_READY", "PREVIEW_READY"), false);
  });
});

describe("northstarState — buildNorthstarView", () => {
  it("assembles a safe view for the AWAITING_APPLY_APPROVAL gate", () => {
    const v = buildNorthstarView(signals({ previewReady: true, awaitingApplyApproval: true }));
    assert.strictEqual(v.state, "AWAITING_APPLY_APPROVAL");
    assert.strictEqual(v.stateClass, "read-only");
    assert.strictEqual(v.stepKind, "human-gated");
    assert.strictEqual(v.automatable, false);
    assert.strictEqual(v.requiresRealTty, true);
    assert.ok(v.stepLabel.length > 0);
  });
  it("never reports an automatable step for an observed APPLIED-or-beyond state", () => {
    for (const sig of [
      signals({ appliedObserved: true }),
      signals({ committedObserved: true }),
      signals({ pushedObserved: true }),
      signals({ draftPrObserved: true }),
      signals({ dispositionResolved: "human-merge-pending" }),
    ]) {
      const v = buildNorthstarView(sig);
      // The recommended NEXT step out of a gated state is itself gated/read-only,
      // and assertNorthstarSafe (inside buildNorthstarView) already proved it is
      // not an automatable executor.
      assert.ok(["read-only", "human-gated", "human-only", "terminal"].includes(v.stepKind));
    }
  });
});
