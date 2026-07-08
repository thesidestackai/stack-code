import * as path from "path";
import * as vscode from "vscode";
import { A2HarnessPanel, PanelMessage } from "./panel";
import {
  RenderModel,
  PanelInputs,
  HelperOutput,
  NavView,
  DiscoveryView,
  WorkspaceStatusView,
  NorthstarLadderView,
  N3PanelView,
  N4PanelView,
  N5PanelView,
  FoundationView,
  Tier3View,
  ExecutorDryRunView,
  emptyInputs,
  renderHtml,
} from "./render";
import {
  EvidenceSnapshotView,
  parseEvidenceSnapshot,
} from "./tier3EvidenceSnapshot";
import { refreshOutcomeFromResult } from "./tier3EvidenceRefresh";
import {
  computeTier3Readiness,
  dirtyControlCheckoutBlock,
  Tier3Readiness,
} from "./tier3Readiness";
import { validateWorktreePlan, summarizePlan } from "./disposableWorktreePlan";
import {
  formatMutationLedger,
  MutationLedgerEvent,
  mutationEvent,
} from "./mutationEvidence";
import { policyInvariant } from "./safeMutationPolicy";
import { ApprovedLane, computeDryRun, summarizeDryRun } from "./executorDryRun";
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
  WorkspaceProbe,
  computeWorkspaceStatusCard,
  renderWorkspaceStatusLines,
} from "./workspaceStatus";
import {
  NorthstarSignals,
  emptyNorthstarSignals,
  buildNorthstarView,
} from "./northstarState";
import {
  TaskDraft,
  TaskIntakeEvent,
  emptyTaskDraft,
  reduceTaskIntake,
  renderTaskIntakeLines,
} from "./n3TaskIntake";
import { riskDisposition } from "./n3RiskClassifier";
import { renderPlanDraftLines } from "./n3PlanDraft";
import { buildN3View, n3ToLadderSignals } from "./n3State";
import { buildN4View } from "./n4View";
import { buildN5View } from "./n5View";
import {
  N6SessionState,
  N6PanelView,
  emptyN6SessionState,
  buildN6View,
} from "./n6View";
import {
  N6_SUB_TOKEN_PLAN,
  N6_SUB_TOKEN_COMMIT,
  N6_SUB_TOKEN_PUSH,
  N6_SUB_TOKEN_PR,
} from "./n6State";
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
  // Northstar Phase N2 read-only views (workspace status card + state model).
  workspaceCard: WorkspaceStatusView | null;
  northstar: NorthstarLadderView | null;
  // Northstar Phase N3 local task intake draft + its read-only view.
  taskDraft: TaskDraft;
  n3: N3PanelView | null;
  // Northstar Phase N4 read-only preview/diff/evidence viewer over the N3 draft.
  n4: N4PanelView | null;
  // Northstar Phase N5 read-only gated execution readiness board.
  n5: N5PanelView | null;
  // Northstar Phase N6 execution boundary.
  // n6State: in-memory token + exec state (D2=A; never persisted).
  // n6: derived panel view, recomputed on every state change.
  n6State: N6SessionState;
  n6: N6PanelView | null;
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
  // Tier 3 Foundation v0: session-local mutation evidence ledger (read-only).
  mutationLedger: MutationLedgerEvent[];
  // Operator-provided a2-tier3-evidence-snapshot.v0 text (Option A acquisition).
  // The operator runs the read-only collector themselves and pastes the output;
  // the panel obtains it by no spawn. null until provided.
  evidenceSnapshotText: string | null;
}

