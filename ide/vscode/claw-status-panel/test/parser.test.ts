import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { FIXTURES_DIR } from "./_paths";
import { parseStatus } from "../src/parser";
import {
  EXIT_STATUS_REFUSED,
  PHASES,
  STOP_CONDITIONS,
  SCHEMA_VERSION_LITERAL,
  READ_ONLY_INVARIANT_LITERAL,
  NEXT_OPERATOR_COMMAND_STOP_LITERAL,
  NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL,
} from "../src/envelope";
import {
  baselineRefusalEnvelope,
  baselineSuccessEnvelope,
  jsonOf,
} from "./fixtures/_helpers";

describe("parseStatus — valid envelopes", () => {
  it("parses the on-disk baseline_awaiting_approval fixture (not STOP)", () => {
    const raw = fs.readFileSync(
      path.join(FIXTURES_DIR, "baseline_awaiting_approval.json"),
      { encoding: "utf8" },
    );
    const parsed = parseStatus(raw, 0);
    assert.strictEqual(parsed.isStop, false, JSON.stringify(parsed.stopReasons));
    assert.ok(parsed.envelope);
    assert.strictEqual(parsed.envelope!.phase, "awaiting_approval");
    assert.strictEqual(
      parsed.envelope!.read_only_invariant,
      READ_ONLY_INVARIANT_LITERAL,
    );
    assert.deepStrictEqual(parsed.stopReasons, []);
  });

  it("parses the on-disk baseline_refusal fixture (STOP)", () => {
    const raw = fs.readFileSync(
      path.join(FIXTURES_DIR, "baseline_refusal.json"),
      { encoding: "utf8" },
    );
    const parsed = parseStatus(raw, EXIT_STATUS_REFUSED);
    assert.strictEqual(parsed.isStop, true);
    assert.strictEqual(parsed.stopConditionObserved, "workspace-root-invalid");
    assert.ok(
      parsed.stopReasons.some((r) => r.kind === "envelope-refusal"),
      "expected envelope-refusal reason",
    );
  });
});

describe("parseStatus — closed phase enum", () => {
  for (const phase of PHASES) {
    const isStopPhase =
      phase === "rolled_back" || phase === "non_approvable" || phase === "unknown";
    it(`phase=${phase} → ${isStopPhase ? "STOP" : "OK"}`, () => {
      const env = baselineSuccessEnvelope({
        phase,
        next_operator_command: isStopPhase
          ? NEXT_OPERATOR_COMMAND_STOP_LITERAL
          : phase === "no_run_found"
            ? NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL
            : "claw plan apply /disposable/wks/.claw/l2b-checkpoints/run-0001/step-001/apply-bundle.json",
      });
      const parsed = parseStatus(jsonOf(env), 0);
      assert.strictEqual(parsed.isStop, isStopPhase, `phase=${phase}`);
      if (isStopPhase) {
        assert.ok(
          parsed.stopReasons.some((r) => r.kind === "envelope-stop-phase"),
          `expected envelope-stop-phase reason for ${phase}`,
        );
      }
    });
  }
});

describe("parseStatus — closed stop_condition enum (success exit 0)", () => {
  for (const sc of STOP_CONDITIONS) {
    it(`stop_condition=${sc} → STOP`, () => {
      const env = baselineSuccessEnvelope({
        stop_condition: sc,
        next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
        audit_markers: [
          "a2-l2d-status-read",
          "a2-l2d-status-stop-condition-detected",
        ],
      });
      const parsed = parseStatus(jsonOf(env), 0);
      assert.strictEqual(parsed.isStop, true);
      assert.strictEqual(parsed.stopConditionObserved, sc);
      assert.ok(
        parsed.stopReasons.some(
          (r) => r.kind === "envelope-stop-condition" && r.literal === sc,
        ),
        `expected envelope-stop-condition for ${sc}`,
      );
    });
  }
});

describe("parseStatus — refusal envelope per stop_condition", () => {
  for (const sc of ["workspace-root-invalid", "run-manifest-unreadable", "preview-bundle-unreadable"]) {
    it(`refusal stop_condition=${sc} with refused marker → STOP, refusal recognized`, () => {
      const env = baselineRefusalEnvelope({ stop_condition: sc });
      const parsed = parseStatus(jsonOf(env), EXIT_STATUS_REFUSED);
      assert.strictEqual(parsed.isStop, true);
      assert.ok(parsed.stopReasons.some((r) => r.kind === "envelope-refusal"));
      assert.ok(
        !parsed.stopReasons.some((r) => r.kind === "exit-12-missing-refused-marker"),
      );
    });
  }
});

