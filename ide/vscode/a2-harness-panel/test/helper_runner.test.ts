import * as assert from "assert";
import {
  buildHelperRequest,
  runHelper,
  HelperRunnerRefusal,
  ALLOWED_SUBCOMMANDS,
  ALLOWED_FLAGS,
  HELPER_BASENAME,
  SpawnImpl,
  SpawnResult,
} from "../src/helperRunner";

const HELPER = "/disposable/wks/scripts/a2-ide-harness.sh";

describe("helperRunner — argv shape", () => {
  it("builds `<helper> audit-workspace --workspace <ws>` exactly", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "audit-workspace",
      options: { workspace: "/disposable/wks" },
    });
    assert.strictEqual(req.binary, HELPER);
    assert.deepStrictEqual(req.args, ["audit-workspace", "--workspace", "/disposable/wks"]);
  });

  it("builds verify-final with all three flags", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "verify-final",
      options: { workspace: "/disposable/wks", target: "/disposable/wks/out.txt", "after-sha": "abc123" },
    });
    assert.strictEqual(req.args[0], "verify-final");
    assert.ok(req.args.includes("--workspace"));
    assert.ok(req.args.includes("--target"));
    assert.ok(req.args.includes("--after-sha"));
    assert.ok(req.args.includes("abc123"));
  });

  it("help takes no flags", () => {
    const req = buildHelperRequest({ helperPath: HELPER, subcommand: "help" });
    assert.deepStrictEqual(req.args, ["help"]);
  });
});

describe("helperRunner — subcommand allowlist", () => {
  it("the allowlist contains all read-only/print subcommands and the 4 N6A execution subcommands", () => {
    assert.deepStrictEqual([...ALLOWED_SUBCOMMANDS].sort(), [
      "audit-workspace",
      "find-artifacts",
      "help",
      "package-commit",
      "package-plan",
      "package-pr",
      "package-push",
      "print-apply",
      "print-apply-bundle",
      "print-approval",
      "print-preview",
      "print-tier3-evidence",
      "validate-input",
      "verify-final",
    ]);
  });

  it("contains NO chain-write run/approve/apply executor subcommand", () => {
    for (const s of ALLOWED_SUBCOMMANDS) {
      assert.ok(!/^run$|^approve$|^apply$|^apply-bundle$/.test(s), `unexpected executor subcommand: ${s}`);
    }
  });

  it("refuses an unapproved subcommand", () => {
    assert.throws(
      () => buildHelperRequest({ helperPath: HELPER, subcommand: "run" as never }),
      HelperRunnerRefusal,
    );
  });

  it("refuses an unapproved flag for a subcommand", () => {
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: HELPER,
          subcommand: "find-artifacts",
          options: { plan: "/x/plan.yaml" }, // find-artifacts only allows --workspace
        }),
      HelperRunnerRefusal,
    );
  });
});

describe("helperRunner — print-tier3-evidence (Option B refresh)", () => {
  it("builds `<helper> print-tier3-evidence --workspace <ws>` exactly", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "print-tier3-evidence",
      options: { workspace: "/disposable/wks" },
    });
    assert.strictEqual(req.binary, HELPER);
    assert.deepStrictEqual(req.args, ["print-tier3-evidence", "--workspace", "/disposable/wks"]);
  });

  it("is a read-only/print subcommand (print- prefix, no executor verb)", () => {
    assert.ok((ALLOWED_SUBCOMMANDS as readonly string[]).includes("print-tier3-evidence"));
    assert.ok(!/^run$|^approve$|^apply$|^apply-bundle$/.test("print-tier3-evidence"));
  });

  it("allows ONLY the --workspace flag", () => {
    assert.deepStrictEqual(ALLOWED_FLAGS["print-tier3-evidence"], ["workspace"]);
  });

  it("refuses any flag other than --workspace", () => {
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: HELPER,
          subcommand: "print-tier3-evidence",
          options: { plan: "/x/plan.yaml" },
        }),
      HelperRunnerRefusal,
    );
  });

  it("refuses a flag-shaped or chain-write-shaped workspace value", () => {
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: HELPER,
          subcommand: "print-tier3-evidence",
          options: { workspace: "--apply" },
        }),
      HelperRunnerRefusal,
    );
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: HELPER,
          subcommand: "print-tier3-evidence",
          options: { workspace: "/x; claw plan apply y" },
        }),
      HelperRunnerRefusal,
    );
  });

  it("never produces a bare `claw` or chain-write argv element", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "print-tier3-evidence",
      options: { workspace: "/disposable/wks" },
    });
    for (const a of [req.binary, ...req.args]) {
      assert.ok(a !== "claw", "argv must not contain a bare `claw`");
      assert.ok(!/claw\s+plan\s+(run|approve|apply-bundle|apply)\b/.test(a), `chain-write phrase: ${a}`);
    }
  });
});

