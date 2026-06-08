import * as path from "path";
import * as vscode from "vscode";
import { A2HarnessPanel, PanelMessage } from "./panel";
import {
  RenderModel,
  PanelInputs,
  HelperOutput,
  NavView,
  DiscoveryView,
  FoundationView,
  emptyInputs,
  renderHtml,
} from "./render";
import {
  PERMISSION_TIERS,
  TierId,
  defaultEffectiveTier,
  assertEffectiveTierSafe,
} from "./permissionTiers";
import { deniedFamilyLabels } from "./deniedCommands";
import { computeReadiness, dirtyCheckoutWarning, AgentReadiness } from "./agentReadiness";
import {
  AgentLedgerEvent,
  ledgerEvent,
  appendLedger,
  formatLedger,
} from "./agentEvidence";
import {
  ALLOWED_FLAGS,
  HelperSubcommand,
  HelperInvocation,
  defaultSpawnImpl,
  runHelper,
  ALLOWED_SUBCOMMANDS,
} from "./helperRunner";
import { PANEL_BUTTONS, HelperButton } from "./buttons";
import {
  AuditParse,
  ArtifactName,
  parseAuditWorkspace,
  parseHelpClawPath,
  selectCandidate,
  auditPathFor,
  ARTIFACT_NAMES,
} from "./discovery";
import {
  SetupStatus,
  HelperProbe,
  computeSetupStatus,
} from "./setupStatus";
import {
  deriveState,
  nextSafeStep,
  stepLabel,
  stepButtonId,
  assertSafe,
} from "./stateMachine";
import {
  TimelineEvent,
  event as timelineEvent,
  append as appendEvent,
  formatTimeline,
} from "./evidence";

// Maps a helper flag name to the PanelInputs key that supplies its value.
const FLAG_TO_INPUT: Record<string, keyof PanelInputs> = {
  "workspace": "workspace",
  "plan": "plan",
  "preview-bundle": "previewBundle",
  "preview-generator-result": "generatorResult",
  "approval-result": "approvalResult",
  "approval-output": "approvalOutput",
  "apply-bundle": "applyBundle",
  "target": "target",
  "after-sha": "afterSha",
};

interface SessionState {
  panel: A2HarnessPanel | null;
  inputs: PanelInputs;
  output: HelperOutput | null;
  notice: string | null;
  // Workspace-first read-only state.
  setup: SetupStatus | null;
  nav: NavView | null;
  discovery: DiscoveryView | null;
  timeline: TimelineEvent[];
  // True once a validate-input run exited 0 in this session.
  validated: boolean;
  // Last audit-workspace parse, used to drive setup status + the state machine.
  audit: AuditParse | null;
  // Result of the last one-shot helper presence probe (`help`).
  helperProbe: HelperProbe;
  // Configured claw path parsed from the helper usage output (never verified).
  clawPath: string | null;
  // plan.yaml candidates from the last read-only vscode search.
  planCandidates: string[];
  // A2 Local Coding Agent Foundation v0: session-local agent evidence ledger.
  agentLedger: AgentLedgerEvent[];
}

const session: SessionState = {
  panel: null,
  inputs: emptyInputs(),
  output: null,
  notice: null,
  setup: null,
  nav: null,
  discovery: null,
  timeline: [],
  validated: false,
  audit: null,
  helperProbe: "not-run",
  clawPath: null,
  planCandidates: [],
  agentLedger: [],
};

