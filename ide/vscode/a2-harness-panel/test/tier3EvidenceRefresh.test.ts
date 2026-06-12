import * as assert from "assert";
import { refreshOutcomeFromResult } from "../src/tier3EvidenceRefresh";

const SNAP =
  '{"schema_version":"a2-tier3-evidence-snapshot.v0","tier3_status":"ok"}';

describe("tier3EvidenceRefresh — outcome mapping (fail-closed)", () => {
  it("stores trimmed stdout as snapshot text on exit 0 with non-empty output", () => {
    const out = refreshOutcomeFromResult({ exitCode: 0, stdout: "  " + SNAP + "\n", stderr: "" });
    assert.strictEqual(out.snapshotText, SNAP);
    assert.strictEqual(out.notice, null);
  });

  it("fails closed (null snapshot + notice) on a non-zero exit code", () => {
    const out = refreshOutcomeFromResult({
      exitCode: 3,
      stdout: "",
      stderr: "a2-ide-harness.sh: ERROR: collector not found",
    });
    assert.strictEqual(out.snapshotText, null);
    assert.ok(out.notice && /refresh/i.test(out.notice), "notice should mention refresh");
    assert.ok(/collector not found/.test(out.notice as string), "notice should surface stderr cause");
  });

  it("fails closed when exit 0 but stdout is empty/whitespace", () => {
    const out = refreshOutcomeFromResult({ exitCode: 0, stdout: "   \n", stderr: "" });
    assert.strictEqual(out.snapshotText, null);
    assert.ok(out.notice && /refresh/i.test(out.notice));
  });

  it("never returns control/executor text in the notice", () => {
    const out = refreshOutcomeFromResult({ exitCode: 7, stdout: "", stderr: "boom" });
    assert.ok(!/claw plan (run|approve|apply-bundle|apply)/.test(out.notice as string));
  });
});
