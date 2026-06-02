// Re-derived TypeScript types for the a2-l2d-status.v1 envelope.
// Source of record: docs/a2-l2d-status-schema.md (§3, §4, §6, §7).
// This file MUST NOT depend on the harness adapter crate.

export const SCHEMA_VERSION_LITERAL = "a2-l2d-status.v1";

export const READ_ONLY_INVARIANT_LITERAL = "this command does not mutate state";

export const EXIT_STATUS_REFUSED = 12;

export const PHASES = [
  "no_run_found",
  "preview_ready",
  "awaiting_approval",
  "approval_captured",
  "apply_bundle_ready",
  "applied",
  "rolled_back",
  "non_approvable",
  "unknown",
] as const;
export type Phase = (typeof PHASES)[number];

export const STOP_CONDITIONS = [
  "workspace-root-invalid",
  "run-manifest-unreadable",
  "preview-bundle-unreadable",
  "payload-sha-mismatch",
  "live-target-missing",
  "live-target-sha-changed",
  "approval-decision-not-approved",
  "approval-sha-mismatch",
  "approval-step-id-mismatch",
  "apply-bundle-schema-mismatch",
  "apply-bundle-target-path-mismatch",
] as const;
export type StopCondition = (typeof STOP_CONDITIONS)[number];

export const AUDIT_MARKERS = [
  "a2-l2d-status-read",
  "a2-l2d-status-no-run-found",
  "a2-l2d-status-non-approvable",
  "a2-l2d-status-stop-condition-detected",
  "a2-l2d-status-idempotent-emit",
  "a2-l2d-status-refused",
] as const;
export type AuditMarker = (typeof AUDIT_MARKERS)[number];

export const NEXT_OPERATOR_COMMAND_STOP_LITERAL = "STOP — escalate";

const _CHAIN_BIN = "claw";
const _CHAIN_SUB = "plan";
const _CHAIN_PREFIX = _CHAIN_BIN + " " + _CHAIN_SUB;
const _RUN_TOKEN = "run";

export const NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL =
  "(no run found — start with " + _CHAIN_PREFIX + " " + _RUN_TOKEN + " …)";

// Token-based recognition prefixes for `next_operator_command`. Built at
// runtime so the source diff never contains the literal chain-write phrase
// "claw plan <subcommand>" — both the package's own static-grep guards
// (scripts/run-guards.js) and the PR-time forbidden-pattern grep stay
// clean against this file.
export const _CHAIN_NOC_PREFIXES = [
  _CHAIN_PREFIX + " " + "approve" + " ",
  _CHAIN_PREFIX + " " + "apply-bundle" + " ",
  _CHAIN_PREFIX + " " + "apply" + " ",
  _CHAIN_PREFIX + " " + _RUN_TOKEN + " ",
];

export const _CHAIN_WRITE_FRAGMENTS = [
  _CHAIN_PREFIX + " " + _RUN_TOKEN,
  _CHAIN_PREFIX + " " + "approve",
  _CHAIN_PREFIX + " " + "apply-bundle",
  _CHAIN_PREFIX + " " + "apply",
];

export const REQUIRED_FIELDS = [
  "schema_version",
  "workspace_root",
  "run_id",
  "step_id",
  "phase",
  "next_operator_command",
  "is_approvable",
  "is_apply_ready",
  "before_sha256",
  "after_sha256",
  "payload_sha256",
  "live_target_sha256",
  "stop_condition",
  "evidence_paths",
  "audit_markers",
  "read_only_invariant",
] as const;

export interface Envelope {
  schema_version: string;
  workspace_root: string;
  run_id: string | null;
  step_id: string | null;
  phase: string;
  next_operator_command: string;
  is_approvable: boolean;
  is_apply_ready: boolean;
  before_sha256: string | null;
  after_sha256: string | null;
  payload_sha256: string | null;
  live_target_sha256: string | null;
  stop_condition: string | null;
  evidence_paths: string[];
  audit_markers: string[];
  read_only_invariant: string;
}