// Build the read-only A2 Local Coding Agent Foundation v0 view from the pure
// foundation modules. This adds NO capability: it computes the current
// (read-only) permission tier, an honest readiness view (git state stays
// not-checked — v0 wires no guard-safe git probe), the denied-command registry
// vocabulary, the agent evidence ledger lines, and the proposed next lane
// (which enables no mutation).
function readinessRows(r: AgentReadiness): Array<{ label: string; value: string }> {
  return [
    { label: "workspace root", value: r.workspaceRoot },
    { label: "repo detected", value: r.repoDetected },
    { label: "git branch", value: r.gitBranch },
    { label: "dirty checkout", value: r.dirtyState },
    { label: "staged changes", value: r.stagedChanges },
    { label: "unstaged changes", value: r.unstagedChanges },
    { label: "untracked files", value: r.untrackedFiles },
    { label: "current tier", value: "Tier " + String(r.currentTier) },
    { label: "denied registry loaded", value: r.deniedRegistryLoaded },
    { label: "safe executor mode", value: r.safeExecutorMode },
  ];
}

function buildFoundationView(): FoundationView {
  const ws = session.inputs.workspace ?? defaultWorkspace();
  const readOnlyHelperUsed = session.helperProbe === "ran";
  const currentTier: TierId = assertEffectiveTierSafe(defaultEffectiveTier(readOnlyHelperUsed));

  // v0: no git facts are supplied (no guard-safe probe is wired), so git
  // readiness renders honestly as not-checked.
  const readiness = computeReadiness({
    workspaceRoot: ws,
    currentTier,
    deniedRegistryLoaded: true,
    safeExecutorMode: "print-validate-only",
  });

  return {
    currentTier,
    readiness: {
      rows: readinessRows(readiness),
      dirtyWarning: dirtyCheckoutWarning(readiness),
      gitProbeNote: readiness.gitProbeNote,
    },
    tiers: PERMISSION_TIERS.map((t) => ({
      id: t.id,
      name: t.name,
      current: t.id === currentTier,
      deniedByDefault: t.deniedByDefault,
      requiresExplicitApproval: t.requiresExplicitApproval,
      summary: t.summary,
    })),
    deniedFamilies: deniedFamilyLabels(),
    ledgerLines: formatLedger(session.agentLedger),
    nextLane: {
      name: "A2 Local Coding Agent Foundation v0 Review / Push PR",
      summary:
        "Review the foundation control plane, then push the branch and open a PR for operator review. No mutation lane is enabled until v0 is merged and a separate, explicitly-approved mutation lane is opened.",
      mutationEnabled: false,
      blocked: [
        "file editing by the panel",
        "PR creation by the panel",
        "branch deletion by the panel",
        "live A2 chain execution (preview/approval/apply-bundle/apply)",
        "runtime/model/broker/service actions and raw :11434 inference",
        "hidden command execution",
      ],
    },
  };
}

function recordLedger(ev: AgentLedgerEvent): void {
  session.agentLedger = appendLedger(session.agentLedger, ev);
}

function model(): RenderModel {
  return {
    inputs: session.inputs,
    output: session.output,
    notice: session.notice,
    setup: session.setup,
    nav: session.nav,
    discovery: session.discovery,
    timeline: session.timeline.length > 0 ? formatTimeline(session.timeline) : null,
    foundation: buildFoundationView(),
  };
}

function record(ev: TimelineEvent): void {
  session.timeline = appendEvent(session.timeline, ev);
}

function rerender(): void {
  if (session.panel) {
    session.panel.show(model());
  }
}

function defaultWorkspace(): string | null {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return null;
  }
  return folders[0].uri.fsPath;
}

// Resolve the helper path from configuration. Relative paths resolve against
// the workspace folder. Returns null if it cannot be resolved.
function resolveHelperPath(): string | null {
  const config = vscode.workspace.getConfiguration("a2HarnessPanel");
  const raw = config.get<string>("helperPath", "scripts/a2-ide-harness.sh");
  const helper = raw && raw.trim().length > 0 ? raw : "scripts/a2-ide-harness.sh";
  if (path.isAbsolute(helper)) {
    return helper;
  }
  const ws = session.inputs.workspace ?? defaultWorkspace();
  if (!ws) {
    return null;
  }
  return path.join(ws, helper);
}

function isHelperSubcommand(s: string): s is HelperSubcommand {
  return (ALLOWED_SUBCOMMANDS as readonly string[]).includes(s);
}

