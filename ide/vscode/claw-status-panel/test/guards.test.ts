import * as assert from "assert";
import { spawnSync } from "child_process";
import { PKG_ROOT, GUARDS_SCRIPT } from "./_paths";

describe("guards — static-grep guards over src/", () => {
  it("scripts/run-guards.js exits 0 on the shipped src/ tree", () => {
    const result = spawnSync(process.execPath, [GUARDS_SCRIPT], {
      cwd: PKG_ROOT,
      encoding: "utf8",
    });
    if (result.status !== 0) {
      throw new Error(
        `guards FAILED:\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}`,
      );
    }
    assert.strictEqual(result.status, 0);
    assert.ok(result.stdout.includes("claw-status-panel guards PASS"));
  });
});
