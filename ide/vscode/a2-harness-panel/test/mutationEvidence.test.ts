import * as assert from "assert";
import {
  mutationEvent,
  formatMutationLedger,
  appendMutationEvent,
  MutationLedgerEvent,
} from "../src/mutationEvidence";

describe("mutationEvidence — event shape", () => {
  it("defaults printedNotRun false and fabricates no timestamp", () => {
    const e = mutationEvent({ kind: "checkpoint", tier: 3, action: "base", status: "info", summary: "s" });
    assert.strictEqual(e.printedNotRun, false);
    assert.strictEqual(e.timestamp, undefined);
  });

  it("carries details, printedNotRun, and caller timestamp", () => {
    const e = mutationEvent({
      kind: "decision",
      tier: 3,
      action: "write src/x.ts",
      status: "denied",
      summary: "outside declared set",
      details: "rejected",
      printedNotRun: true,
      timestamp: "T0",
    });
    assert.strictEqual(e.printedNotRun, true);
    assert.strictEqual(e.timestamp, "T0");
    assert.strictEqual(e.details, "rejected");
  });
});

describe("mutationEvidence — formatting", () => {
  it("formats an empty ledger with a placeholder", () => {
    const lines = formatMutationLedger([]);
    assert.strictEqual(lines.length, 1);
    assert.ok(/no Tier 3 mutation-lane gestures/i.test(lines[0]));
  });

  it("marks printed-not-run and index-prefixes the sequence", () => {
    const events: MutationLedgerEvent[] = [
      mutationEvent({ kind: "checkpoint", tier: 3, action: "base", status: "info", summary: "recorded", printedNotRun: true }),
      mutationEvent({ kind: "decision", tier: 3, action: "write", status: "denied", summary: "out of scope" }),
    ];
    const lines = formatMutationLedger(events);
    assert.ok(lines[0].startsWith("[0] Tier 3 checkpoint/info: base [printed-not-run]"));
    assert.ok(lines[1].includes("decision/denied"));
  });
});

describe("mutationEvidence — append bounded + non-mutating", () => {
  it("appends without mutating the source", () => {
    const a: MutationLedgerEvent[] = [mutationEvent({ kind: "note", tier: 3, action: "n", status: "info", summary: "one" })];
    const b = appendMutationEvent(a, mutationEvent({ kind: "note", tier: 3, action: "n", status: "info", summary: "two" }));
    assert.strictEqual(a.length, 1);
    assert.strictEqual(b.length, 2);
  });

  it("caps the ledger and retains the most recent", () => {
    let events: MutationLedgerEvent[] = [];
    for (let i = 0; i < 250; i++) {
      events = appendMutationEvent(events, mutationEvent({ kind: "note", tier: 3, action: "n", status: "info", summary: "n" + i }));
    }
    assert.ok(events.length <= 200);
    assert.strictEqual(events[events.length - 1].summary, "n249");
  });
});