// Build the helper options object for a subcommand from the currently-set
// inputs, restricted to that subcommand's allowed flags.
function optionsFor(sub: HelperSubcommand): { options: Record<string, string>; missing: string[] } {
  const options: Record<string, string> = {};
  const missing: string[] = [];
  const button = PANEL_BUTTONS.find(
    (b): b is HelperButton => b.kind === "helper" && b.subcommand === sub,
  );
  const needs = button ? button.needs : ALLOWED_FLAGS[sub];
  for (const flag of needs) {
    const key = FLAG_TO_INPUT[flag];
    const value = key ? session.inputs[key] : null;
    if (value && value.length > 0) {
      options[flag] = value;
    } else {
      missing.push(flag);
    }
  }
  return { options, missing };
}

async function runSubcommand(sub: string): Promise<void> {
  if (!isHelperSubcommand(sub)) {
    session.notice = `refused: ${sub} is not a read-only/print helper subcommand`;
    rerender();
    return;
  }
  const helperPath = resolveHelperPath();
  if (!helperPath) {
    session.notice = "Set a workspace first (or configure an absolute a2HarnessPanel.helperPath).";
    rerender();
    return;
  }
  const { options, missing } = optionsFor(sub);
  if (missing.length > 0) {
    session.notice = `Set these first for ${sub}: ${missing.join(", ")}`;
    rerender();
    return;
  }

  const inv: HelperInvocation = { helperPath, subcommand: sub, options };
  try {
    const result = await runHelper(inv, defaultSpawnImpl());
    session.output = {
      subcommand: sub,
      exitCode: result.exitCode,
      stdout: result.stdout,
      stderr: result.stderr,
    };
    session.notice = null;

    // A print-* subcommand only PRINTS a command; record it as printed-not-run.
    const printed = sub.indexOf("print-") === 0;
    record(
      timelineEvent(
        "helper",
        printed ? `${sub} — command printed (not run)` : sub,
        result.exitCode,
      ),
    );

    if (sub === "validate-input" && result.exitCode === 0) {
      session.validated = true;
    }
    if (sub === "audit-workspace") {
      session.audit = parseAuditWorkspace(result.stdout);
    }
    // Recompute the workspace-first views from the freshest signals.
    recomputeViews();
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    session.notice = `helper invocation refused/failed: ${msg}`;
    record(timelineEvent("note", `helper ${sub} refused/failed`));
  }
  rerender();
}

async function pickPath(prompt: string, key: keyof PanelInputs): Promise<void> {
  const value = await vscode.window.showInputBox({ prompt, ignoreFocusOut: true });
  if (value !== undefined) {
    const v = value.trim().length > 0 ? value.trim() : null;
    session.inputs[key] = v;
    session.notice = null;
    if (key === "plan") {
      // Changing the plan invalidates a prior validate-input pass.
      session.validated = false;
    }
    record(timelineEvent("field-set", `${String(key)} = ${v ?? "(cleared)"}`));
    recomputeViews();
    rerender();
  }
}

// Recompute the read-only workspace-first views (setup status, next-step,
// discovery) from the current session signals. Pure aggregation: it spawns
// nothing and reads no file — it only assembles already-gathered data.
function recomputeViews(): void {
  const ws = session.inputs.workspace ?? defaultWorkspace();
  session.setup = computeSetupStatus({
    helperProbe: session.helperProbe,
    clawPath: session.clawPath,
    workspaceRoot: ws,
    inputs: {
      workspace: session.inputs.workspace,
      plan: session.inputs.plan,
      target: session.inputs.target,
      afterSha: session.inputs.afterSha,
      previewBundle: session.inputs.previewBundle,
      generatorResult: session.inputs.generatorResult,
      approvalResult: session.inputs.approvalResult,
      applyBundle: session.inputs.applyBundle,
    },
    audit: session.audit,
    planCandidates: session.planCandidates,
  });

  const planKnown = session.setup.plan === "found";
  const state = deriveState({
    workspaceDetected: typeof ws === "string" && ws.trim().length > 0,
    planKnown,
    validated: session.validated,
    chainState: session.audit ? session.audit.chainState : null,
    targetHashChecked: session.audit ? session.audit.targetHash.checked : false,
    targetHashMatch: session.audit ? session.audit.targetHash.match : null,
  });
  const step = assertSafe(nextSafeStep(state));
  session.nav = { state, stepLabel: stepLabel(step), stepButtonId: stepButtonId(step) };

  session.discovery = buildDiscoveryView();
}

