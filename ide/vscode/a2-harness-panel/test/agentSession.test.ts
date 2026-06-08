import * as assert from "assert";
import {
  newAgentSession,
  hasNoSecretFields,
  summarizeSession,
  FORBIDDEN_SESSION_KEYS,
} from "../src/agentSession";

describe("agentSession — model", () => {
  it("requires a non-empty sessionId", () => {
    assert.throws(() => newAgentSession({ sessionId: "" }));
    assert.throws(() => newAgentSession({ sessionId: "   " }));
  });

  it("defaults to read-only tier and observing status, no persistence inputs", () => {
    const s = newAgentSession({ sessionId: "abc" });
    assert.strictEqual(s.allowedTier, 1);
    assert.strictEqual(s.status, "observing");
    assert.strictEqual(s.objective, null);
    assert.strictEqual(s.createdAt, null); // not fabricated
    assert.deepStrictEqual(s.touchedSurfaces, []);
    assert.deepStrictEqual(s.evidenceLedger, []);
  });

  it("carries through provided fields and copies arrays defensively", () => {
    const surfaces = ["a.ts"];
    const s = newAgentSession({
      sessionId: "abc",
      objective: "do the thing",
      workspaceRoot: "/ws",
      touchedSurfaces: surfaces,
      allowedTier: 2,
    });
    assert.strictEqual(s.objective, "do the thing");
    assert.strictEqual(s.workspaceRoot, "/ws");
    assert.strictEqual(s.allowedTier, 2);
    surfaces.push("b.ts");
    assert.deepStrictEqual(s.touchedSurfaces, ["a.ts"]); // not aliased
  });
});

describe("agentSession — no secrets", () => {
  it("a fresh session contains no secret-like field keys", () => {
    const s = newAgentSession({ sessionId: "abc", objective: "x" });
    assert.strictEqual(hasNoSecretFields(s), true);
    const keys = Object.keys(s).map((k) => k.toLowerCase());
    for (const forbidden of FORBIDDEN_SESSION_KEYS) {
      assert.ok(!keys.includes(forbidden.toLowerCase()), `session must not have key ${forbidden}`);
    }
  });

  it("hasNoSecretFields detects an injected secret-like key", () => {
    const tainted = { sessionId: "abc", token: "leak" };
    assert.strictEqual(hasNoSecretFields(tainted), false);
  });
});

describe("agentSession — summary", () => {
  it("renders human-readable summary lines without secrets", () => {
    const s = newAgentSession({ sessionId: "abc", objective: "x", allowedTier: 1 });
    const lines = summarizeSession(s);
    assert.ok(lines.some((l) => l.startsWith("session: abc")));
    assert.ok(lines.some((l) => l.includes("allowed tier: Tier 1")));
    assert.ok(lines.some((l) => l.includes("status: observing")));
    assert.ok(!lines.join("\n").toLowerCase().includes("token"));
  });
});