const session: SessionState = {
  panel: null,
  inputs: emptyInputs(),
  output: null,
  notice: null,
  setup: null,
  nav: null,
  discovery: null,
  workspaceCard: null,
  northstar: null,
  taskDraft: emptyTaskDraft("task-1"),
  n3: null,
  n4: null,
  n5: null,
  n6State: emptyN6SessionState(),
  n6: null,
  timeline: [],
  validated: false,
  audit: null,
  helperProbe: "not-run",
  clawPath: null,
  planCandidates: [],
  agentLedger: [],
  mutationLedger: [],
  evidenceSnapshotText: null,
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

// Build the read-only Tier 3 Foundation v0 view from the pure Tier 3 modules.
// v0 supplies NO facts (no guard-safe probe is wired), declares no plan and no
// touched files, and grants no approval — so readiness renders not-checked /
// not-ready, no mutation is enabled, and nothing is created or written.
function tier3ReadinessRows(r: Tier3Readiness): Array<{ label: string; value: string }> {
  return [
    { label: "control checkout clean", value: r.controlCheckoutClean },
    { label: "origin/main confirmed", value: r.originMainConfirmed },
    { label: "worktree path free", value: r.worktreePathFree },
    { label: "branch name free", value: r.branchNameFree },
    { label: "operator approved", value: r.operatorApproved },
    { label: "plan valid", value: r.planValid },
    { label: "declared scope present", value: r.declaredScopePresent },
    { label: "denied registry loaded", value: r.deniedRegistryLoaded },
  ];
}

function buildTier3View(): Tier3View {
  // v0: no worktree plan is proposed yet (plan only; never created).
  const planValidation = validateWorktreePlan(null);
  const readiness = computeTier3Readiness({
    // No facts supplied in v0 → honest not-checked.
    planValid: planValidation.valid,
    declaredScopePresent: false,
    deniedRegistryLoaded: true,
  });
  const ledger: MutationLedgerEvent[] = session.mutationLedger;
  return {
    readinessRows: tier3ReadinessRows(readiness),
    overall: readiness.overall,
    dirtyControlCheckoutBlock: dirtyControlCheckoutBlock(readiness),
    probeNote: readiness.probeNote,
    planLines: summarizePlan(null),
    planValid: planValidation.valid,
    planProblems: planValidation.problems,
    declaredPaths: [],
    policyInvariant: policyInvariant(),
    ledgerLines: formatMutationLedger(ledger),
    operatorApproved: false,
  };
}

// Build the read-only Tier 3 Mutation Executor v0 (dry-run) view. v0 has no
// approved lane loaded (objective/plan/declared-scope empty; operator not
// approved), so the dry-run is not-ready and prints the external command only —
// it creates no worktree and writes nothing.
function buildExecutorDryRunView(): ExecutorDryRunView {
  const emptyLane: ApprovedLane = {
    objective: null,
    worktreePlan: null,
    declaredPaths: [],
    proposedWrites: [],
    proposedCommands: [],
    operatorApproved: false,
  };
  // No facts are supplied in v0 (no guard-safe probe), so readiness is not-ready.
  const result = computeDryRun(emptyLane);
  const evidence: MutationLedgerEvent[] = [
    mutationEvent({
      kind: "decision",
      tier: 3,
      action: "dry-run computed (no approved lane)",
      status: "info",
      summary: result.summary,
      printedNotRun: true,
    }),
  ];
  return {
    printedCommand: result.printedCommand,
    resultLines: summarizeDryRun(result),
    summary: result.summary,
    wouldCreateWorktree: result.wouldCreateWorktree,
    wouldWriteFiles: result.wouldWriteFiles,
    evidenceLines: formatMutationLedger(evidence),
  };
}

// Build the read-only Tier 3 evidence snapshot view (Option A). The SOLE input
// is operator-provided snapshot text held in session; nothing is acquired here
// (no fs, no spawn, no network). When no text is set, the view is absent and the
// section degrades to a muted placeholder. A bad/mismatched snapshot yields the
// pure renderer's fail-closed view (it never fabricates readiness).
function buildEvidenceSnapshotView(): EvidenceSnapshotView | null {
  const text = session.evidenceSnapshotText;
  if (typeof text !== "string" || text.trim().length === 0) {
    return null;
  }
  return parseEvidenceSnapshot(text);
}

function model(): RenderModel {
  return {
    inputs: session.inputs,
    output: session.output,
    notice: session.notice,
    setup: session.setup,
    nav: session.nav,
    discovery: session.discovery,
    workspaceCard: session.workspaceCard,
    northstar: session.northstar,
    n3: session.n3,
    n4: session.n4,
    n5: session.n5,
    n6: session.n6,
    timeline: session.timeline.length > 0 ? formatTimeline(session.timeline) : null,
    foundation: buildFoundationView(),
    tier3: buildTier3View(),
    executorDryRun: buildExecutorDryRunView(),
    evidenceSnapshot: buildEvidenceSnapshotView(),
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

// Northstar Phase N3 — dispatch a task-intake reducer event over the local
// session draft, then recompute views + re-render. The reducer is pure; this
// boundary supplies the timestamp and re-renders. It spawns nothing, reads no
// file, calls no model, and never runs apply/package/PR.
function dispatchN3(event: TaskIntakeEvent): void {
  session.taskDraft = reduceTaskIntake(session.taskDraft, event);
  record(timelineEvent("field-set", `n3 ${event.type} -> ${session.taskDraft.draft_status}`));
  recomputeViews();
  rerender();
}

function n3Stamp(): { now: string } {
  return { now: new Date().toISOString() };
}

// Capture the task summary + intent (free text). Empty input is a no-op.
async function n3DescribeTask(): Promise<void> {
  const summary = await vscode.window.showInputBox({
    prompt: "Task summary (one line). The plan draft is non-executing; nothing runs.",
    ignoreFocusOut: true,
  });
  if (summary === undefined) {
    return;
  }
  const intent = await vscode.window.showInputBox({
    prompt: "Operator intent (free text describing the desired change).",
    ignoreFocusOut: true,
  });
  if (intent === undefined) {
    return;
  }
  const ws = session.inputs.workspace ?? defaultWorkspace();
  dispatchN3({ type: "DescribeTask", summary, intent, workspaceRoot: ws, stamp: n3Stamp() });
}

// Add one exact declared target path (no globs; deny-list always wins).
async function n3AddDeclaredPath(): Promise<void> {
  const value = await vscode.window.showInputBox({
    prompt: "Declare ONE exact, workspace-relative target path (no globs, no absolute paths).",
    ignoreFocusOut: true,
  });
  if (value !== undefined && value.trim().length > 0) {
    dispatchN3({ type: "DeclareTarget", path: value.trim(), stamp: n3Stamp() });
  }
}

// Add one explicit forbidden path (in addition to the always-denied families).
async function n3AddForbiddenPath(): Promise<void> {
  const value = await vscode.window.showInputBox({
    prompt: "Add ONE forbidden path (deny-list). Runtime/services/HQ/Vault/secrets are always denied.",
    ignoreFocusOut: true,
  });
  if (value !== undefined && value.trim().length > 0) {
    dispatchN3({ type: "DeclareForbidden", path: value.trim(), stamp: n3Stamp() });
  }
}

// Produce the non-executing plan draft and validate it (classify -> draft ->
// validate). This runs no command and opens nothing; the result is a review
// artifact that is structurally non-runnable.
function n3DraftPlan(): void {
  const stamp = n3Stamp();
  session.taskDraft = reduceTaskIntake(session.taskDraft, { type: "DraftPlan", stamp });
  session.taskDraft = reduceTaskIntake(session.taskDraft, { type: "ValidateDraft", stamp });
  record(timelineEvent("field-set", `n3 DraftPlan+Validate -> ${session.taskDraft.draft_status}`));
  recomputeViews();
  rerender();
}

function n3Reset(): void {
  dispatchN3({ type: "Reset", taskId: "task-1", stamp: n3Stamp() });
}

// Option A acquisition: capture the operator-provided evidence-snapshot text.
// The operator runs the read-only collector themselves and pastes its output;
// this only stores the text on the session and re-renders. It spawns nothing,
// reads no file, and runs no helper subcommand. Clearing the input removes it.
async function pasteEvidenceSnapshot(): Promise<void> {
  const value = await vscode.window.showInputBox({
    prompt:
      "Paste the read-only a2-tier3-evidence-snapshot.v0 JSON (run the collector yourself; the panel obtains nothing). Empty clears it.",
    ignoreFocusOut: true,
  });
  if (value !== undefined) {
    const v = value.trim().length > 0 ? value.trim() : null;
    session.evidenceSnapshotText = v;
    session.notice = null;
    record(timelineEvent("field-set", `evidence-snapshot = ${v ? "(provided)" : "(cleared)"}`));
    rerender();
  }
}

// Option B acquisition: read-only in-panel refresh of the Tier 3 evidence
// snapshot. Runs the print-only `print-tier3-evidence` helper subcommand through
// the SAME single spawn boundary (helperRunner) the rest of the panel uses; that
// subcommand runs the read-only, writes-nothing, non-claw collector and prints
// its a2-tier3-evidence-snapshot.v0 to stdout. We store that stdout as the
// session snapshot text (fed to the existing pure parser/renderer) — exactly the
// Option A path, only the acquisition source changes from paste to the helper.
// It creates no worktree, writes no file, and runs no claw/model/broker/runtime.
// Fail-closed: a non-zero exit or empty output clears the snapshot and shows a
// notice instead of fabricating readiness.
async function refreshTier3EvidenceSnapshot(): Promise<void> {
  const helperPath = resolveHelperPath();
  const ws = session.inputs.workspace ?? defaultWorkspace();
  if (!helperPath || !ws) {
    session.notice =
      "Set a workspace first (or configure an absolute a2HarnessPanel.helperPath) to refresh Tier 3 evidence.";
    rerender();
    return;
  }
  const inv: HelperInvocation = {
    helperPath,
    subcommand: "print-tier3-evidence",
    options: { workspace: ws },
  };
  try {
    const result = await runHelper(inv, defaultSpawnImpl());
    const outcome = refreshOutcomeFromResult(result);
    session.evidenceSnapshotText = outcome.snapshotText;
    session.notice = outcome.notice;
    record(
      timelineEvent(
        "helper",
        "print-tier3-evidence — Tier 3 evidence refreshed read-only",
        result.exitCode,
      ),
    );
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    session.notice = `Tier 3 evidence refresh refused/failed: ${msg}`;
    record(timelineEvent("note", "print-tier3-evidence refused/failed"));
  }
  rerender();
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

  // Northstar Phase N2 — read-only workspace status card. The root is auto-
  // detected from the vscode workspace folder (available on open). Branch /
  // clean-dirty / origin-main freshness need a read-only git probe the print/
  // validate-only helper does not yet emit, so they stay honest unknowns until a
  // later phase wires it. No fs, no spawn here.
  const workspaceProbe: WorkspaceProbe = {
    workspaceRoot: ws,
    branch: null,
    worktreeClean: null,
    originMainFreshness: null,
  };
  const card = computeWorkspaceStatusCard(workspaceProbe);
  session.workspaceCard = {
    lines: renderWorkspaceStatusLines(card),
    gitProbeNote: card.gitProbeNote,
  };

  // Northstar Phase N2 — read-only state-model view, derived from the same
  // read-only signals (workspace + validate-input + the audit chain state).
  // Forward signals (task/plan-draft/package/PR/disposition) are not observed in
  // N2 and stay false; the model then rests at the most-advanced OBSERVED state
  // and never auto-advances past the apply gate (buildNorthstarView asserts it).
  const chain = session.audit ? session.audit.chainState : null;
  // Northstar Phase N3: the local task-draft produces the early-ladder signals
  // (taskDescribed / planDrafted / planValidated) the N2 model already consumes.
  // It never sets a signal at or beyond the apply gate.
  const ladder = n3ToLadderSignals(session.taskDraft);
  const nsSignals: NorthstarSignals = {
    ...emptyNorthstarSignals(),
    workspaceReady: typeof ws === "string" && ws.trim().length > 0,
    taskDescribed: ladder.taskDescribed,
    planDrafted: ladder.planDrafted,
    planValidated: ladder.planValidated || session.validated,
    previewReady: chain === "preview-ready",
    awaitingApplyApproval: chain === "approval-ready" || chain === "apply-bundle-ready",
    appliedObserved: chain === "applied",
  };
  const nsView = buildNorthstarView(nsSignals);

  // Northstar Phase N3 read-only view from the local task draft.
  const n3v = buildN3View(session.taskDraft);
  const draft = session.taskDraft;
  session.n3 = {
    state: n3v.state,
    stepLabel: n3v.stepLabel,
    isBlocked: n3v.isBlocked,
    isTerminal: n3v.isTerminal,
    riskLevel: draft.risk_level ?? "(unclassified)",
    riskDisposition: draft.risk_level ? riskDisposition(draft.risk_level) : "(n/a)",
    intakeLines: renderTaskIntakeLines(draft),
    planDraftLines: draft.plan_draft ? renderPlanDraftLines(draft.plan_draft) : null,
    lintStatus: draft.plan_validation ? draft.plan_validation.status : null,
    lintReasons: draft.plan_validation ? draft.plan_validation.reasons : [],
  };

  // Northstar Phase N4 — read-only preview/diff/evidence viewer over the same
  // local N3 draft. Display-only: it renders present data labelled by trust
  // level and fails closed; it runs no preview/apply/package/PR and writes
  // nothing. buildN4View asserts no N4 state routes to the apply gate.
  const n4v = buildN4View(session.taskDraft);
  session.n4 = {
    state: n4v.state,
    stepLabel: n4v.stepLabel,
    isBlocked: n4v.isBlocked,
    preview: { trust: n4v.preview.trust, lines: n4v.preview.lines },
    diff: { trust: n4v.diff.trust, lines: n4v.diff.lines },
    evidence: { trust: n4v.evidence.trust, lines: n4v.evidence.lines },
  };

  // Northstar Phase N5 — read-only gated execution readiness board over the
  // same local N3/N4 draft. Display-only: renders per-rung package-ladder
  // readiness labelled by trust level (incl. EXECUTION_REQUIRED); fails closed;
  // runs no rung, opens no PR, writes nothing, calls no model/broker/runtime.
  // buildN5View asserts no N5 state routes to any execution-capable target.
  const n5v = buildN5View(session.taskDraft);
  session.n5 = {
    state: n5v.state,
    stepLabel: n5v.stepLabel,
    isBlocked: n5v.isBlocked,
    n4State: n5v.n4State,
    n4StepLabel: n5v.n4StepLabel,
    taskSummary: n5v.taskSummary,
    riskLevel: n5v.riskLevel,
    ladder: n5v.ladder,
  };

  // Northstar Phase N6 — execution boundary view. Pure: reads the current
  // in-memory N6 session state + the N5 ladder readiness (for plan READY gate).
  // No spawn, no file, no model/broker/Vault.
  session.n6 = buildN6View(session.n5, session.n6State);

  session.northstar = {
    state: nsView.state,
    stateClass: nsView.stateClass,
    stepLabel: nsView.stepLabel,
    stepKind: nsView.stepKind,
    automatable: nsView.automatable,
    requiresRealTty: nsView.requiresRealTty,
  };

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
    case "n3DescribeTask":
      await n3DescribeTask();
      return;
    case "n3AddDeclaredPath":
      await n3AddDeclaredPath();
      return;
    case "n3AddForbiddenPath":
      await n3AddForbiddenPath();
      return;
    case "n3DraftPlan":
      n3DraftPlan();
      return;
    case "n3Reset":
      n3Reset();
      return;
    case "openRunbook":
      await openRunbook();
      return;
    case "exportEvidence":
      await exportEvidence();
      return;
    // Northstar Phase N6 — sub-token activation (D1=A: VS Code input box).
    case "n6ActivatePlanToken":
      await n6ActivateToken("plan", N6_SUB_TOKEN_PLAN);
      return;
    case "n6ActivateCommitToken":
      await n6ActivateToken("commit", N6_SUB_TOKEN_COMMIT);
      return;
    case "n6ActivatePushToken":
      await n6ActivateToken("push", N6_SUB_TOKEN_PUSH);
      return;
    case "n6ActivatePrToken":
      await n6ActivateToken("pr", N6_SUB_TOKEN_PR);
      return;
    // Northstar Phase N6 — rung execution dispatch.
    case "n6RunPlan":
      await n6RunRung("plan");
      return;
    case "n6RunCommit":
      await n6RunRung("commit");
      return;
    case "n6RunPush":
      await n6RunRung("push");
      return;
    case "n6RunPr":
      await n6RunRung("pr");
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

// ---- Northstar Phase N6 helpers --------------------------------------------
//
// Two-level token model (D2=A in-memory only):
//   Level 1 = implementation token (activates the N6 section globally — checked
//             by the operator before this session starts; not re-checked here).
//   Level 2 = per-rung sub-tokens (exact string match; in-memory only; cleared
//             on FAILED — D4=B).
//
// No spawn happens in token-activation paths. Execution dispatch goes through
// helperRunner.ts (single spawn boundary). No model/broker/Vault calls.

// n6ActivateToken: show input box, validate exact sub-token, set session state.
// D1=A: operator supplies the sub-token via VS Code input box.
async function n6ActivateToken(
  rung: "plan" | "commit" | "push" | "pr",
  expected: string,
): Promise<void> {
  const entered = await vscode.window.showInputBox({
    prompt: `Enter N6 sub-token for rung "${rung}" (exact match required)`,
    ignoreFocusOut: true,
    password: false,
  });
  if (entered === undefined) {
    // Cancelled — no-op.
    return;
  }
  if (entered.trim() !== expected) {
    session.notice = `N6: token mismatch for ${rung} — not activated`;
    recomputeViews();
    rerender();
    return;
  }
  // Token validated. Set active flag and advance exec state to TOKEN_ACTIVE.
  const s = session.n6State;
  if (rung === "plan")   { s.planTokenActive   = true; s.planExec   = "TOKEN_ACTIVE"; }
  if (rung === "commit") { s.commitTokenActive = true; s.commitExec = "TOKEN_ACTIVE"; }
  if (rung === "push")   { s.pushTokenActive   = true; s.pushExec   = "TOKEN_ACTIVE"; }
  if (rung === "pr")     { s.prTokenActive     = true; s.prExec     = "TOKEN_ACTIVE"; }
  session.notice = null;
  recomputeViews();
  rerender();
}

// n6ClearToken: D4=B — clear sub-token on FAILED so the operator must supply a
// fresh one to retry. Called by n6RunRung on any non-zero exit or spawn error.
function n6ClearToken(rung: "plan" | "commit" | "push" | "pr"): void {
  const s = session.n6State;
  if (rung === "plan")   { s.planTokenActive   = false; }
  if (rung === "commit") { s.commitTokenActive = false; }
  if (rung === "push")   { s.pushTokenActive   = false; }
  if (rung === "pr")     { s.prTokenActive     = false; }
}

// n6SetExec: update per-rung exec state + captured output/exit.
function n6SetExec(
  rung: "plan" | "commit" | "push" | "pr",
  exec: "RUNNING" | "DONE" | "FAILED",
  output?: string,
  exitCode?: number,
): void {
  const s = session.n6State;
  if (rung === "plan") {
    s.planExec = exec;
    if (output !== undefined) { s.planOutput   = output; }
    if (exitCode !== undefined) { s.planExitCode = exitCode; }
  } else if (rung === "commit") {
    s.commitExec = exec;
    if (output !== undefined) { s.commitOutput   = output; }
    if (exitCode !== undefined) { s.commitExitCode = exitCode; }
  } else if (rung === "push") {
    s.pushExec = exec;
    if (output !== undefined) { s.pushOutput   = output; }
    if (exitCode !== undefined) { s.pushExitCode = exitCode; }
  } else {
    s.prExec = exec;
    if (output !== undefined) { s.prOutput   = output; }
    if (exitCode !== undefined) { s.prExitCode = exitCode; }
  }
}

// n6RunRung: dispatch a package rung through helperRunner.ts.
// Called only when the operator has supplied the exact sub-token AND the
// run button is visible (showRunButton === true from buildN6View).
// IMPORTANT: NO live execution in the N6 IMPLEMENTATION lane — the run
// buttons are only visible after Level 2 sub-tokens are supplied at runtime.
async function n6RunRung(rung: "plan" | "commit" | "push" | "pr"): Promise<void> {
  // Guard: token must be active.
  const s = session.n6State;
  const tokenActive = {
    plan:   s.planTokenActive,
    commit: s.commitTokenActive,
    push:   s.pushTokenActive,
    pr:     s.prTokenActive,
  }[rung];
  if (!tokenActive) {
    session.notice = `N6: ${rung} sub-token not active — activate it first`;
    rerender();
    return;
  }

  const helperPath = resolveHelperPath();
  if (!helperPath) {
    session.notice = "N6: set workspace first (or configure helperPath)";
    rerender();
    return;
  }

  const ws = session.inputs.workspace ?? defaultWorkspace();
  if (!ws) {
    session.notice = "N6: workspace required — set it first";
    rerender();
    return;
  }

  // Build the HelperInvocation options per rung.
  // ALLOWED_FLAGS enforced by helperRunner.ts; values sourced from session or input box.
  let sub: HelperSubcommand;
  let options: Record<string, string | string[]>;

  if (rung === "plan") {
    const planPath = session.inputs.plan;
    if (!planPath) {
      session.notice = "N6: plan file required for package-plan — set it first";
      rerender();
      return;
    }
    sub = "package-plan";
    options = { workspace: ws, plan: planPath };
    if (session.clawPath) {
      options["claw-binary"] = session.clawPath;
    }
  } else if (rung === "commit") {
    const files = session.taskDraft.declared_target_paths;
    if (!files || files.length === 0) {
      session.notice = "N6: declared_target_paths required for package-commit — add paths in N3";
      rerender();
      return;
    }
    const message = session.taskDraft.task_summary ?? "A2 harness: package commit";
    sub = "package-commit";
    options = { workspace: ws, file: files, message };
  } else if (rung === "push") {
    // Ask operator for remote and branch (no git probe available in extension).
    const remote = await vscode.window.showInputBox({
      prompt: "N6 package-push: remote name (e.g. origin)",
      value: "origin",
      ignoreFocusOut: true,
    });
    if (!remote || remote.trim().length === 0) { return; }
    const branch = await vscode.window.showInputBox({
      prompt: "N6 package-push: branch name to push",
      ignoreFocusOut: true,
    });
    if (!branch || branch.trim().length === 0) { return; }
    sub = "package-push";
    options = { workspace: ws, remote: remote.trim(), branch: branch.trim() };
  } else {
    // package-pr (D5=A): operator supplies all PR fields via input box.
    const base = await vscode.window.showInputBox({
      prompt: "N6 package-pr: base branch (e.g. main)",
      value: "main",
      ignoreFocusOut: true,
    });
    if (!base || base.trim().length === 0) { return; }
    const head = await vscode.window.showInputBox({
      prompt: "N6 package-pr: head branch",
      ignoreFocusOut: true,
    });
    if (!head || head.trim().length === 0) { return; }
    const title = await vscode.window.showInputBox({
      prompt: "N6 package-pr: PR title (max 256 chars)",
      value: session.taskDraft.task_summary ?? "",
      ignoreFocusOut: true,
    });
    if (!title || title.trim().length === 0) { return; }
    // D5=A: operator supplies body-file path.
    const bodyFile = await vscode.window.showInputBox({
      prompt: "N6 package-pr: path to PR body file (D5-A)",
      ignoreFocusOut: true,
    });
    if (!bodyFile || bodyFile.trim().length === 0) {
      session.notice = "N6: body-file path required for package-pr (D5-A)";
      rerender();
      return;
    }
    sub = "package-pr";
    options = {
      workspace: ws,
      base: base.trim(),
      head: head.trim(),
      title: title.trim(),
      "body-file": bodyFile.trim(),
    };
  }

  // Dispatch through helperRunner.ts (single spawn boundary).
  n6SetExec(rung, "RUNNING");
  recomputeViews();
  rerender();

  const inv: HelperInvocation = { helperPath, subcommand: sub, options };
  const subLabel = sub; // captured for catch block (TypeScript definite assignment)
  try {
    const result = await runHelper(inv, defaultSpawnImpl());
    const combinedOutput = result.stdout + (result.stderr ? `\n[stderr]\n${result.stderr}` : "");
    if (result.exitCode === 0) {
      n6SetExec(rung, "DONE", combinedOutput, result.exitCode);
    } else {
      n6SetExec(rung, "FAILED", combinedOutput, result.exitCode);
      n6ClearToken(rung); // D4=B: clear token on FAILED
    }
    session.output = { subcommand: sub, exitCode: result.exitCode, stdout: result.stdout, stderr: result.stderr };
    record(timelineEvent("helper", `N6 ${subLabel}`, result.exitCode));
    recomputeViews();
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    n6SetExec(rung, "FAILED", msg, -1);
    n6ClearToken(rung); // D4=B: clear token on error
    session.notice = `N6 ${rung} dispatch failed: ${msg}`;
    record(timelineEvent("note", `N6 ${subLabel} dispatch error`));
    recomputeViews();
  }
  rerender();
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
  // Northstar Phase N2: auto-detect the workspace on open and populate the
  // read-only workspace status card + state-model view before the first render.
  // recomputeViews is pure (computeSetupStatus / computeWorkspaceStatusCard /
  // buildNorthstarView) — it spawns nothing and reads no file; the workspace
  // root comes from the vscode folder via defaultWorkspace().
  recomputeViews();
  session.panel.show(model());
  // Workspace-first: kick off a single read-only status refresh on open so the
  // panel shows setup status + next safe step without the operator typing
  // anything. Fire-and-forget (no timer, no watcher); it re-renders when done.
  void refreshWorkspaceStatus();
}

export function activate(context: vscode.ExtensionContext): void {
  const disposable = vscode.commands.registerCommand("a2HarnessPanel.open", openPanel);
  context.subscriptions.push(disposable);
  const pasteSnapshot = vscode.commands.registerCommand(
    "a2HarnessPanel.pasteEvidenceSnapshot",
    pasteEvidenceSnapshot,
  );
  context.subscriptions.push(pasteSnapshot);
  const refreshSnapshot = vscode.commands.registerCommand(
    "a2HarnessPanel.refreshTier3EvidenceSnapshot",
    refreshTier3EvidenceSnapshot,
  );
  context.subscriptions.push(refreshSnapshot);
}

export function deactivate(): void {
  if (session.panel) {
    session.panel.dispose();
    session.panel = null;
  }
}

// Exported for type-checking parity; renderHtml is the single render entry.
export { renderHtml };