function buildDiscoveryView(): DiscoveryView | null {
  const lines: string[] = [];
  const planSel = selectCandidate(session.planCandidates);
  if (planSel.mode === "auto" && planSel.path) {
    lines.push(`plan.yaml: auto-selected ${planSel.path}`);
  } else if (planSel.mode === "select-needed") {
    lines.push(
      `plan.yaml: ${planSel.candidates.length} candidates — select one (${planSel.candidates.join(", ")})`,
    );
  }
  if (session.audit) {
    for (const name of ARTIFACT_NAMES) {
      const p = auditPathFor(session.audit, name);
      if (p) {
        lines.push(`${name}: ${p}`);
      }
    }
  }
  return lines.length > 0 ? { lines } : null;
}

// Discover plan.yaml candidates read-only via the vscode file index. This uses
// no node `fs`, sets up no watcher, and does not poll — it is a single search
// per Refresh gesture.
async function discoverPlans(): Promise<string[]> {
  try {
    const uris = await vscode.workspace.findFiles("**/plan.yaml", "**/node_modules/**", 25);
    return uris.map((u) => u.fsPath);
  } catch {
    return [];
  }
}

// Auto-fill a field ONLY when its discovery is a single unambiguous candidate
// and the field is currently unset. Every auto-fill is recorded and shown in the
// field table + discovery section before the operator uses it. Zero/many
// candidates are never silently inferred.
function autoPopulateDiscovered(): void {
  if (!session.inputs.plan) {
    const sel = selectCandidate(session.planCandidates);
    if (sel.mode === "auto" && sel.path) {
      session.inputs.plan = sel.path;
      record(timelineEvent("discovery", `plan.yaml auto-filled: ${sel.path}`));
    }
  }
  if (session.audit) {
    const map: Array<[ArtifactName, keyof PanelInputs]> = [
      ["preview-bundle.json", "previewBundle"],
      ["preview-generator-result.json", "generatorResult"],
      ["approval-result.json", "approvalResult"],
      ["apply-bundle.json", "applyBundle"],
    ];
    for (const [name, key] of map) {
      if (!session.inputs[key]) {
        const p = auditPathFor(session.audit, name);
        if (p) {
          session.inputs[key] = p;
          record(timelineEvent("discovery", `${name} auto-filled: ${p}`));
        }
      }
    }
  }
}

