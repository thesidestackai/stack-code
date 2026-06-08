// Evidence timeline (pure).
//
// A read-only, session-local record of the safe gestures taken in a panel
// session: workspace detection, field sets, read-only helper subcommands (with
// exit codes), and discovery results. It records that print steps were PRINTED,
// not run. It writes no file — the extension renders it in the panel and folds
// it into the unsaved evidence summary document. Ordering + exit codes are the
// load-bearing evidence; no wall-clock timestamp is required (and none is
// fabricated).

export type TimelineKind =
  | "workspace"
  | "field-set"
  | "discovery"
  | "helper"
  | "status"
  | "note";

export interface TimelineEvent {
  kind: TimelineKind;
  detail: string;
  // Present for helper events: the read-only subcommand's exit code.
  exitCode?: number;
}

export function event(kind: TimelineKind, detail: string, exitCode?: number): TimelineEvent {
  const e: TimelineEvent = { kind, detail };
  if (typeof exitCode === "number") {
    e.exitCode = exitCode;
  }
  return e;
}

// Format the timeline into ordered, human-readable lines. Index-prefixed so the
// sequence is unambiguous without timestamps.
export function formatTimeline(events: ReadonlyArray<TimelineEvent>): string[] {
  if (!events || events.length === 0) {
    return ["(no safe actions recorded yet)"];
  }
  return events.map((e, i) => {
    const exit = typeof e.exitCode === "number" ? ` (exit ${e.exitCode})` : "";
    return `[${i}] ${e.kind}: ${e.detail}${exit}`;
  });
}

// Append-with-cap: keep the timeline bounded so a long session cannot grow it
// without limit. Returns a new array (does not mutate the input).
const MAX_EVENTS = 200;

export function append(
  events: ReadonlyArray<TimelineEvent>,
  next: TimelineEvent,
): TimelineEvent[] {
  const out = [...events, next];
  if (out.length > MAX_EVENTS) {
    return out.slice(out.length - MAX_EVENTS);
  }
  return out;
}
