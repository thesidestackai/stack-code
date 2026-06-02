// Static audit over the package manifest (package.json). Used by
// test/manifest_audit.test.ts and exported so any consumer of this
// package can re-run the audit deterministically.
//
// Source of record: docs/a2-l3-ide-adapter-implementation-scope-card.md
// §10 (forbidden controls) and §20 (CI matrix row
// "No-write-controls manifest audit").

export interface PackageManifest {
  name: string;
  contributes?: {
    commands?: Array<{ command: string; title: string; category?: string }>;
    keybindings?: Array<{ command: string; key: string; when?: string }>;
    menus?: Record<string, unknown>;
    configuration?: { properties?: Record<string, unknown> };
  };
  activationEvents?: string[];
  dependencies?: Record<string, string>;
  devDependencies?: Record<string, string>;
}

export const FORBIDDEN_COMMAND_FRAGMENTS = [
  "approve",
  "apply",
  "apply-bundle",
  "run-plan",
  "runplan",
  "approveAndApply",
  "approve-and-apply",
  "automaticApproval",
  "automatic-approval",
  "automaticApply",
  "automatic-apply",
  "batchApproval",
  "batch-approval",
  "preapproval",
  "preApprove",
  "oneClickContinue",
  "one-click-continue",
  "trustWorkspace",
  "trust-workspace",
  "trustThisWorkspace",
  "ignoreStop",
  "ignore-stop",
  "muteStop",
  "mute-stop",
  "dismissStop",
  "dismiss-stop",
  "hideStop",
  "hide-stop",
  "snoozeStop",
  "snooze-stop",
];

export const FORBIDDEN_SETTING_NAME_FRAGMENTS = [
  "autoApprove",
  "auto-approve",
  "autoApply",
  "auto-apply",
  "approveAndApply",
  "trustWorkspace",
  "trustThisWorkspace",
  "disposable",
  "safeMode",
  "preApprove",
  "preapproval",
  "batchApprove",
  "batch-approve",
  "autoRefresh",
  "auto-refresh",
  "pollInterval",
  "pollingInterval",
  "refreshOnSave",
  "refreshOnFocus",
  "refreshOnGitPull",
];

export const FORBIDDEN_HTTP_DEPENDENCIES = [
  "axios",
  "node-fetch",
  "got",
  "request",
  "superagent",
  "ky",
  "undici",
];

export const FORBIDDEN_TELEMETRY_DEPENDENCIES = [
  "@vscode/extension-telemetry",
  "vscode-extension-telemetry",
  "@sentry/node",
  "@sentry/browser",
  "@sentry/vsts",
  "newrelic",
  "datadog-metrics",
  "@datadog/browser-logs",
  "@honeycombio/opentelemetry-node",
  "@opentelemetry/api",
  "@opentelemetry/sdk-node",
  "applicationinsights",
  "posthog-node",
  "mixpanel",
];

export const FORBIDDEN_WATCHER_DEPENDENCIES = [
  "chokidar",
  "watchman",
  "fb-watchman",
  "node-watch",
  "@parcel/watcher",
  "nsfw",
];

export interface AuditFinding {
  category:
    | "forbidden-command"
    | "forbidden-setting"
    | "forbidden-http-dep"
    | "forbidden-telemetry-dep"
    | "forbidden-watcher-dep"
    | "forbidden-activation-event";
  detail: string;
}

const FORBIDDEN_ACTIVATION_PATTERNS: RegExp[] = [
  new RegExp("^onStartupFinished$"),
  new RegExp("^onFileSystem"),
  new RegExp("^onLanguage:"),
  new RegExp("^onView:"),
  new RegExp("^onUri:"),
  new RegExp("^onWebviewPanel:"),
  new RegExp("^onSave"),
  new RegExp("^onChange"),
];

export function auditManifest(manifest: PackageManifest): AuditFinding[] {
  const findings: AuditFinding[] = [];

  const commands = manifest.contributes?.commands ?? [];
  for (const c of commands) {
    const haystack = `${c.command} ${c.title} ${c.category ?? ""}`.toLowerCase();
    for (const fragment of FORBIDDEN_COMMAND_FRAGMENTS) {
      if (haystack.includes(fragment.toLowerCase())) {
        findings.push({
          category: "forbidden-command",
          detail: `command ${c.command} matches forbidden fragment "${fragment}"`,
        });
      }
    }
  }

  const keybindings = manifest.contributes?.keybindings ?? [];
  for (const k of keybindings) {
    const haystack = k.command.toLowerCase();
    for (const fragment of FORBIDDEN_COMMAND_FRAGMENTS) {
      if (haystack.includes(fragment.toLowerCase())) {
        findings.push({
          category: "forbidden-command",
          detail: `keybinding ${k.key} → ${k.command} matches forbidden fragment "${fragment}"`,
        });
      }
    }
  }

  const settings = manifest.contributes?.configuration?.properties ?? {};
  for (const key of Object.keys(settings)) {
    for (const fragment of FORBIDDEN_SETTING_NAME_FRAGMENTS) {
      if (key.toLowerCase().includes(fragment.toLowerCase())) {
        findings.push({
          category: "forbidden-setting",
          detail: `setting ${key} matches forbidden fragment "${fragment}"`,
        });
      }
    }
  }

  const deps = {
    ...(manifest.dependencies ?? {}),
    ...(manifest.devDependencies ?? {}),
  };
  const depNames = Object.keys(deps);

  for (const d of depNames) {
    if (FORBIDDEN_HTTP_DEPENDENCIES.includes(d)) {
      findings.push({ category: "forbidden-http-dep", detail: d });
    }
    if (FORBIDDEN_TELEMETRY_DEPENDENCIES.includes(d)) {
      findings.push({ category: "forbidden-telemetry-dep", detail: d });
    }
    if (FORBIDDEN_WATCHER_DEPENDENCIES.includes(d)) {
      findings.push({ category: "forbidden-watcher-dep", detail: d });
    }
  }

  const activation = manifest.activationEvents ?? [];
  for (const ev of activation) {
    for (const pattern of FORBIDDEN_ACTIVATION_PATTERNS) {
      if (pattern.test(ev)) {
        findings.push({
          category: "forbidden-activation-event",
          detail: `activation event "${ev}" matches forbidden pattern ${pattern.toString()}`,
        });
      }
    }
  }

  return findings;
}

export function loadManifest(path: string): PackageManifest {
  // Synchronous read by design: this is invoked from tests / CI guards, not
  // from the runtime panel. We do NOT read `.claw/**` from anywhere in this
  // package; this read is bounded to the package's own package.json.
  const fs = require("fs") as typeof import("fs");
  const raw = fs.readFileSync(path, { encoding: "utf8" });
  return JSON.parse(raw) as PackageManifest;
}