// One-shot, read-only workspace inspection. Runs only the allowlisted read-only
// helper subcommands (`help`, `audit-workspace`) + a vscode file search. It
// spawns no `claw`, runs no chain command, writes nothing, and adds no watcher
// or timer. Triggered on panel open and on the explicit Refresh button.
async function refreshWorkspaceStatus(): Promise<void> {
  const ws = session.inputs.workspace ?? defaultWorkspace();
  if (ws && !session.inputs.workspace) {
    session.inputs.workspace = ws;
    record(timelineEvent("workspace", `detected: ${ws}`));
  }

  const helperPath = resolveHelperPath();

  if (helperPath) {
    try {
      const help = await runHelper({ helperPath, subcommand: "help" }, defaultSpawnImpl());
      session.helperProbe = "ran";
      const claw = parseHelpClawPath(help.stdout);
      if (claw) {
        session.clawPath = claw;
      }
      record(timelineEvent("helper", "help (read-only probe)", help.exitCode));
    } catch {
      session.helperProbe = "spawn-error";
      record(timelineEvent("note", "helper not runnable (spawn error)"));
    }
  } else {
    session.helperProbe = "not-run";
  }

  if (helperPath && ws) {
    try {
      const audit = await runHelper(
        { helperPath, subcommand: "audit-workspace", options: { workspace: ws } },
        defaultSpawnImpl(),
      );
      session.audit = parseAuditWorkspace(audit.stdout);
      record(timelineEvent("helper", "audit-workspace (read-only)", audit.exitCode));
    } catch {
      record(timelineEvent("note", "audit-workspace not runnable"));
    }
  }

  session.planCandidates = await discoverPlans();
  autoPopulateDiscovered();

  session.notice = null;
  record(timelineEvent("status", "workspace status refreshed (read-only)"));
  // Foundation v0: note the readiness refresh and that git state stays
  // not-checked (no guard-safe git probe is wired in v0).
  recordLedger(
    ledgerEvent({
      kind: "readiness",
      tier: defaultEffectiveTier(session.helperProbe === "ran"),
      action: "refresh agent readiness",
      status: "ok",
      summary: "workspace/tier readiness refreshed; git readiness not-checked (no guard-safe probe in v0)",
    }),
  );
  recomputeViews();
  rerender();
}

async function handleUiAction(action: string): Promise<void> {
  switch (action) {
    case "selectWorkspace": {
      const ws = defaultWorkspace();
      if (ws && !session.inputs.workspace) {
        session.inputs.workspace = ws;
        rerender();
        return;
      }
      await pickPath("A2 workspace root (contains .claw and the target)", "workspace");
      return;
    }
    case "selectPlan":
      await pickPath("Path to plan.yaml (after_file must be relative)", "plan");
      return;
    case "selectPreviewBundle":
      await pickPath("Path to preview-bundle.json", "previewBundle");
      return;
    case "selectGeneratorResult":
      await pickPath("Path to preview-generator-result.json", "generatorResult");
      return;
    case "selectApprovalResult":
      await pickPath("Path to the persisted approval-result.json", "approvalResult");
      return;
    case "selectApprovalOutput":
      await pickPath("Path to write the new approval-result.json (must not exist)", "approvalOutput");
      return;
    case "selectApplyBundle":
      await pickPath("Path to apply-bundle.json", "applyBundle");
      return;
    case "selectTarget":
      await pickPath("Path to the target file written by plan apply", "target");
      return;
    case "setAfterSha":
      await pickPath("Expected after_sha256 of the target", "afterSha");
      return;
    case "refreshStatus":
      await refreshWorkspaceStatus();
      return;
    case "openRunbook":
      await openRunbook();
      return;
    case "exportEvidence":
      await exportEvidence();
      return;
    default:
      session.notice = `unknown action: ${action}`;
      rerender();
      return;
  }
}

async function openRunbook(): Promise<void> {
  const ws = session.inputs.workspace ?? defaultWorkspace();
  const candidate = ws
    ? path.join(ws, "docs", "runbooks", "a2-ide-harness-workflow.md")
    : null;
  if (!candidate) {
    session.notice = "Set a workspace to locate the runbook.";
    rerender();
    return;
  }
  try {
    const uri = vscode.Uri.file(candidate);
    await vscode.window.showTextDocument(uri, { preview: true });
  } catch {
    session.notice = `Could not open runbook at ${candidate}`;
    rerender();
  }
}

