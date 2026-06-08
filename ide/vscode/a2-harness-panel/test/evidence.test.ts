import * as assert from "assert";
import { event, formatTimeline, append, TimelineEvent } from "../src/evidence";

describe("evidence — timeline formatting", () => {
  it("formats an ordered, index-prefixed timeline", () => {
    const events: TimelineEvent[] = [
      event("workspace", "detected: /disposable/wks"),
      event("field-set", "plan = /disposable/wks/plan.yaml"),
      event("helper", "validate-input", 0),
      event("helper", "print-preview — command printed (not run)", 0),
    ];
    const lines = formatTimeline(events);
    assert.strictEqual(lines.length, 4);
    assert.ok(lines[0].startsWith("[0] workspace: detected"));
    assert.ok(lines[2].includes("validate-input"));
    assert.ok(lines[2].includes("(exit 0)"));
    assert.ok(lines[3].includes("printed (not run)"));
  });

  it("shows an empty-state line when there are no events", () => {
    const lines = formatTimeline([]);
    assert.strictEqual(lines.length, 1);
    assert.ok(/no safe actions recorded/i.test(lines[0]));
  });

  it("omits the exit suffix for non-helper events", () => {
    const lines = formatTimeline([event("note", "a note")]);
    assert.ok(!/exit/.test(lines[0]));
  });
});

describe("evidence — append is bounded and non-mutating", () => {
  it("appends without mutating the source array", () => {
    const a: TimelineEvent[] = [event("note", "one")];
    const b = append(a, event("note", "two"));
    assert.strictEqual(a.length, 1);
    assert.strictEqual(b.length, 2);
    assert.strictEqual(b[1].detail, "two");
  });

  it("caps the timeline length", () => {
    let events: TimelineEvent[] = [];
    for (let i = 0; i < 250; i++) {
      events = append(events, event("note", "n" + i));
    }
    assert.ok(events.length <= 200);
    // The most recent event is retained.
    assert.strictEqual(events[events.length - 1].detail, "n249");
  });
});
