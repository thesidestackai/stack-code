import {
  AUDIT_MARKERS,
  EXIT_STATUS_REFUSED,
  Envelope,
  NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL,
  NEXT_OPERATOR_COMMAND_STOP_LITERAL,
  PHASES,
  Phase,
  READ_ONLY_INVARIANT_LITERAL,
  REQUIRED_FIELDS,
  SCHEMA_VERSION_LITERAL,
  STOP_CONDITIONS,
  StopCondition,
  _CHAIN_NOC_PREFIXES,
} from "./envelope";

export type StopReasonKind =
  | "schema-version-mismatch"
  | "missing-required-field"
  | "missing-read-only-invariant"
  | "substituted-read-only-invariant"
  | "unparseable-stdout"
  | "unknown-phase"
  | "unknown-stop-condition"
  | "unknown-next-operator-command"
  | "unknown-audit-marker"
  | "envelope-stop-condition"
  | "envelope-stop-phase"
  | "envelope-refusal"
  | "exit-12-missing-refused-marker"
  | "exit-code-unexpected"
  | "non-null-stop-with-empty-evidence-paths";

export interface StopReason {
  kind: StopReasonKind;
  literal: string;
}

export interface ParsedStatus {
  isStop: boolean;
  envelope: Envelope | null;
  rawStdout: string;
  exitCode: number;
  stopReasons: StopReason[];
  schemaVersionObserved: string | null;
  phaseObserved: string | null;
  stopConditionObserved: string | null;
  nextOperatorCommandObserved: string | null;
  unknownAuditMarkers: string[];
  missingFields: string[];
  readOnlyInvariantObserved: string | null;
}

function isKnownPhase(value: string): value is Phase {
  return (PHASES as readonly string[]).includes(value);
}

function isKnownStopCondition(value: string): value is StopCondition {
  return (STOP_CONDITIONS as readonly string[]).includes(value);
}

function isKnownAuditMarker(value: string): boolean {
  return (AUDIT_MARKERS as readonly string[]).includes(value);
}

function isKnownNextOperatorCommandShape(value: string): boolean {
  if (value === NEXT_OPERATOR_COMMAND_STOP_LITERAL) {
    return true;
  }
  if (value === NEXT_OPERATOR_COMMAND_NO_RUN_LITERAL) {
    return true;
  }
  for (const prefix of _CHAIN_NOC_PREFIXES) {
    if (value.startsWith(prefix)) {
      return true;
    }
  }
  return false;
}

function pushReason(
  reasons: StopReason[],
  kind: StopReasonKind,
  literal: string,
): void {
  reasons.push({ kind, literal });
}

function shallowTypeCheck(
  obj: Record<string, unknown>,
): { ok: boolean; missing: string[]; mistyped: string[] } {
  const missing: string[] = [];
  const mistyped: string[] = [];
  for (const field of REQUIRED_FIELDS) {
    if (!(field in obj)) {
      missing.push(field);
    }
  }
  const stringFields = [
    "schema_version",
    "workspace_root",
    "phase",
    "next_operator_command",
    "read_only_invariant",
  ];
  for (const f of stringFields) {
    if (f in obj && typeof obj[f] !== "string") {
      mistyped.push(f);
    }
  }
  const boolFields = ["is_approvable", "is_apply_ready"];
  for (const f of boolFields) {
    if (f in obj && typeof obj[f] !== "boolean") {
      mistyped.push(f);
    }
  }
  const arrayFields = ["evidence_paths", "audit_markers"];
  for (const f of arrayFields) {
    if (f in obj && !Array.isArray(obj[f])) {
      mistyped.push(f);
    }
  }
  return { ok: missing.length === 0 && mistyped.length === 0, missing, mistyped };
}

