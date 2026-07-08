import * as assert from "assert";
import {
  N6_STATES,
  N6_RUNG_EXEC_STATES,
  N6_FORBIDDEN_TARGETS,
  N6_SUB_TOKEN_PLAN,
  N6_SUB_TOKEN_COMMIT,
  N6_SUB_TOKEN_PUSH,
  N6_SUB_TOKEN_PR,
  assertN6Safe,
  deriveN6RungStateName,
  n6RungStateNote,
} from "../src/n6State";
import { N5_FORBIDDEN_TARGETS } from "../src/n5State";

describe("N6State — state machine and forbidden-target guard", () => {
  it("exports 23 distinct N6 states", () => {
    assert.strictEqual(N6_STATES.length, 23);
    const unique = new Set(N6_STATES);
    assert.strictEqual(unique.size, 23, "all 23 states must be distinct");
  });

  it("exports 5 per-rung exec states", () => {
    assert.strictEqual(N6_RUNG_EXEC_STATES.length, 5);
    const expected = ["AWAITING_TOKEN", "TOKEN_ACTIVE", "RUNNING", "DONE", "FAILED"];
    for (const s of expected) {
      assert.ok(N6_RUNG_EXEC_STATES.includes(s as never), `expected exec state: ${s}`);
    }
  });

  it("N6_FORBIDDEN_TARGETS is a strict superset of N5_FORBIDDEN_TARGETS", () => {
    for (const t of N5_FORBIDDEN_TARGETS) {
      assert.ok(
        N6_FORBIDDEN_TARGETS.includes(t),
        `N6 forbidden targets must include N5 forbidden target: ${t}`,
      );
    }
    assert.ok(
      N6_FORBIDDEN_TARGETS.length > N5_FORBIDDEN_TARGETS.length,
      "N6 must add at least one new forbidden target beyond N5",
    );
  });

  it("N6_FORBIDDEN_TARGETS includes all N6-specific additions", () => {
    const n6Additions = [
      "APPLY_EXECUTING",
      "APPLY_APPROVED",
      "APPLY_DONE",
      "PR_APPROVED",
      "PR_MERGED",
      "MERGED",
      "MODEL_CALL_EXECUTING",
      "BROKER_CALL_EXECUTING",
      "VAULT_READ_EXECUTING",
      "AUTO_APPROVED",
      "HIDDEN_APPLY",
      "PUSH_FORCE",
      "PR_MARK_READY",
    ];
    for (const t of n6Additions) {
      assert.ok(N6_FORBIDDEN_TARGETS.includes(t), `must include: ${t}`);
    }
  });

  it("sub-token strings are exact non-empty strings", () => {
    assert.strictEqual(N6_SUB_TOKEN_PLAN, "APPROVED: N6 Package Plan Only");
    assert.strictEqual(N6_SUB_TOKEN_COMMIT, "APPROVED: N6 Package Commit Only");
    assert.strictEqual(N6_SUB_TOKEN_PUSH, "APPROVED: N6 Package Push Only");
    assert.strictEqual(N6_SUB_TOKEN_PR, "APPROVED: N6 Draft PR Only");
    // All 4 sub-tokens are distinct.
    const tokens = [N6_SUB_TOKEN_PLAN, N6_SUB_TOKEN_COMMIT, N6_SUB_TOKEN_PUSH, N6_SUB_TOKEN_PR];
    assert.strictEqual(new Set(tokens).size, 4, "all 4 sub-tokens must be distinct");
  });

  it("assertN6Safe returns valid N6 states unchanged", () => {
    for (const s of N6_STATES) {
      assert.strictEqual(assertN6Safe(s), s);
    }
  });

  it("assertN6Safe throws on all N6_FORBIDDEN_TARGETS", () => {
    for (const t of N6_FORBIDDEN_TARGETS) {
      assert.throws(
        () => assertN6Safe(t),
        /unsafe N6 state/,
        `must throw on forbidden: ${t}`,
      );
    }
  });

  it("assertN6Safe throws on unknown states", () => {
    assert.throws(() => assertN6Safe("UNKNOWN_STATE"), /unknown N6 state/);
    assert.throws(() => assertN6Safe(""), /unknown N6 state/);
    assert.throws(() => assertN6Safe("N6_NONEXISTENT"), /unknown N6 state/);
  });

  it("deriveN6RungStateName maps all rung+exec combinations", () => {
    const expected: Array<[Parameters<typeof deriveN6RungStateName>[0], Parameters<typeof deriveN6RungStateName>[1], string]> = [
      ["plan", "AWAITING_TOKEN", "N6_AWAITING_PACKAGE_PLAN_TOKEN"],
      ["plan", "TOKEN_ACTIVE",   "N6_PACKAGE_PLAN_TOKEN_ACTIVE"],
      ["plan", "RUNNING",        "N6_PACKAGE_PLAN_RUNNING"],
      ["plan", "DONE",           "N6_PACKAGE_PLAN_DONE"],
      ["plan", "FAILED",         "N6_PACKAGE_PLAN_FAILED"],
      ["commit", "AWAITING_TOKEN", "N6_AWAITING_PACKAGE_COMMIT_TOKEN"],
      ["commit", "TOKEN_ACTIVE",   "N6_PACKAGE_COMMIT_TOKEN_ACTIVE"],
      ["commit", "RUNNING",        "N6_PACKAGE_COMMIT_RUNNING"],
      ["commit", "DONE",           "N6_PACKAGE_COMMIT_DONE"],
      ["commit", "FAILED",         "N6_PACKAGE_COMMIT_FAILED"],
      ["push", "AWAITING_TOKEN", "N6_AWAITING_PACKAGE_PUSH_TOKEN"],
      ["push", "TOKEN_ACTIVE",   "N6_PACKAGE_PUSH_TOKEN_ACTIVE"],
      ["push", "RUNNING",        "N6_PACKAGE_PUSH_RUNNING"],
      ["push", "DONE",           "N6_PACKAGE_PUSH_DONE"],
      ["push", "FAILED",         "N6_PACKAGE_PUSH_FAILED"],
      ["pr", "AWAITING_TOKEN", "N6_AWAITING_DRAFT_PR_TOKEN"],
      ["pr", "TOKEN_ACTIVE",   "N6_DRAFT_PR_TOKEN_ACTIVE"],
      ["pr", "RUNNING",        "N6_DRAFT_PR_RUNNING"],
      ["pr", "DONE",           "N6_DRAFT_PR_DONE"],
      ["pr", "FAILED",         "N6_DRAFT_PR_FAILED"],
    ];
    for (const [rung, exec, expectedState] of expected) {
      const result = deriveN6RungStateName(rung, exec);
      assert.strictEqual(result, expectedState, `rung=${rung} exec=${exec}`);
    }
  });

  it("deriveN6RungStateName output passes assertN6Safe for all combinations", () => {
    const rungs = ["plan", "commit", "push", "pr"] as const;
    const execs = N6_RUNG_EXEC_STATES;
    for (const rung of rungs) {
      for (const exec of execs) {
        const name = deriveN6RungStateName(rung, exec);
        assert.doesNotThrow(
          () => assertN6Safe(name),
          `rung=${rung} exec=${exec} → ${name} must be safe`,
        );
      }
    }
  });

  it("n6RungStateNote returns non-empty strings for all exec states", () => {
    const rungs = ["plan", "commit", "push", "pr"] as const;
    for (const rung of rungs) {
      for (const exec of N6_RUNG_EXEC_STATES) {
        const note = n6RungStateNote(rung, exec);
        assert.ok(note.length > 0, `note must be non-empty for rung=${rung} exec=${exec}`);
      }
    }
  });

  it("FAILED state note mentions token cleared (D4=B contract)", () => {
    const rungs = ["plan", "commit", "push", "pr"] as const;
    for (const rung of rungs) {
      const note = n6RungStateNote(rung, "FAILED");
      assert.ok(
        note.toLowerCase().includes("token cleared") || note.toLowerCase().includes("d4-b"),
        `FAILED note for ${rung} must mention token-cleared (D4=B): "${note}"`,
      );
    }
  });
});
