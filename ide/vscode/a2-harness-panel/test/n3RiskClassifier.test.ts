import * as assert from "assert";
import {
  RISK_CATEGORIES,
  classifyRisk,
  riskDisposition,
  isStopRisk,
  validateDeclaredPath,
  isForbiddenFamily,
  validateBoundaries,
  defaultForbiddenPaths,
  ALWAYS_FORBIDDEN_MARKERS,
} from "../src/n3RiskClassifier";

describe("n3RiskClassifier — risk disposition", () => {
  it("READ_ONLY/DOCS_ONLY proceed", () => {
    assert.strictEqual(riskDisposition("READ_ONLY"), "proceed");
    assert.strictEqual(riskDisposition("DOCS_ONLY"), "proceed");
  });
  it("DISPOSABLE_FIXTURE/SOURCE_EDIT require a future lane", () => {
    assert.strictEqual(riskDisposition("DISPOSABLE_FIXTURE"), "requires-future-lane");
    assert.strictEqual(riskDisposition("SOURCE_EDIT"), "requires-future-lane");
  });
  it("RUNTIME_CONFIG/SECRETS_OR_VAULT/DESTRUCTIVE_OR_FORCE/UNKNOWN are STOP (fail closed)", () => {
    for (const c of ["RUNTIME_CONFIG", "SECRETS_OR_VAULT", "DESTRUCTIVE_OR_FORCE", "UNKNOWN"] as const) {
      assert.strictEqual(riskDisposition(c), "stop");
      assert.strictEqual(isStopRisk(c), true);
    }
  });
  it("every category has a disposition", () => {
    for (const c of RISK_CATEGORIES) {
      assert.ok(["proceed", "requires-future-lane", "stop"].includes(riskDisposition(c)));
    }
  });
});

describe("n3RiskClassifier — declared path validation (exact, no globs)", () => {
  it("accepts an exact workspace-relative path", () => {
    assert.strictEqual(validateDeclaredPath("ide/vscode/a2-harness-panel/src/x.ts").ok, true);
  });
  it("rejects glob chars", () => {
    for (const p of ["src/**", "src/*.ts", "a[b].ts", "a{b}.ts", "a?.ts"]) {
      assert.strictEqual(validateDeclaredPath(p).ok, false, p);
    }
  });
  it("rejects absolute paths", () => {
    assert.strictEqual(validateDeclaredPath("/etc/passwd").ok, false);
    assert.strictEqual(validateDeclaredPath("C:\\win").ok, false);
  });
  it("rejects parent-escape and trailing-slash dir", () => {
    assert.strictEqual(validateDeclaredPath("../outside.ts").ok, false);
    assert.strictEqual(validateDeclaredPath("src/dir/").ok, false);
  });
  it("rejects empty", () => {
    assert.strictEqual(validateDeclaredPath("   ").ok, false);
  });
});

describe("n3RiskClassifier — forbidden families (deny-list wins)", () => {
  it("default forbidden set includes the always-denied families", () => {
    const def = defaultForbiddenPaths();
    for (const m of ALWAYS_FORBIDDEN_MARKERS) {
      assert.ok(def.includes(m), `missing ${m}`);
    }
  });
  it("isForbiddenFamily catches runtime/services/hq/vault/secrets/.env", () => {
    assert.strictEqual(isForbiddenFamily("services/orchestrator/app.py"), true);
    assert.strictEqual(isForbiddenFamily("runtime/config.toml"), true);
    assert.strictEqual(isForbiddenFamily("hq/page.tsx"), true);
    assert.strictEqual(isForbiddenFamily("infra/vault/policy.hcl"), true);
    assert.strictEqual(isForbiddenFamily("app/.env"), true);
    assert.strictEqual(isForbiddenFamily("src/ok.ts"), false);
  });
  it("validateBoundaries flags a declared path inside a forbidden family", () => {
    const r = validateBoundaries(["services/x.py"], defaultForbiddenPaths());
    assert.strictEqual(r.ok, false);
    assert.ok(r.problems.some((p) => p.includes("always-forbidden")));
  });
  it("validateBoundaries flags missing always-denied families in forbidden_paths", () => {
    const r = validateBoundaries(["src/ok.ts"], ["runtime"]);
    assert.strictEqual(r.ok, false);
    assert.ok(r.problems.some((p) => p.includes("missing always-denied")));
  });
  it("validateBoundaries passes for a clean source edit with the full deny-list", () => {
    const r = validateBoundaries(["src/ok.ts"], defaultForbiddenPaths());
    assert.strictEqual(r.ok, true, JSON.stringify(r.problems));
  });
});

describe("n3RiskClassifier — classifyRisk (fail closed)", () => {
  it("empty declared paths => READ_ONLY", () => {
    assert.strictEqual(classifyRisk([], ""), "READ_ONLY");
  });
  it("docs/md only => DOCS_ONLY", () => {
    assert.strictEqual(classifyRisk(["docs/x.md", "handoffs/y.md"], ""), "DOCS_ONLY");
  });
  it("source files => SOURCE_EDIT", () => {
    assert.strictEqual(classifyRisk(["src/a.ts"], ""), "SOURCE_EDIT");
  });
  it("vault/secret/.env => SECRETS_OR_VAULT (STOP)", () => {
    assert.strictEqual(classifyRisk(["infra/vault/p.hcl"], ""), "SECRETS_OR_VAULT");
    assert.strictEqual(classifyRisk(["app/.env"], ""), "SECRETS_OR_VAULT");
  });
  it("runtime/services/.service/docker => RUNTIME_CONFIG (STOP)", () => {
    assert.strictEqual(classifyRisk(["runtime/x.toml"], ""), "RUNTIME_CONFIG");
    assert.strictEqual(classifyRisk(["deploy/web.service"], ""), "RUNTIME_CONFIG");
  });
  it("destructive intent => DESTRUCTIVE_OR_FORCE (STOP), wins over path family", () => {
    assert.strictEqual(classifyRisk(["src/a.ts"], "do a git reset --hard then rebuild"), "DESTRUCTIVE_OR_FORCE");
    assert.strictEqual(classifyRisk(["docs/x.md"], "rm -rf the cache"), "DESTRUCTIVE_OR_FORCE");
  });
  it("fixture-only => DISPOSABLE_FIXTURE", () => {
    assert.strictEqual(classifyRisk(["test/fixtures/sample.txt"], ""), "DISPOSABLE_FIXTURE");
  });
  it("unrecognized => UNKNOWN (fail closed)", () => {
    assert.strictEqual(classifyRisk(["data/blob.bin"], ""), "UNKNOWN");
  });
});
