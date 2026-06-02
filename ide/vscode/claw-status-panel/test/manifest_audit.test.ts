import * as assert from "assert";
import {
  auditManifest,
  loadManifest,
  PackageManifest,
} from "../src/manifest_audit";
import { PACKAGE_JSON_PATH } from "./_paths";

describe("manifest_audit — shipped package.json passes the audit", () => {
  it("auditManifest on the real package.json returns NO findings", () => {
    const manifest = loadManifest(PACKAGE_JSON_PATH);
    const findings = auditManifest(manifest);
    assert.deepStrictEqual(
      findings,
      [],
      `expected zero findings; got: ${JSON.stringify(findings, null, 2)}`,
    );
  });

  it("declared commands list contains ONLY clawStatus.refresh", () => {
    const manifest = loadManifest(PACKAGE_JSON_PATH);
    const cmds = manifest.contributes?.commands ?? [];
    assert.strictEqual(cmds.length, 1);
    assert.strictEqual(cmds[0].command, "clawStatus.refresh");
  });

  it("declared keybindings list is absent or empty", () => {
    const manifest = loadManifest(PACKAGE_JSON_PATH);
    const kbs = manifest.contributes?.keybindings ?? [];
    assert.strictEqual(kbs.length, 0);
  });

  it("activationEvents list contains ONLY operator-gesture-driven events", () => {
    const manifest = loadManifest(PACKAGE_JSON_PATH);
    const events = manifest.activationEvents ?? [];
    for (const ev of events) {
      assert.ok(
        ev.startsWith("onCommand:"),
        `activation event ${ev} must be onCommand:* (operator-gesture-driven)`,
      );
    }
  });

  it("declared settings contain NO auto-refresh / poll-interval / trust-workspace", () => {
    const manifest = loadManifest(PACKAGE_JSON_PATH);
    const settings = Object.keys(manifest.contributes?.configuration?.properties ?? {});
    for (const s of settings) {
      const lc = s.toLowerCase();
      assert.ok(!lc.includes("autorefresh"), `setting ${s} must not name 'autorefresh'`);
      assert.ok(!lc.includes("auto-refresh"), `setting ${s} must not name 'auto-refresh'`);
      assert.ok(!lc.includes("pollinterval"), `setting ${s} must not name 'pollinterval'`);
      assert.ok(!lc.includes("trust"), `setting ${s} must not contain 'trust'`);
      assert.ok(!lc.includes("disposable"), `setting ${s} must not contain 'disposable'`);
    }
  });

  it("dependencies include NO HTTP / telemetry / watcher clients", () => {
    const manifest = loadManifest(PACKAGE_JSON_PATH);
    const deps = {
      ...(manifest.dependencies ?? {}),
      ...(manifest.devDependencies ?? {}),
    };
    const forbidden = [
      "axios",
      "node-fetch",
      "got",
      "request",
      "superagent",
      "ky",
      "undici",
      "@vscode/extension-telemetry",
      "vscode-extension-telemetry",
      "@sentry/node",
      "@sentry/browser",
      "newrelic",
      "datadog-metrics",
      "@datadog/browser-logs",
      "applicationinsights",
      "posthog-node",
      "mixpanel",
      "chokidar",
      "watchman",
      "fb-watchman",
      "node-watch",
      "@parcel/watcher",
      "nsfw",
    ];
    for (const d of forbidden) {
      assert.ok(!(d in deps), `forbidden dependency present: ${d}`);
    }
  });
});

describe("manifest_audit — synthetic violations", () => {
  it("detects an approve-bearing command", () => {
    const m: PackageManifest = {
      name: "x",
      contributes: {
        commands: [{ command: "clawStatus.approve", title: "Approve" }],
      },
    };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-command"));
  });

  it("detects an auto-approve setting", () => {
    const m: PackageManifest = {
      name: "x",
      contributes: {
        configuration: { properties: { "clawStatus.autoApprove": {} } },
      },
    };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-setting"));
  });

  it("detects a forbidden HTTP dependency", () => {
    const m: PackageManifest = { name: "x", dependencies: { axios: "1.0.0" } };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-http-dep"));
  });

  it("detects a forbidden telemetry dependency", () => {
    const m: PackageManifest = {
      name: "x",
      dependencies: { "@vscode/extension-telemetry": "0.9.0" },
    };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-telemetry-dep"));
  });

  it("detects a forbidden watcher dependency", () => {
    const m: PackageManifest = { name: "x", dependencies: { chokidar: "3.0.0" } };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-watcher-dep"));
  });

  it("detects a forbidden activation event", () => {
    const m: PackageManifest = {
      name: "x",
      activationEvents: ["onStartupFinished"],
    };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-activation-event"));
  });

  it("detects an apply-bound keybinding", () => {
    const m: PackageManifest = {
      name: "x",
      contributes: {
        keybindings: [{ command: "clawStatus.apply", key: "ctrl+a" }],
      },
    };
    const findings = auditManifest(m);
    assert.ok(findings.some((f) => f.category === "forbidden-command"));
  });
});
