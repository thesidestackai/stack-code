import * as assert from "assert";
import {
  buildSpawnRequest,
  runClawStatus,
  SpawnImpl,
  SpawnResult,
  SubprocessRefusal,
} from "../src/subprocess";

describe("subprocess — argv shape", () => {
  it("builds `claw plan status <workspace>` exactly", () => {
    const req = buildSpawnRequest({ binary: "claw", workspace: "/disposable/wks" });
    assert.strictEqual(req.binary, "claw");
    assert.deepStrictEqual(req.args, ["plan", "status", "/disposable/wks"]);
  });

  it("builds `claw plan status <workspace> <approval-result>` when supplied", () => {
    const req = buildSpawnRequest({
      binary: "claw",
      workspace: "/disposable/wks",
      approvalResultPath: "/elsewhere/approval-result.json",
    });
    assert.deepStrictEqual(req.args, [
      "plan",
      "status",
      "/disposable/wks",
      "/elsewhere/approval-result.json",
    ]);
  });

  it("refuses an empty binary", () => {
    assert.throws(
      () => buildSpawnRequest({ binary: "", workspace: "/disposable/wks" }),
      SubprocessRefusal,
    );
  });

  it("refuses an empty workspace", () => {
    assert.throws(
      () => buildSpawnRequest({ binary: "claw", workspace: "" }),
      SubprocessRefusal,
    );
  });

  it("refuses workspace containing `claw plan run`", () => {
    assert.throws(
      () =>
        buildSpawnRequest({
          binary: "claw",
          workspace: "/disposable/wks; claw plan run x",
        }),
      SubprocessRefusal,
    );
  });

  it("refuses approval-result containing `claw plan approve`", () => {
    assert.throws(
      () =>
        buildSpawnRequest({
          binary: "claw",
          workspace: "/disposable/wks",
          approvalResultPath: "/x/claw plan approve",
        }),
      SubprocessRefusal,
    );
  });

  it("refuses binary containing `claw plan apply-bundle`", () => {
    assert.throws(
      () =>
        buildSpawnRequest({
          binary: "/usr/local/bin/claw plan apply-bundle injector",
          workspace: "/disposable/wks",
        }),
      SubprocessRefusal,
    );
  });

  it("refuses approval-result containing `claw plan apply`", () => {
    assert.throws(
      () =>
        buildSpawnRequest({
          binary: "claw",
          workspace: "/disposable/wks",
          approvalResultPath: "/x/claw plan apply payload",
        }),
      SubprocessRefusal,
    );
  });

  it("refuses workspace starting with `-` (flag shape)", () => {
    assert.throws(
      () => buildSpawnRequest({ binary: "claw", workspace: "--apply" }),
      SubprocessRefusal,
    );
  });

  it("refuses approval-result starting with `-` (flag shape)", () => {
    assert.throws(
      () =>
        buildSpawnRequest({
          binary: "claw",
          workspace: "/disposable/wks",
          approvalResultPath: "--yes",
        }),
      SubprocessRefusal,
    );
  });

  it("never adds flag-shaped args of its own", () => {
    const req = buildSpawnRequest({ binary: "claw", workspace: "/disposable/wks" });
    for (const a of req.args) {
      assert.ok(!a.startsWith("-"), `arg ${a} must not start with '-'`);
    }
  });

  it("argv length is at most 4 (`plan`, `status`, workspace, [approval-result])", () => {
    const r1 = buildSpawnRequest({ binary: "claw", workspace: "/x" });
    const r2 = buildSpawnRequest({
      binary: "claw",
      workspace: "/x",
      approvalResultPath: "/y",
    });
    assert.strictEqual(r1.args.length, 3);
    assert.strictEqual(r2.args.length, 4);
  });
});

describe("subprocess — spawn injection", () => {
  it("runClawStatus invokes spawn exactly once with the bounded argv", async () => {
    const seen: Array<{ binary: string; args: string[] }> = [];
    const mock: SpawnImpl = async (req) => {
      seen.push({ binary: req.binary, args: [...req.args] });
      const result: SpawnResult = { exitCode: 0, stdout: "{}", stderr: "" };
      return result;
    };
    await runClawStatus(
      { binary: "claw", workspace: "/disposable/wks" },
      mock,
    );
    assert.strictEqual(seen.length, 1);
    assert.strictEqual(seen[0].binary, "claw");
    assert.deepStrictEqual(seen[0].args, ["plan", "status", "/disposable/wks"]);
  });

  it("a chain-write subcommand in args is refused before any spawn", async () => {
    let spawnCalls = 0;
    const mock: SpawnImpl = async () => {
      spawnCalls++;
      return { exitCode: 0, stdout: "", stderr: "" };
    };
    await assert.rejects(
      runClawStatus(
        {
          binary: "claw",
          workspace: "/disposable/wks",
          approvalResultPath: "claw plan approve injected",
        },
        mock,
      ),
      SubprocessRefusal,
    );
    assert.strictEqual(spawnCalls, 0);
  });
});
