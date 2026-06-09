// Mutation evidence ledger (pure) — Tier 3 Foundation v0.
//
// A renderable, session-local record of Tier 3 mutation-lane gestures
// (checkpoint / mutation / validation / decision), per
// docs/a2-tier3-disposable-worktree-mutation-scope.md §§11,12. This module is
// PURE: it builds and formats ledger events. It writes NO file, performs NO IO,
// and is NOT persisted. It records decisions and checkpoints; it does not make a
// mutation. Print/checkpoint steps are marked printed-not-run.

export type MutationKind =
  | "checkpoint"
  | "mutation"
  | "validation"
  | "decision"
  | "note";

export type MutationStatus = "allowed" | "denied" | "ok" | "blocked" | "info";

export interface MutationLedgerEvent {
  // Caller-supplied marker; NOT fabricated (no wall-clock is read). Optional.
  timestamp?: string | null;
  kind: MutationKind;
  // The tier in effect (3 for a granted Tier 3 lane; read-only otherwise).
  tier: number;
  action: string;
  status: MutationStatus;
  summary: string;
  details?: string;
  // True when this records a step that was PRINTED/planned, not executed.
  printedNotRun: boolean;
}

export interface NewMutationEventInput {
  kind: MutationKind;
  tier: number;
  action: string;
  status: MutationStatus;
  summary: string;
  details?: string;
  printedNotRun?: boolean;
  timestamp?: string | null;
}

export function mutationEvent(input: NewMutationEventInput): MutationLedgerEvent {
  const e: MutationLedgerEvent = {
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

// Format the ledger into ordered, render-ready lines. Index-prefixed (no
// fabricated wall-clock). Print/checkpoint-only steps marked "[printed-not-run]".
export function formatMutationLedger(events: ReadonlyArray<MutationLedgerEvent>): string[] {
  if (!events || events.length === 0) {
    return ["(no Tier 3 mutation-lane gestures recorded yet)"];
  }
  return events.map((e, i) => {
    const printed = e.printedNotRun ? " [printed-not-run]" : "";
    const detail = e.details ? " — " + e.details : "";
    return `[${i}] Tier ${e.tier} ${e.kind}/${e.status}: ${e.action}${printed} — ${e.summary}${detail}`;
  });
}

const MAX_EVENTS = 200;

export function appendMutationEvent(
  events: ReadonlyArray<MutationLedgerEvent>,
  next: MutationLedgerEvent,
): MutationLedgerEvent[] {
  const out = [...events, next];
  if (out.length > MAX_EVENTS) {
    return out.slice(out.length - MAX_EVENTS);
  }
  return out;
}