describe("helperRunner — binary boundary (never claw)", () => {
  it("refuses a binary whose basename is not the helper", () => {
    assert.throws(
      () => buildHelperRequest({ helperPath: "/usr/local/bin/claw", subcommand: "help" }),
      HelperRunnerRefusal,
    );
  });

  it("accepts only the exact helper basename", () => {
    const req = buildHelperRequest({ helperPath: HELPER, subcommand: "help" });
    const base = req.binary.split("/").pop();
    assert.strictEqual(base, HELPER_BASENAME);
  });

  it("refuses a helper path containing a chain-write fragment", () => {
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: "/x/claw plan apply/a2-ide-harness.sh",
          subcommand: "help",
        }),
      HelperRunnerRefusal,
    );
  });
});

describe("helperRunner — value refusals", () => {
  it("refuses a flag-shaped value", () => {
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: HELPER,
          subcommand: "audit-workspace",
          options: { workspace: "--apply" },
        }),
      HelperRunnerRefusal,
    );
  });

  it("refuses a value containing `claw plan approve`", () => {
    assert.throws(
      () =>
        buildHelperRequest({
          helperPath: HELPER,
          subcommand: "audit-workspace",
          options: { workspace: "/x; claw plan approve y" },
        }),
      HelperRunnerRefusal,
    );
  });

  it("never produces an argv element equal to `claw` or a chain-write phrase", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "print-apply",
      options: { "apply-bundle": "/disposable/wks/.claw/ab.json" },
    });
    for (const a of [req.binary, ...req.args]) {
      assert.ok(a !== "claw", "argv must not contain a bare `claw`");
      assert.ok(!/claw\s+plan\s+(run|approve|apply-bundle|apply)\b/.test(a), `chain-write phrase in argv: ${a}`);
    }
  });

  it("never emits a flag-shaped arg of its own beyond the fixed `--<flag>` forms", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "validate-input",
      options: { workspace: "/disposable/wks", plan: "/disposable/wks/plan.yaml" },
    });
    const flagArgs = req.args.filter((a) => a.startsWith("-"));
    for (const f of flagArgs) {
      assert.ok(/^--(workspace|plan)$/.test(f), `unexpected flag arg: ${f}`);
    }
  });
});

describe("helperRunner — spawn injection", () => {
  it("runHelper spawns exactly once with the bounded argv", async () => {
    const seen: Array<{ binary: string; args: string[] }> = [];
    const mock: SpawnImpl = async (req) => {
      seen.push({ binary: req.binary, args: [...req.args] });
      const r: SpawnResult = { exitCode: 0, stdout: "ok", stderr: "" };
      return r;
    };
    await runHelper(
      { helperPath: HELPER, subcommand: "audit-workspace", options: { workspace: "/disposable/wks" } },
      mock,
    );
    assert.strictEqual(seen.length, 1);
    assert.strictEqual(seen[0].binary, HELPER);
    assert.deepStrictEqual(seen[0].args, ["audit-workspace", "--workspace", "/disposable/wks"]);
  });

  it("refuses before any spawn when a subcommand is not allowlisted", async () => {
    let calls = 0;
    const mock: SpawnImpl = async () => {
      calls++;
      return { exitCode: 0, stdout: "", stderr: "" };
    };
    await assert.rejects(
      runHelper({ helperPath: HELPER, subcommand: "approve" as never }, mock),
      HelperRunnerRefusal,
    );
    assert.strictEqual(calls, 0);
  });

  it("every allowlisted subcommand has a defined flag set", () => {
    for (const s of ALLOWED_SUBCOMMANDS) {
      assert.ok(Array.isArray(ALLOWED_FLAGS[s]), `missing ALLOWED_FLAGS for ${s}`);
    }
  });
});
