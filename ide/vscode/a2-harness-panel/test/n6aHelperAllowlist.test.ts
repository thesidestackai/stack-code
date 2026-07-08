import * as assert from "assert";
import {
  ALLOWED_SUBCOMMANDS,
  ALLOWED_FLAGS,
  buildHelperRequest,
  HelperRunnerRefusal,
} from "../src/helperRunner";

const HELPER = "/disposable/wks/scripts/a2-ide-harness.sh";

describe("n6a — ALLOWED_SUBCOMMANDS", () => {
  it("contains package-plan", () => {
    assert.ok((ALLOWED_SUBCOMMANDS as readonly string[]).includes("package-plan"));
  });
  it("contains package-commit", () => {
    assert.ok((ALLOWED_SUBCOMMANDS as readonly string[]).includes("package-commit"));
  });
  it("contains package-push", () => {
    assert.ok((ALLOWED_SUBCOMMANDS as readonly string[]).includes("package-push"));
  });
  it("contains package-pr", () => {
    assert.ok((ALLOWED_SUBCOMMANDS as readonly string[]).includes("package-pr"));
  });
  it("has exactly 14 entries (10 print/validate + 4 N6A execution)", () => {
    assert.strictEqual(ALLOWED_SUBCOMMANDS.length, 14);
  });
  it("every allowlisted subcommand has a defined flag set (including N6A)", () => {
    for (const s of ALLOWED_SUBCOMMANDS) {
      assert.ok(Array.isArray(ALLOWED_FLAGS[s]), `missing ALLOWED_FLAGS for ${s}`);
    }
  });
});

describe("n6a — ALLOWED_FLAGS", () => {
  it("package-plan flags are exactly workspace / plan / claw-binary", () => {
    assert.deepStrictEqual(
      [...ALLOWED_FLAGS["package-plan"]].sort(),
      ["claw-binary", "plan", "workspace"],
    );
  });
  it("package-commit flags are exactly workspace / file / message", () => {
    assert.deepStrictEqual(
      [...ALLOWED_FLAGS["package-commit"]].sort(),
      ["file", "message", "workspace"],
    );
  });
  it("package-push flags are exactly workspace / remote / branch", () => {
    assert.deepStrictEqual(
      [...ALLOWED_FLAGS["package-push"]].sort(),
      ["branch", "remote", "workspace"],
    );
  });
  it("package-pr flags are exactly workspace / base / head / title / body-file", () => {
    assert.deepStrictEqual(
      [...ALLOWED_FLAGS["package-pr"]].sort(),
      ["base", "body-file", "head", "title", "workspace"],
    );
  });
  it("package-push flags contain no force-family strings", () => {
    for (const f of ["force", "force-with-lease", "force-if-includes"]) {
      assert.ok(!ALLOWED_FLAGS["package-push"].includes(f), `package-push must not allow --${f}`);
    }
  });
  it("package-pr flags contain no ready / approve / merge", () => {
    for (const f of ["ready", "approve", "merge"]) {
      assert.ok(!ALLOWED_FLAGS["package-pr"].includes(f), `package-pr must not allow --${f}`);
    }
  });
  it("package-commit flags contain no amend / all", () => {
    for (const f of ["amend", "all"]) {
      assert.ok(!ALLOWED_FLAGS["package-commit"].includes(f), `package-commit must not allow --${f}`);
    }
  });
});

describe("n6a — buildHelperRequest: package-plan", () => {
  it("builds correct argv for valid flags", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "package-plan",
      options: { workspace: "/ws", plan: "/ws/plan.yaml", "claw-binary": "/usr/local/bin/claw" },
    });
    assert.strictEqual(req.binary, HELPER);
    assert.strictEqual(req.args[0], "package-plan");
    assert.ok(req.args.includes("--workspace"));
    assert.ok(req.args.includes("--plan"));
    assert.ok(req.args.includes("--claw-binary"));
    assert.ok(req.args.includes("/usr/local/bin/claw"));
  });

  it("refuses unknown flag --force", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-plan",
        options: { workspace: "/ws", plan: "/ws/plan.yaml", "claw-binary": "/bin/claw", force: "true" },
      }),
      HelperRunnerRefusal,
    );
  });

  it("refuses flag-shaped claw-binary value", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-plan",
        options: { workspace: "/ws", plan: "/ws/plan.yaml", "claw-binary": "--force" },
      }),
      HelperRunnerRefusal,
    );
  });

  it("never produces a chain-write argv element", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "package-plan",
      options: { workspace: "/ws", plan: "/ws/plan.yaml", "claw-binary": "/bin/claw" },
    });
    for (const a of [req.binary, ...req.args]) {
      assert.ok(!/claw\s+plan\s+(approve|apply-bundle|apply)\b/.test(a), `forbidden chain-write in argv: ${a}`);
    }
  });
});