export function parseStatus(rawStdout: string, exitCode: number): ParsedStatus {
  const reasons: StopReason[] = [];
  const result: ParsedStatus = {
    isStop: false,
    envelope: null,
    rawStdout,
    exitCode,
    stopReasons: reasons,
    schemaVersionObserved: null,
    phaseObserved: null,
    stopConditionObserved: null,
    nextOperatorCommandObserved: null,
    unknownAuditMarkers: [],
    missingFields: [],
    readOnlyInvariantObserved: null,
  };

  let parsed: unknown;
  try {
    parsed = JSON.parse(rawStdout);
  } catch {
    pushReason(reasons, "unparseable-stdout", rawStdout.slice(0, 200));
    result.isStop = true;
    if (exitCode !== 0 && exitCode !== EXIT_STATUS_REFUSED) {
      pushReason(reasons, "exit-code-unexpected", String(exitCode));
    }
    return result;
  }

  if (parsed === null || typeof parsed !== "object" || Array.isArray(parsed)) {
    pushReason(reasons, "unparseable-stdout", rawStdout.slice(0, 200));
    result.isStop = true;
    return result;
  }

  const obj = parsed as Record<string, unknown>;
  const typeCheck = shallowTypeCheck(obj);
  if (!typeCheck.ok) {
    for (const m of typeCheck.missing) {
      pushReason(reasons, "missing-required-field", m);
    }
    for (const m of typeCheck.mistyped) {
      pushReason(reasons, "missing-required-field", m);
    }
    result.missingFields = [...typeCheck.missing, ...typeCheck.mistyped];
    result.isStop = true;
  }

  const sv = obj["schema_version"];
  if (typeof sv === "string") {
    result.schemaVersionObserved = sv;
  }
  const ph = obj["phase"];
  if (typeof ph === "string") {
    result.phaseObserved = ph;
  }
  const sc = obj["stop_condition"];
  if (typeof sc === "string") {
    result.stopConditionObserved = sc;
  }
  const noc = obj["next_operator_command"];
  if (typeof noc === "string") {
    result.nextOperatorCommandObserved = noc;
  }
  const roi = obj["read_only_invariant"];
  if (typeof roi === "string") {
    result.readOnlyInvariantObserved = roi;
  }

  if (result.schemaVersionObserved !== null) {
    if (result.schemaVersionObserved !== SCHEMA_VERSION_LITERAL) {
      pushReason(reasons, "schema-version-mismatch", result.schemaVersionObserved);
      result.isStop = true;
    }
  }

  if (!("read_only_invariant" in obj)) {
    pushReason(reasons, "missing-read-only-invariant", "(field absent)");
    result.isStop = true;
  } else if (result.readOnlyInvariantObserved === null) {
    pushReason(reasons, "missing-read-only-invariant", "(non-string)");
    result.isStop = true;
  } else if (result.readOnlyInvariantObserved !== READ_ONLY_INVARIANT_LITERAL) {
    pushReason(
      reasons,
      "substituted-read-only-invariant",
      result.readOnlyInvariantObserved,
    );
    result.isStop = true;
  }

  if (result.phaseObserved !== null && !isKnownPhase(result.phaseObserved)) {
    pushReason(reasons, "unknown-phase", result.phaseObserved);
    result.isStop = true;
  } else if (
    result.phaseObserved === "non_approvable" ||
    result.phaseObserved === "rolled_back" ||
    result.phaseObserved === "unknown"
  ) {
    pushReason(reasons, "envelope-stop-phase", result.phaseObserved);
    result.isStop = true;
  }

  if (result.stopConditionObserved !== null) {
    if (!isKnownStopCondition(result.stopConditionObserved)) {
      pushReason(reasons, "unknown-stop-condition", result.stopConditionObserved);
      result.isStop = true;
    } else {
      pushReason(reasons, "envelope-stop-condition", result.stopConditionObserved);
      result.isStop = true;
    }
    const ep = obj["evidence_paths"];
    if (Array.isArray(ep) && ep.length === 0) {
      pushReason(reasons, "non-null-stop-with-empty-evidence-paths", "[]");
      result.isStop = true;
    }
  }

  if (
    result.nextOperatorCommandObserved !== null &&
    !isKnownNextOperatorCommandShape(result.nextOperatorCommandObserved)
  ) {
    pushReason(
      reasons,
      "unknown-next-operator-command",
      result.nextOperatorCommandObserved,
    );
    result.isStop = true;
  }

  const am = obj["audit_markers"];
  if (Array.isArray(am)) {
    const unknown = (am as unknown[])
      .filter((m): m is string => typeof m === "string")
      .filter((m) => !isKnownAuditMarker(m));
    if (unknown.length > 0) {
      for (const u of unknown) {
        pushReason(reasons, "unknown-audit-marker", u);
      }
      result.unknownAuditMarkers = unknown;
      result.isStop = true;
    }
    if (exitCode === EXIT_STATUS_REFUSED) {
      const hasRefused = (am as unknown[]).some(
        (m) => m === "a2-l2d-status-refused",
      );
      if (hasRefused) {
        pushReason(reasons, "envelope-refusal", String(EXIT_STATUS_REFUSED));
        result.isStop = true;
      } else {
        pushReason(
          reasons,
          "exit-12-missing-refused-marker",
          JSON.stringify(am),
        );
        result.isStop = true;
      }
    }
  }

  if (exitCode !== 0 && exitCode !== EXIT_STATUS_REFUSED) {
    pushReason(reasons, "exit-code-unexpected", String(exitCode));
    result.isStop = true;
  }

  if (typeCheck.ok) {
    result.envelope = obj as unknown as Envelope;
  }

  return result;
}
