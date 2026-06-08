// Agent evidence ledger (pure) — A2 Local Coding Agent Foundation v0.
//
// A renderable, session-local record of every agent-cockpit gesture/command
// (docs/a2-local-coding-agent-foundation-scope.md §8). This module is PURE: it
// builds and formats ledger events for the webview. It writes NO file, performs
// NO IO, and is NOT persisted. Print-only steps are explicitly recorded as
// printed-not-run — the panel never claims a printed command was executed.
//
// This is distinct from the existing read-only `evidence.ts` timeline (the safe
// helper-action timeline). The agent evidence ledger is the foundation control
// plane's record: it carries the permission tier and the allowed/denied
// decision for each agent-cockpit gesture.

export type LedgerKind =
  | "session"
  | "readiness"
  | "tier"
  | "command"
  | "decision"
  | "note";

export type LedgerStatus = "allowed" | "denied" | "ok" | "blocked" | "info";

export interface AgentLedgerEvent {
  // Caller-supplied marker; NOT fabricated (no wall-clock is read). Optional.
  timestamp?: string | null;
  kind: LedgerKind;
  // The permission tier in effect for this event (0-5).
  tier: number;
  action: string;
  status: LedgerStatus;
  summary: string;
  details?: string;
  // True when this records a command that was PRINTED, not executed.
  printedNotRun: boolean;
}

export interface NewLedgerEventInput {
  kind: LedgerKind;
  tier: number;
  action: string;
  status: LedgerStatus;
  summary: string;
  details?: string;
  printedNotRun?: boolean;
  timestamp?: string | null;
}

export function ledgerEvent(input: NewLedgerEventInput): AgentLedgerEvent {
  const e: AgentLedgerEvent = {
    kind: input.kind,
    tier: input.tier,
    action: input.action,
    status: input.status,
    summary: input.summary,
    printedNotRun: input.printedNotRun === true,
  };
  if (typeof input.details === "string") {
    e.details = input.details;
  }
  if (typeof input.timestamp === "string") {
    e.timestamp = input.timestamp;
  }
  return e;
}

// Format the ledger into ordered, render-ready lines. Index-prefixed so the
// sequence is unambiguous without a fabricated wall-clock. Print-only steps are
// marked "[printed-not-run]".
export function formatLedger(events: ReadonlyArray<AgentLedgerEvent>): string[] {
  if (!events || events.length === 0) {
    return ["(no agent-cockpit gestures recorded yet)"];
  }
  return events.map((e, i) => {
    const printed = e.printedNotRun ? " [printed-not-run]" : "";
    const detail = e.details ? " — " + e.details : "";
    return `[${i}] Tier ${e.tier} ${e.kind}/${e.status}: ${e.action}${printed} — ${e.summary}${detail}`;
  });
}

// Append-with-cap: keep the ledger bounded (mirrors evidence.ts). Returns a new
// array; does not mutate the input.
const MAX_LEDGER_EVENTS = 200;

export function appendLedger(
  events: ReadonlyArray<AgentLedgerEvent>,
  next: AgentLedgerEvent,
): AgentLedgerEvent[] {
  const out = [...events, next];
  if (out.length > MAX_LEDGER_EVENTS) {
    return out.slice(out.length - MAX_LEDGER_EVENTS);
  }
  return out;
}