describe("n6a — buildHelperRequest: package-commit (repeated --file)", () => {
  it("builds correct argv for single file", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "package-commit",
      options: { workspace: "/ws", file: "src/foo.ts", message: "fix: update foo" },
    });
    assert.strictEqual(req.args[0], "package-commit");
    assert.ok(req.args.includes("--file"));
    assert.ok(req.args.includes("src/foo.ts"));
    assert.ok(req.args.includes("--message"));
  });

  it("builds repeated --file entries for array value", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "package-commit",
      options: { workspace: "/ws", file: ["src/a.ts", "src/b.ts"], message: "fix: update" },
    });
    const filePositions = req.args.reduce<number[]>((acc, v, i) => v === "--file" ? [...acc, i] : acc, []);
    assert.strictEqual(filePositions.length, 2);
    assert.strictEqual(req.args[filePositions[0] + 1], "src/a.ts");
    assert.strictEqual(req.args[filePositions[1] + 1], "src/b.ts");
  });

  it("refuses --amend flag", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-commit",
        options: { workspace: "/ws", file: "src/a.ts", message: "msg", amend: "true" },
      }),
      HelperRunnerRefusal,
    );
  });

  it("refuses --all flag", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-commit",
        options: { workspace: "/ws", file: "src/a.ts", message: "msg", all: "true" },
      }),
      HelperRunnerRefusal,
    );
  });
});

describe("n6a — buildHelperRequest: package-push", () => {
  it("builds correct argv for valid flags", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "package-push",
      options: { workspace: "/ws", remote: "origin", branch: "feat/n6a" },
    });
    assert.strictEqual(req.args[0], "package-push");
    assert.ok(req.args.includes("--remote"));
    assert.ok(req.args.includes("origin"));
    assert.ok(req.args.includes("--branch"));
    assert.ok(req.args.includes("feat/n6a"));
  });

  it("refuses --force flag", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-push",
        options: { workspace: "/ws", remote: "origin", branch: "feat/n6a", force: "true" },
      }),
      HelperRunnerRefusal,
    );
  });

  it("refuses --delete flag", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-push",
        options: { workspace: "/ws", remote: "origin", branch: "feat/n6a", delete: "true" },
      }),
      HelperRunnerRefusal,
    );
  });
});

describe("n6a — buildHelperRequest: package-pr", () => {
  it("builds correct argv for valid flags", () => {
    const req = buildHelperRequest({
      helperPath: HELPER,
      subcommand: "package-pr",
      options: {
        workspace: "/ws",
        base: "main",
        head: "feat/n6a",
        title: "feat: n6a allowlist",
        "body-file": "/tmp/body.md",
      },
    });
    assert.strictEqual(req.args[0], "package-pr");
    assert.ok(req.args.includes("--base"));
    assert.ok(req.args.includes("--head"));
    assert.ok(req.args.includes("--title"));
    assert.ok(req.args.includes("--body-file"));
  });

  it("refuses --ready flag", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-pr",
        options: { workspace: "/ws", base: "main", head: "feat/n6a", title: "t", "body-file": "/tmp/b", ready: "true" },
      }),
      HelperRunnerRefusal,
    );
  });

  it("refuses --merge flag", () => {
    assert.throws(
      () => buildHelperRequest({
        helperPath: HELPER,
        subcommand: "package-pr",
        options: { workspace: "/ws", base: "main", head: "feat/n6a", title: "t", "body-file": "/tmp/b", merge: "true" },
      }),
      HelperRunnerRefusal,
    );
  });
});
