import * as assert from "assert";
import {
  buildCopyRequest,
  copySingleField,
  ClipboardWriter,
} from "../src/clipboard";

describe("clipboard — single-field, verbatim copy", () => {
  it("places the exact payload on the clipboard, undecorated", async () => {
    const seen: string[] = [];
    const writer: ClipboardWriter = async (p) => {
      seen.push(p);
    };
    const payload =
      "'/media/suki/18TB 2/build-artifacts/stack-code/rust/target/debug/claw' plan apply '/x/ab.json'";
    const req = buildCopyRequest("helper_command", payload);
    await copySingleField(req, writer);
    assert.strictEqual(seen.length, 1);
    assert.strictEqual(seen[0], payload);
  });

  it("does not concatenate multiple fields into one payload", async () => {
    const seen: string[] = [];
    const writer: ClipboardWriter = async (p) => {
      seen.push(p);
    };
    await copySingleField(buildCopyRequest("evidence_path", "/disposable/wks/.claw/x.json"), writer);
    await copySingleField(buildCopyRequest("raw_stdout", "raw output"), writer);
    assert.deepStrictEqual(seen, ["/disposable/wks/.claw/x.json", "raw output"]);
  });

  it("rejects a non-string payload at build time", () => {
    assert.throws(() => buildCopyRequest("helper_command", undefined as never), /string/);
  });
});