// Export a read-only evidence summary into an UNSAVED untitled document. The
// panel writes no file to disk; the operator saves it themselves if they wish.
async function exportEvidence(): Promise<void> {
  const i = session.inputs;
  const o = session.output;
  const s = session.setup;
  const nav = session.nav;
  const lines = [
    "# A2 Harness Panel — Evidence Summary",
    "",
    "## Inputs",
    `- workspace: ${i.workspace ?? "(not set)"}`,
    `- plan: ${i.plan ?? "(not set)"}`,
    `- preview-bundle: ${i.previewBundle ?? "(not set)"}`,
    `- preview-generator-result: ${i.generatorResult ?? "(not set)"}`,
    `- approval-result: ${i.approvalResult ?? "(not set)"}`,
    `- approval-output: ${i.approvalOutput ?? "(not set)"}`,
    `- apply-bundle: ${i.applyBundle ?? "(not set)"}`,
    `- target: ${i.target ?? "(not set)"}`,
    `- after-sha: ${i.afterSha ?? "(not set)"}`,
    "",
    "## Workspace status",
    s
      ? [
          `- helper path: ${s.helperPath}`,
          `- claw binary: ${s.clawBinary}`,
          `- workspace root: ${s.workspaceRoot}`,
          `- plan.yaml: ${s.plan}`,
          `- target: ${s.target}`,
          `- after_sha: ${s.afterSha}`,
          `- preview bundle: ${s.previewBundle}`,
          `- approval result: ${s.approvalResult}`,
          `- apply bundle: ${s.applyBundle}`,
          `- final verification: ${s.finalVerification}`,
        ].join("\n")
      : "- (not inspected yet — click Refresh Workspace Status)",
    "",
    "## Next safe step",
    nav ? `- state: ${nav.state}` : "- (no recommendation yet)",
    nav ? `- ${nav.stepLabel}` : "",
    "",
    "## Evidence timeline",
    ...formatTimeline(session.timeline).map((l) => `- ${l}`),
    "",
    "## Last helper output",
    o
      ? `- subcommand: ${o.subcommand} (exit ${o.exitCode})`
      : "- (no helper subcommand has been run yet)",
    "",
    "```text",
    o ? o.stdout : "",
    "```",
    "",
    "_This summary is read-only. The panel ran no A2 chain command, made no model/broker call, and wrote no target. Print steps are recorded as printed, not run._",
  ];
  const doc = await vscode.workspace.openTextDocument({
    content: lines.join("\n"),
    language: "markdown",
  });
  await vscode.window.showTextDocument(doc, { preview: false });
}

async function handleMessage(msg: PanelMessage): Promise<void> {
  if (msg.type === "runSubcommand") {
    await runSubcommand(msg.subcommand);
    return;
  }
  if (msg.type === "uiAction") {
    await handleUiAction(msg.action);
    return;
  }
  if (msg.type === "copyOutput") {
    const payload = session.output ? session.output.stdout : "";
    await vscode.env.clipboard.writeText(payload);
    vscode.window.setStatusBarMessage("A2 Harness: copied helper output to clipboard", 2000);
    return;
  }
}

function openPanel(): void {
  if (!session.panel) {
    session.panel = new A2HarnessPanel({ onMessage: handleMessage });
  }
  // A2 Local Coding Agent Foundation v0: record the session-open gesture in the
  // agent evidence ledger (read-only; no mutation, no execution).
  if (session.agentLedger.length === 0) {
    recordLedger(
      ledgerEvent({
        kind: "session",
        tier: defaultEffectiveTier(session.helperProbe === "ran"),
        action: "open agent cockpit (Foundation v0)",
        status: "info",
        summary: "read-only control plane; no mutation lane enabled",
      }),
    );
  }
  session.panel.show(model());
  // Workspace-first: kick off a single read-only status refresh on open so the
  // panel shows setup status + next safe step without the operator typing
  // anything. Fire-and-forget (no timer, no watcher); it re-renders when done.
  void refreshWorkspaceStatus();
}

export function activate(context: vscode.ExtensionContext): void {
  const disposable = vscode.commands.registerCommand("a2HarnessPanel.open", openPanel);
  context.subscriptions.push(disposable);
}

export function deactivate(): void {
  if (session.panel) {
    session.panel.dispose();
    session.panel = null;
  }
}

// Exported for type-checking parity; renderHtml is the single render entry.
export { renderHtml };
