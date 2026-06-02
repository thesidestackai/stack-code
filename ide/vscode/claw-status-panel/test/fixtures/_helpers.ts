import {
  PHASES,
  STOP_CONDITIONS,
  AUDIT_MARKERS,
  SCHEMA_VERSION_LITERAL,
  READ_ONLY_INVARIANT_LITERAL,
  NEXT_OPERATOR_COMMAND_STOP_LITERAL,
  NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL,
} from "../../src/envelope";

export interface EnvelopeOverrides {
  schema_version?: string;
  workspace_root?: string;
  run_id?: string | null;
  step_id?: string | null;
  phase?: string;
  next_operator_command?: string;
  is_approvable?: boolean;
  is_apply_ready?: boolean;
  before_sha256?: string | null;
  after_sha256?: string | null;
  payload_sha256?: string | null;
  live_target_sha256?: string | null;
  stop_condition?: string | null;
  evidence_paths?: string[];
  audit_markers?: string[];
  read_only_invariant?: string;
}

export function baselineSuccessEnvelope(
  overrides: EnvelopeOverrides = {},
): Record<string, unknown> {
  const env: Record<string, unknown> = {
    schema_version: SCHEMA_VERSION_LITERAL,
    workspace_root: "/disposable/wks",
    run_id: "run-0001",
    step_id: "step-001",
    phase: "awaiting_approval",
    next_operator_command:
      "claw plan approve /disposable/wks/.claw/l2b-preview-bundles/run-0001/step-001/preview-bundle.json",
    is_approvable: true,
    is_apply_ready: false,
    before_sha256: "a".repeat(64),
    after_sha256: "b".repeat(64),
    payload_sha256: "b".repeat(64),
    live_target_sha256: "a".repeat(64),
    stop_condition: null,
    evidence_paths: [
      "/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json",
      "/disposable/wks/.claw/l2b-preview-bundles/run-0001/step-001/preview-bundle.json",
    ],
    audit_markers: ["a2-l2d-status-idempotent-emit", "a2-l2d-status-read"],
    read_only_invariant: READ_ONLY_INVARIANT_LITERAL,
  };
  for (const [k, v] of Object.entries(overrides)) {
    env[k] = v as unknown;
  }
  return env;
}

export function baselineRefusalEnvelope(
  overrides: EnvelopeOverrides = {},
): Record<string, unknown> {
  return baselineSuccessEnvelope({
    workspace_root: "/no/such/path",
    run_id: null,
    step_id: null,
    phase: "unknown",
    next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
    is_approvable: false,
    is_apply_ready: false,
    before_sha256: null,
    after_sha256: null,
    payload_sha256: null,
    live_target_sha256: null,
    stop_condition: "workspace-root-invalid",
    evidence_paths: [],
    audit_markers: [
      "a2-l2d-status-read",
      "a2-l2d-status-refused",
      "a2-l2d-status-stop-condition-detected",
    ],
    ...overrides,
  });
}

export function jsonOf(env: Record<string, unknown>): string {
  return JSON.stringify(env, null, 2);
}

export {
  PHASES,
  STOP_CONDITIONS,
  AUDIT_MARKERS,
  SCHEMA_VERSION_LITERAL,
  READ_ONLY_INVARIANT_LITERAL,
  NEXT_OPERATOR_COMMAND_STOP_LITERAL,
  NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL,
};
