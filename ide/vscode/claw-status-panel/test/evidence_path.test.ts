import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { classifyEvidencePath, classifyAll } from "../src/evidence_path";

describe("evidence_path — workspace classification", () => {
  it("classifies a path inside the workspace as in-workspace", () => {
    const c = classifyEvidencePath(
      "/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json",
      "/disposable/wks",
    );
    assert.strictEqual(c.location, "in-workspace");
    assert.strictEqual(c.raw, "/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json");
  });

  it("classifies a path outside the workspace as out-of-workspace", () => {
    const c = classifyEvidencePath(
      "/elsewhere/approval-result.json",
      "/disposable/wks",
    );
    assert.strictEqual(c.location, "out-of-workspace");
  });

  it("rejects sibling-prefix collisions (e.g. /disposable/wks-other)", () => {
    const c = classifyEvidencePath(
      "/disposable/wks-other/file",
      "/disposable/wks",
    );
    assert.strictEqual(c.location, "out-of-workspace");
  });

  it("preserves the raw path verbatim (no canonicalization)", () => {
    const c = classifyEvidencePath(
      "/disposable/wks/./.claw/l2b-runs/run-0001/run-manifest.json",
      "/disposable/wks",
    );
    assert.strictEqual(
      c.raw,
      "/disposable/wks/./.claw/l2b-runs/run-0001/run-manifest.json",
    );
  });

  it("missing file → exists=false (existence check is allowed; content read is not)", () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "claw-status-panel-test-"));
    try {
      const c = classifyEvidencePath(path.join(tmp, "absent.json"), tmp);
      assert.strictEqual(c.exists, false);
      assert.strictEqual(c.location, "in-workspace");
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("present file → exists=true (the panel still does NOT read the contents)", () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "claw-status-panel-test-"));
    try {
      const p = path.join(tmp, "present.json");
      fs.writeFileSync(p, "{}");
      const c = classifyEvidencePath(p, tmp);
      assert.strictEqual(c.exists, true);
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });

  it("classifyAll preserves input order and length", () => {
    const result = classifyAll(
      ["/disposable/wks/a", "/disposable/wks/b", "/elsewhere/c"],
      "/disposable/wks",
    );
    assert.strictEqual(result.length, 3);
    assert.strictEqual(result[0].raw, "/disposable/wks/a");
    assert.strictEqual(result[1].raw, "/disposable/wks/b");
    assert.strictEqual(result[2].location, "out-of-workspace");
  });
});
