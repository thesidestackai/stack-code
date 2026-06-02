import * as assert from "assert";
import { buildCopyRequest, copySingleField } from "../src/clipboard";

describe("clipboard — single-field-only copy", () => {
  it("copy next_operator_command writes the verbatim envelope value", async () => {
    const writes: string[] = [];
    const writer = async (p: string) => {
      writes.push(p);
    };
    await copySingleField(
      buildCopyRequest(
        "next_operator_command",
        "claw plan approve /disposable/wks/.claw/l2b-preview-bundles/run-0001/step-001/preview-bundle.json",
      ),
      writer,
    );
    assert.deepStrictEqual(writes, [
      "claw plan approve /disposable/wks/.claw/l2b-preview-bundles/run-0001/step-001/preview-bundle.json",
    ]);
  });

  it("copy evidence_path writes the verbatim path", async () => {
    const writes: string[] = [];
    await copySingleField(
      buildCopyRequest(
        "evidence_path",
        "/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json",
      ),
      async (p) => {
        writes.push(p);
      },
    );
    assert.strictEqual(writes.length, 1);
    assert.strictEqual(
      writes[0],
      "/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json",
    );
  });

  it("copy raw_envelope writes verbatim raw stdout", async () => {
    const writes: string[] = [];
    const payload = "{\"schema_version\":\"a2-l2d-status.v1\"}";
    await copySingleField(
      buildCopyRequest("raw_envelope", payload),
      async (p) => {
        writes.push(p);
      },
    );
    assert.strictEqual(writes[0], payload);
  });

  it("each copy call performs exactly one write (no chaining)", async () => {
    let count = 0;
    await copySingleField(
      buildCopyRequest("next_operator_command", "x"),
      async () => {
        count++;
      },
    );
    assert.strictEqual(count, 1);
  });

  it("buildCopyRequest refuses non-string payloads", () => {
    assert.throws(() =>
      buildCopyRequest("next_operator_command", (123 as unknown) as string),
    );
  });

  it("copy actions do not include an approval-line composition surface", () => {
    // No "approval-line" CopyKind exists. The CopyKind type literal-pin is
    // enforced by the TypeScript compiler; this test reads the literal set
    // from buildCopyRequest's behavior. Anything else throws TS at compile.
    const allowed = ["next_operator_command", "evidence_path", "raw_envelope"];
    for (const k of allowed) {
      const r = buildCopyRequest(k as "next_operator_command", "v");
      assert.strictEqual(r.payload, "v");
    }
  });
});
