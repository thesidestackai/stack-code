import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";

// Reads the shell harness source. Path from out-test/test/ → workspace root → scripts/.
// Mirrors the pattern used in panelClickHandler.test.ts for panel.ts.
const HARNESS_SRC = fs.readFileSync(
  path.join(__dirname, "../../../../../scripts/a2-ide-harness.sh"),
  "utf8",
);

// Extract only the non-comment lines of cmd_package_plan for targeted assertions.
// The function spans from the `cmd_package_plan()` line to the closing `}` that
// follows. We strip lines that start with optional whitespace + `#` (comments)
// so the checks are not fooled by explanatory comment text.
function extractPackagePlanBody(): string {
  const lines = HARNESS_SRC.split("\n");
  let inside = false;
  let depth = 0;
  const body: string[] = [];
  for (const line of lines) {
    if (!inside && /^cmd_package_plan\(\)/.test(line)) {
      inside = true;
    }
    if (!inside) continue;
    body.push(line);
    for (const ch of line) {
      if (ch === "{") depth++;
      if (ch === "}") depth--;
    }
    if (depth === 0 && body.length > 1) break;
  }
  // Strip comment lines so assertions test the live code path only.
  return body.filter((l) => !/^\s*#/.test(l)).join("\n");
}

const PLAN_BODY = extractPackagePlanBody();

describe("n6a harness — package-plan wrapper path (regression guard)", () => {
  describe("wrapper derived from workspace root (not CWD)", () => {
    it("derives wrapper as $ws/scripts/claw-sidestack-local", () => {
      assert.ok(
        PLAN_BODY.includes('$ws/scripts/claw-sidestack-local'),
        'cmd_package_plan must derive wrapper as "$ws/scripts/claw-sidestack-local" ' +
        "so the path is absolute regardless of process CWD",
      );
    });

    it("passes --wrapper to claw plan run", () => {
      assert.ok(
        PLAN_BODY.includes("--wrapper"),
        "cmd_package_plan must pass --wrapper to claw plan run; " +
        "without it claw falls back to a CWD-relative path that fails when the " +
        "panel spawns from a non-workspace-root directory",
      );
    });

    it("does NOT rely on bare scripts/claw-sidestack-local without workspace prefix", () => {
      // The old (broken) default is resolved relative to CWD by claw itself.
      // The harness must not pass the relative form explicitly.
      assert.ok(
        !PLAN_BODY.match(/--wrapper\s+["']?scripts\/claw-sidestack-local["']?/),
        "--wrapper must not use the bare relative path scripts/claw-sidestack-local; " +
        "use the workspace-rooted form $ws/scripts/claw-sidestack-local",
      );
    });
  });

  describe("wrapper existence / executable guards", () => {
    it("checks wrapper file exists before invoking claw", () => {
      assert.ok(
        PLAN_BODY.includes('-f "$wrapper"'),
        'cmd_package_plan must check [[ -f "$wrapper" ]] before calling claw plan run',
      );
    });

    it("checks wrapper is executable before invoking claw", () => {
      assert.ok(
        PLAN_BODY.includes('-x "$wrapper"'),
        'cmd_package_plan must check [[ -x "$wrapper" ]] before calling claw plan run',
      );
    });
  });

  describe("existing invariants preserved", () => {
    it("still passes --workspace-write-preview", () => {
      assert.ok(
        PLAN_BODY.includes("--workspace-write-preview"),
        "--workspace-write-preview must remain in the claw plan run invocation",
      );
    });

    it("does not introduce claw plan apply/approve/apply-bundle", () => {
      assert.ok(
        !PLAN_BODY.match(/claw.*plan\s+(apply|approve|apply-bundle)\b/),
        "cmd_package_plan must never invoke claw plan apply/approve/apply-bundle",
      );
    });

    it("does not introduce raw :11434", () => {
      assert.ok(
        !PLAN_BODY.includes("11434"),
        "cmd_package_plan must not reference raw :11434",
      );
    });
  });

  describe("other N6A subcommands unchanged", () => {
    // The fix must not alter package-commit/push/pr — verify by checking their
    // functions do NOT contain wrapper-related strings.
    function extractFunctionBody(name: string): string {
      const lines = HARNESS_SRC.split("\n");
      let inside = false;
      let depth = 0;
      const body: string[] = [];
      for (const line of lines) {
        if (!inside && new RegExp(`^${name}\\(\\)`).test(line)) {
          inside = true;
        }
        if (!inside) continue;
        body.push(line);
        for (const ch of line) {
          if (ch === "{") depth++;
          if (ch === "}") depth--;
        }
        if (depth === 0 && body.length > 1) break;
      }
      return body.filter((l) => !/^\s*#/.test(l)).join("\n");
    }

    it("package-commit does not reference claw-sidestack-local", () => {
      const body = extractFunctionBody("cmd_package_commit");
      assert.ok(body.length > 0, "cmd_package_commit not found");
      assert.ok(
        !body.includes("claw-sidestack-local"),
        "cmd_package_commit must not reference claw-sidestack-local (only package-plan uses it)",
      );
    });

    it("package-push does not reference claw-sidestack-local", () => {
      const body = extractFunctionBody("cmd_package_push");
      assert.ok(body.length > 0, "cmd_package_push not found");
      assert.ok(!body.includes("claw-sidestack-local"));
    });

    it("package-pr does not reference claw-sidestack-local", () => {
      const body = extractFunctionBody("cmd_package_pr");
      assert.ok(body.length > 0, "cmd_package_pr not found");
      assert.ok(!body.includes("claw-sidestack-local"));
    });
  });
});
