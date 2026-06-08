import * as assert from "assert";
import { computeSetupStatus, SetupProbe, SetupInputs } from "../src/setupStatus";
import { parseAuditWorkspace } from "../src/discovery";

function emptyInputs(): SetupInputs {
  return {
    workspace: null,
    plan: null,
    target: null,
    afterSha: null,
    previewBundle: null,
    generatorResult: null,
    approvalResult: null,
    applyBundle: null,
  };
}

function baseProbe(): SetupProbe {
  return {
    helperProbe: "not-run",
    clawPath: null,
    workspaceRoot: null,
    inputs: emptyInputs(),
    audit: null,
    planCandidates: [],
  };
}

describe("setupStatus — helper path", () => {
  it("found when a read-only probe ran", () => {
    const p = baseProbe();
    p.helperProbe = "ran";
    assert.strictEqual(computeSetupStatus(p).helperPath, "found");
  });
  it("missing when the probe hit a spawn error", () => {
    const p = baseProbe();
    p.helperProbe = "spawn-error";
    assert.strictEqual(computeSetupStatus(p).helperPath, "missing");
  });
  it("not-checked before any probe", () => {
    assert.strictEqual(computeSetupStatus(baseProbe()).helperPath, "not-checked");
  });
});

describe("setupStatus — claw binary is configured/unknown, never claimed found", () => {
  it("configured when a path was parsed", () => {
    const p = baseProbe();
    p.clawPath = "/build/claw";
    assert.strictEqual(computeSetupStatus(p).clawBinary, "configured");
  });
  it("unknown when no path is known", () => {
    assert.strictEqual(computeSetupStatus(baseProbe()).clawBinary, "unknown");
  });
});

describe("setupStatus — workspace root", () => {
  it("detected when a root is present", () => {
    const p = baseProbe();
    p.workspaceRoot = "/disposable/wks";
    assert.strictEqual(computeSetupStatus(p).workspaceRoot, "detected");
  });
  it("not-detected otherwise", () => {
    assert.strictEqual(computeSetupStatus(baseProbe()).workspaceRoot, "not-detected");
  });
});

describe("setupStatus — plan discovery", () => {
  it("found when the operator already set a plan", () => {
    const p = baseProbe();
    p.inputs.plan = "/a/plan.yaml";
    assert.strictEqual(computeSetupStatus(p).plan, "found");
  });
  it("found when exactly one candidate is discovered", () => {
    const p = baseProbe();
    p.planCandidates = ["/a/plan.yaml"];
    assert.strictEqual(computeSetupStatus(p).plan, "found");
  });
  it("select-needed when multiple candidates are discovered", () => {
    const p = baseProbe();
    p.planCandidates = ["/a/plan.yaml", "/b/plan.yaml"];
    assert.strictEqual(computeSetupStatus(p).plan, "select-needed");
  });
  it("unknown when nothing is set or discovered", () => {
    assert.strictEqual(computeSetupStatus(baseProbe()).plan, "unknown");
  });
});

describe("setupStatus — target / after-sha", () => {
  it("known when set", () => {
    const p = baseProbe();
    p.inputs.target = "/a/out.txt";
    p.inputs.afterSha = "abc";
    const s = computeSetupStatus(p);
    assert.strictEqual(s.target, "known");
    assert.strictEqual(s.afterSha, "known");
  });
  it("unknown when unset", () => {
    const s = computeSetupStatus(baseProbe());
    assert.strictEqual(s.target, "unknown");
    assert.strictEqual(s.afterSha, "unknown");
  });
});

describe("setupStatus — artifact presence reflects audit", () => {
  const AUDIT = `chain state: preview-ready
  present : preview-bundle.json  (/d/.claw/preview-bundle.json)
  absent  : approval-result.json
  absent  : apply-bundle.json
`;
  it("not-checked before an audit", () => {
    const s = computeSetupStatus(baseProbe());
    assert.strictEqual(s.previewBundle, "not-checked");
    assert.strictEqual(s.approvalResult, "not-checked");
    assert.strictEqual(s.applyBundle, "not-checked");
  });
  it("found/not-found after an audit", () => {
    const p = baseProbe();
    p.audit = parseAuditWorkspace(AUDIT);
    const s = computeSetupStatus(p);
    assert.strictEqual(s.previewBundle, "found");
    assert.strictEqual(s.approvalResult, "not-found");
    assert.strictEqual(s.applyBundle, "not-found");
  });
  it("an operator-set field counts as found even without audit", () => {
    const p = baseProbe();
    p.inputs.previewBundle = "/d/.claw/preview-bundle.json";
    assert.strictEqual(computeSetupStatus(p).previewBundle, "found");
  });
});

describe("setupStatus — final verification", () => {
  it("match / mismatch / not-checked", () => {
    assert.strictEqual(computeSetupStatus(baseProbe()).finalVerification, "not-checked");

    const m = baseProbe();
    m.audit = parseAuditWorkspace("chain state: applied\n  MATCH — ok\n");
    assert.strictEqual(computeSetupStatus(m).finalVerification, "match");

    const x = baseProbe();
    x.audit = parseAuditWorkspace("chain state: applied\n  MISMATCH — bad\n");
    assert.strictEqual(computeSetupStatus(x).finalVerification, "mismatch");
  });
});