describe("parseStatus — unknown-enum synthetic fixtures", () => {
  it("unknown phase → STOP with verbatim literal", () => {
    const env = baselineSuccessEnvelope({ phase: "totally_made_up_phase" });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.strictEqual(parsed.phaseObserved, "totally_made_up_phase");
    assert.ok(
      parsed.stopReasons.some(
        (r) => r.kind === "unknown-phase" && r.literal === "totally_made_up_phase",
      ),
    );
  });

  it("unknown stop_condition → STOP with verbatim literal", () => {
    const env = baselineSuccessEnvelope({
      stop_condition: "made-up-stop-condition",
      next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
    });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.strictEqual(parsed.stopConditionObserved, "made-up-stop-condition");
    assert.ok(
      parsed.stopReasons.some(
        (r) =>
          r.kind === "unknown-stop-condition" &&
          r.literal === "made-up-stop-condition",
      ),
    );
  });

  it("unknown next_operator_command shape → STOP with verbatim literal", () => {
    const env = baselineSuccessEnvelope({
      next_operator_command: "do something cool",
    });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some(
        (r) =>
          r.kind === "unknown-next-operator-command" &&
          r.literal === "do something cool",
      ),
    );
  });

  it("unknown audit_marker → STOP with verbatim literal", () => {
    const env = baselineSuccessEnvelope({
      audit_markers: ["a2-l2d-status-read", "a2-l2d-mystery-marker"],
    });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.deepStrictEqual(parsed.unknownAuditMarkers, ["a2-l2d-mystery-marker"]);
    assert.ok(
      parsed.stopReasons.some(
        (r) =>
          r.kind === "unknown-audit-marker" &&
          r.literal === "a2-l2d-mystery-marker",
      ),
    );
  });

  it("unknown phase + known stop_condition → both STOPs surfaced independently", () => {
    const env = baselineSuccessEnvelope({
      phase: "still_made_up",
      stop_condition: "payload-sha-mismatch",
      next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
    });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(parsed.stopReasons.some((r) => r.kind === "unknown-phase"));
    assert.ok(
      parsed.stopReasons.some((r) => r.kind === "envelope-stop-condition"),
    );
  });
});

describe("parseStatus — schema-drift fixtures", () => {
  it("missing read_only_invariant → STOP", () => {
    const env = baselineSuccessEnvelope();
    delete (env as Record<string, unknown>)["read_only_invariant"];
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some((r) => r.kind === "missing-read-only-invariant"),
    );
  });

  it("substituted read_only_invariant → STOP with verbatim substitution", () => {
    const env = baselineSuccessEnvelope({
      read_only_invariant: "trust me, no writes",
    });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.strictEqual(parsed.readOnlyInvariantObserved, "trust me, no writes");
    assert.ok(
      parsed.stopReasons.some(
        (r) =>
          r.kind === "substituted-read-only-invariant" &&
          r.literal === "trust me, no writes",
      ),
    );
  });

  it("schema_version mismatch → STOP with verbatim literal", () => {
    const env = baselineSuccessEnvelope({ schema_version: "a2-l2d-status.v2" });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some(
        (r) =>
          r.kind === "schema-version-mismatch" &&
          r.literal === "a2-l2d-status.v2",
      ),
    );
  });

  it("unparseable stdout → STOP, raw bytes preserved verbatim", () => {
    const parsed = parseStatus("not json at all }}}", 0);
    assert.strictEqual(parsed.isStop, true);
    assert.strictEqual(parsed.rawStdout, "not json at all }}}");
    assert.ok(parsed.stopReasons.some((r) => r.kind === "unparseable-stdout"));
  });

  it("missing required field → STOP, missing field name preserved verbatim", () => {
    const env = baselineSuccessEnvelope();
    delete (env as Record<string, unknown>)["evidence_paths"];
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some(
        (r) => r.kind === "missing-required-field" && r.literal === "evidence_paths",
      ),
    );
  });
});

describe("parseStatus — PR43-style preservation fixtures", () => {
  it("non-null stop_condition with empty evidence_paths → STOP raised in its own right", () => {
    const env = baselineSuccessEnvelope({
      stop_condition: "payload-sha-mismatch",
      evidence_paths: [],
      next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
    });
    const parsed = parseStatus(jsonOf(env), 0);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some(
        (r) => r.kind === "non-null-stop-with-empty-evidence-paths",
      ),
    );
    assert.ok(
      parsed.stopReasons.some((r) => r.kind === "envelope-stop-condition"),
    );
  });

  it("exit 12 WITH refused marker → accepted as refusal (no extra missing-marker STOP)", () => {
    const env = baselineRefusalEnvelope();
    const parsed = parseStatus(jsonOf(env), EXIT_STATUS_REFUSED);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(parsed.stopReasons.some((r) => r.kind === "envelope-refusal"));
    assert.ok(
      !parsed.stopReasons.some((r) => r.kind === "exit-12-missing-refused-marker"),
    );
  });

  it("exit 12 WITHOUT refused marker → STOP with observed marker list verbatim", () => {
    const env = baselineRefusalEnvelope({
      audit_markers: ["a2-l2d-status-read", "a2-l2d-status-stop-condition-detected"],
    });
    const parsed = parseStatus(jsonOf(env), EXIT_STATUS_REFUSED);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some(
        (r) => r.kind === "exit-12-missing-refused-marker",
      ),
    );
  });
});

describe("parseStatus — unexpected exit code", () => {
  it("exit 1 with otherwise-valid envelope → STOP", () => {
    const env = baselineSuccessEnvelope();
    const parsed = parseStatus(jsonOf(env), 1);
    assert.strictEqual(parsed.isStop, true);
    assert.ok(
      parsed.stopReasons.some(
        (r) => r.kind === "exit-code-unexpected" && r.literal === "1",
      ),
    );
  });
});

describe("parseStatus — schema literal pinning", () => {
  it("schema_version literal pinned to a2-l2d-status.v1", () => {
    assert.strictEqual(SCHEMA_VERSION_LITERAL, "a2-l2d-status.v1");
  });
  it("read_only_invariant literal pinned", () => {
    assert.strictEqual(
      READ_ONLY_INVARIANT_LITERAL,
      "this command does not mutate state",
    );
  });
});
