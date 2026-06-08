import * as assert from "assert";
import {
  ledgerEvent,
  formatLedger,
  appendLedger,
  AgentLedgerEvent,
} from "../src/agentEvidence";

describe("agentEvidence — ledger event shape", () => {
  it("builds an event with defaults (printedNotRun false; no fabricated timestamp)", () => {
    const e = ledgerEvent({ kind: "session", tier: 1, action: "open", status: "info", summary: "s" });
    assert.strictEqual(e.printedNotRun, false);
    assert.strictEqual(e.timestamp, undefined);
    assert.strictEqual(e.details, undefined);
  });

  it("carries details, printedNotRun, and a caller-supplied timestamp", () => {
    const e = ledgerEvent({
      kind: "command",
      tier: 1,
      action: "print-preview",
      status: "ok",
      summary: "command printed",
      details: "claw plan run ...",
      printedNotRun: true,
      timestamp: "T0",
    });
    assert.strictEqual(e.printedNotRun, true);
    assert.strictEqual(e.timestamp, "T0");
    assert.strictEqual(e.details, "claw plan run ...");
  });
});

describe("agentEvidence — formatting", () => {
  it("formats an empty ledger with a placeholder", () => {
    const lines = formatLedger([]);
    assert.strictEqual(lines.length, 1);
    assert.ok(/no agent-cockpit gestures/i.test(lines[0]));
  });

  it("marks printed-not-run steps and index-prefixes the sequence", () => {
    const events: AgentLedgerEvent[] = [
      ledgerEvent({ kind: "session", tier: 1, action: "open", status: "info", summary: "ready" }),
      ledgerEvent({
        kind: "command",
        tier: 1,
        action: "print-apply",
        status: "ok",
        summary: "printed",
        printedNotRun: true,
      }),
    ];
    const lines = formatLedger(events);
    assert.strictEqual(lines.length, 2);
    assert.ok(lines[0].startsWith("[0] Tier 1 session/info: open"));
    assert.ok(lines[1].includes("[printed-not-run]"));
  });
});

describe("agentEvidence — append is bounded and non-mutating", () => {
  it("appends without mutating the source array", () => {
    const a: AgentLedgerEvent[] = [ledgerEvent({ kind: "note", tier: 0, action: "n", status: "info", summary: "one" })];
    const b = appendLedger(a, ledgerEvent({ kind: "note", tier: 0, action: "n", status: "info", summary: "two" }));
    assert.strictEqual(a.length, 1);
    assert.strictEqual(b.length, 2);
  });

  it("caps the ledger length and retains the most recent event", () => {
    let events: AgentLedgerEvent[] = [];
    for (let i = 0; i < 250; i++) {
      events = appendLedger(events, ledgerEvent({ kind: "note", tier: 0, action: "n", status: "info", summary: "n" + i }));
    }
    assert.ok(events.length <= 200);
    assert.strictEqual(events[events.length - 1].summary, "n249");
  });
});
