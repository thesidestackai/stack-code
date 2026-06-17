// Pure HTML renderer for the A2 Harness Panel. Given the current panel state
// (operator-selected inputs + the most recent helper output + the workspace-
// first setup status / next-step / discovery / evidence views), it returns the
// full webview HTML. It renders ONLY the safe button catalog, an output area
// that shows helper stdout verbatim, an always-visible Safety / Stop Gates
// section, and the read-only workspace-first sections. It computes no chain
// state of its own — the setup/next-step/discovery views are computed by the
// extension from the helper's read-only output and passed in.

import {
  PanelButton,
  HelperButton,
  helperButtons,
  fieldSetterButtons,
  workflowUiButtons,
} from "./buttons";
import { SetupStatus } from "./setupStatus";
import {
  EvidenceSnapshotView,
  renderEvidenceSnapshotHtml,
} from "./tier3EvidenceSnapshot";

export interface PanelInputs {
  workspace: string | null;
  plan: string | null;
  previewBundle: string | null;
  generatorResult: string | null;
  approvalResult: string | null;
  approvalOutput: string | null;
  applyBundle: string | null;
  target: string | null;
  afterSha: string | null;
}

export interface HelperOutput {
  subcommand: string;
  exitCode: number;
  stdout: string;
  stderr: string;
}

// Read-only view of the next-step state machine result.
export interface NavView {
  state: string;
  stepLabel: string;
  // The existing safe button id the operator should click next, or null for a
  // guidance-only step (open a workspace / done / stop).
  stepButtonId: string | null;
}

// Northstar Phase N2 — read-only WORKSPACE STATUS CARD view. Pre-formatted by
// the extension from the pure workspaceStatus module. Read-only display only.
export interface WorkspaceStatusView {
  lines: string[];
  // Honest reason git facts may be unknown (null when fully probed).
  gitProbeNote: string | null;
}

// Northstar Phase N2 — read-only state-model (ladder) view. Pre-formatted by the
// extension from the pure northstarState module. The render layer only displays
// the current state + the single recommended next safe step; it runs nothing.
export interface NorthstarLadderView {
  state: string;
  stateClass: string;
  stepLabel: string;
  stepKind: string;
  // True only for read-only steps; never for an apply/package/push/pr/merge.
  automatable: boolean;
  // True when the next gate is a REAL terminal (human-typed apply approval).
  requiresRealTty: boolean;
}

// Read-only discovery summary lines (e.g. "plan.yaml: auto-selected /a/plan.yaml",
// "preview-bundle.json: /d/.claw/preview-bundle.json"). Already formatted by the
// extension; every discovered path is shown here before it is used.
export interface DiscoveryView {
  lines: string[];
}

// A2 Local Coding Agent Foundation v0 — read-only control-plane view. All data
// is pre-computed by the extension from the pure foundation modules
// (permissionTiers / deniedCommands / agentSession / agentEvidence /
// agentReadiness). The render layer only displays it; it adds no capability and
// no action buttons.
export interface FoundationTierView {
  id: number;
  name: string;
  current: boolean;
  deniedByDefault: boolean;
  requiresExplicitApproval: boolean;
  summary: string;
}

export interface FoundationReadinessView {
  rows: Array<{ label: string; value: string }>;
  // True only when a real dirty fact says so (never fabricated).
  dirtyWarning: boolean;
  // Stated reason git readiness is not-checked (v0 has no guard-safe probe).
  gitProbeNote: string | null;
}

export interface FoundationNextLaneView {
  name: string;
  summary: string;
  // Always false in v0: no mutation lane is enabled.
  mutationEnabled: boolean;
  blocked: string[];
}

export interface FoundationView {
  currentTier: number;
  readiness: FoundationReadinessView;
  tiers: FoundationTierView[];
  deniedFamilies: string[];
  ledgerLines: string[];
  nextLane: FoundationNextLaneView;
}

// Tier 3 Foundation v0 — read-only control-plane view for the disposable
// worktree mutation path. All data is pre-computed by the extension from the
// pure Tier 3 modules (tier3Readiness / disposableWorktreePlan / mutationScope /
// safeMutationPolicy / mutationEvidence). The render layer only displays it; it
// adds NO mutation executor, NO worktree-creation control, and NO write button.
export interface Tier3View {
  // Honest readiness rows (label/value); git/worktree facts render not-checked.
  readinessRows: Array<{ label: string; value: string }>;
  overall: string; // "ready" | "not-ready"
  dirtyControlCheckoutBlock: boolean;
  probeNote: string | null;
  // Disposable worktree plan summary (plan only; never created).
  planLines: string[];
  planValid: boolean;
  planProblems: string[];
  // Declared exact-path touched-file set (shown before any mutation).
  declaredPaths: string[];
  // The safe-mutation policy invariant (denials win; exact-path).
  policyInvariant: string;
  // Mutation evidence ledger lines (printed-not-run markers).
  ledgerLines: string[];
  // Whether the operator has explicitly approved this exact lane (gate display).
  operatorApproved: boolean;
}

export interface RenderModel {
  inputs: PanelInputs;
  output: HelperOutput | null;
  // A non-load-bearing status line (e.g. "refused: workspace not set"). Never
  // a substitute for the helper stdout or the Safety section.
  notice: string | null;
  // Workspace-first read-only views. All optional: when absent (e.g. before the
  // first Refresh, or in unit fixtures) the sections degrade to a muted hint.
  setup?: SetupStatus | null;
  nav?: NavView | null;
  discovery?: DiscoveryView | null;
  // Northstar Phase N2 read-only views (optional; absent => muted hint). The
  // workspace status card auto-renders on open; the ladder view shows the
  // current Northstar state + the single recommended next safe step. Both are
  // read-only: they add no control and run no apply/package/PR/merge.
  workspaceCard?: WorkspaceStatusView | null;
  northstar?: NorthstarLadderView | null;
  // Pre-formatted evidence-timeline lines.
  timeline?: string[] | null;
  // A2 Local Coding Agent Foundation v0 control-plane view (optional; degrades
  // to a muted hint when absent).
  foundation?: FoundationView | null;
  // Tier 3 Foundation v0 control-plane view (optional; degrades to a muted hint
  // when absent). Read-only; adds no mutation/worktree-creation control.
  tier3?: Tier3View | null;
  // Tier 3 Mutation Executor v0 (dry-run) view (optional; degrades to a muted
  // hint when absent). Read-only; PRINTS the dry-run command + renders the
  // dry-run result. The panel never spawns the executor and never writes.
  executorDryRun?: ExecutorDryRunView | null;
  // Tier 3 read-only evidence snapshot view (optional; degrades to a muted hint
  // when absent). Read-only; rendered by the pure tier3EvidenceSnapshot module
  // from an operator-provided a2-tier3-evidence-snapshot.v0. The panel acquires
  // the snapshot as text and obtains it by no spawn; this view adds no control.
  evidenceSnapshot?: EvidenceSnapshotView | null;
}

// Tier 3 Mutation Executor v0 (dry-run) — read-only view. All data is
// pre-computed by the extension from the pure executorDryRun model. The render
// layer only displays it; it adds NO executor spawn, NO worktree-creation
// control, NO write button. The "command" is PRINTED for the operator to run an
// external tool themselves; the dry-run itself creates nothing and writes nothing.
export interface ExecutorDryRunView {
  // The exact external dry-run command the operator would run (printed only).
  printedCommand: string;
  // Pre-formatted dry-run result lines (ready / readiness / plan / scope / steps).
  resultLines: string[];
  // Honest one-line summary.
  summary: string;
  wouldCreateWorktree: boolean; // always false in v0
  wouldWriteFiles: boolean; // always false in v0
  // Pre-formatted dry-run evidence lines (printed-not-run).
  evidenceLines: string[];
}

export function emptyInputs(): PanelInputs {
  return {
    workspace: null,
    plan: null,
    previewBundle: null,
    generatorResult: null,
    approvalResult: null,
    approvalOutput: null,
    applyBundle: null,
    target: null,
    afterSha: null,
  };
}

function escapeHtml(input: string): string {
  return input
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function inputRow(label: string, key: string, value: string | null): string {
  const shown = value === null || value === "" ? "(not set)" : value;
  return `    <tr data-input="${escapeHtml(key)}"><th>${escapeHtml(label)}</th><td data-input-value="${escapeHtml(key)}">${escapeHtml(shown)}</td></tr>`;
}

function buttonHtml(b: PanelButton): string {
  if (b.kind === "ui") {
    return `    <button class="btn ui" data-ui-action="${escapeHtml(b.action)}" data-button-id="${escapeHtml(b.id)}">${escapeHtml(b.label)}</button>`;
  }
  const hb = b as HelperButton;
  return `    <button class="btn helper" data-subcommand="${escapeHtml(hb.subcommand)}" data-button-id="${escapeHtml(hb.id)}">${escapeHtml(hb.label)}</button>`;
}

function outputBlock(output: HelperOutput | null): string {
  if (output === null) {
    return `<section class="output" data-testid="output">
  <h3>Helper output</h3>
  <p class="muted" data-testid="output-empty">No helper subcommand has been run yet. Every action runs one read-only/print helper subcommand.</p>
</section>`;
  }
  const cls = output.exitCode === 0 ? "ok" : "nonzero";
  return `<section class="output ${cls}" data-testid="output">
  <h3>Helper output — <code>${escapeHtml(output.subcommand)}</code> (exit ${escapeHtml(String(output.exitCode))})</h3>
  <button class="btn copy" data-copy-output="true" data-testid="copy-output">Copy helper output</button>
  <pre data-testid="output-stdout">${escapeHtml(output.stdout)}</pre>
${output.stderr.trim().length > 0 ? `  <details><summary>stderr</summary><pre data-testid="output-stderr">${escapeHtml(output.stderr)}</pre></details>` : ""}
</section>`;
}

// The Safety / Stop Gates section is always rendered and never collapsed. It
// states the invariants the panel holds and the conditions under which the
// operator must stop. It is the panel's loud, persistent safety surface.
function safetyBlock(): string {
  return `<section class="safety" data-testid="safety-gates">
  <h3>Safety / Stop Gates (always on)</h3>
  <ul>
    <li>This panel only runs read-only / print helper subcommands. It never runs <code>claw plan run / approve / apply-bundle / apply</code>.</li>
    <li>No Run Preview / Run Approval / Run Apply-Bundle / Run Apply button exists. The panel shows/copies commands; it does not execute them.</li>
    <li>Approval is human, at a REAL terminal. The panel never composes the approval line and never captures it from this webview.</li>
    <li>apply-bundle is the generator (writes no target). <code>claw plan apply</code> is the only command that writes the target, run once, by you, at your terminal.</li>
    <li>No auto-approval. No hidden apply. No model / broker / runtime / :11434 call. No secrets. No filesystem watching or polling.</li>
    <li>STOP if: a preview/approval-result is missing, a hash mismatches, the target drifted, a prior apply marker exists, or an after_file is absolute / unreviewed.</li>
  </ul>
</section>`;
}

// Workspace-first SETUP STATUS section. Always rendered; degrades to a muted
// hint when no status has been computed yet (before the first Refresh). Each
// row carries a data-status attribute for testability.
function statusRow(label: string, value: string): string {
  return `    <tr data-status="${escapeHtml(label)}"><th>${escapeHtml(label)}</th><td data-status-value="${escapeHtml(label)}">${escapeHtml(value)}</td></tr>`;
}

function setupBlock(setup: SetupStatus | null | undefined): string {
  if (!setup) {
    return `<section class="setup" data-testid="setup-status">
  <h3>Workspace status</h3>
  <p class="muted" data-testid="setup-status-empty">Not inspected yet. Open a workspace, then click <strong>Refresh Workspace Status</strong> (read-only).</p>
</section>`;
  }
  const rows = [
    statusRow("helper path", setup.helperPath),
    statusRow("claw binary", setup.clawBinary),
    statusRow("workspace root", setup.workspaceRoot),
    statusRow("plan.yaml", setup.plan),
    statusRow("target", setup.target),
    statusRow("after_sha", setup.afterSha),
    statusRow("preview bundle", setup.previewBundle),
    statusRow("approval result", setup.approvalResult),
    statusRow("apply bundle", setup.applyBundle),
    statusRow("final verification", setup.finalVerification),
  ].join("\n");
  return `<section class="setup" data-testid="setup-status">
  <h3>Workspace status</h3>
  <table>
${rows}
  </table>
  <p class="muted">Read-only, one-shot detection (open + Refresh). claw binary is shown as <code>configured</code>/<code>unknown</code> — the panel never verifies or runs claw.</p>
</section>`;
}

// NEXT SAFE STEP section. Guidance only — it names the recommended safe step and
// (when applicable) the existing safe button to click. It never runs anything.
function navBlock(nav: NavView | null | undefined): string {
  if (!nav) {
    return "";
  }
  const btn = nav.stepButtonId
    ? `  <p data-testid="next-step-button">Recommended button: <code>${escapeHtml(nav.stepButtonId)}</code></p>`
    : "";
  return `<section class="nav" data-testid="next-step">
  <h3>Next safe step</h3>
  <p data-testid="next-step-state">state: <code>${escapeHtml(nav.state)}</code></p>
  <p data-testid="next-step-label">${escapeHtml(nav.stepLabel)}</p>
${btn}
  <p class="muted">This is a read-only recommendation. Print/validate steps print or copy a command; you run preview/approval/apply yourself at a real terminal.</p>
</section>`;
}

// Northstar Phase N2 — WORKSPACE STATUS CARD section. Auto-rendered on open
// (workspace root is detected from the vscode folder). Read-only; git facts may
// render "unknown" with an honest probe note — never green-by-default.
function workspaceCardBlock(card: WorkspaceStatusView | null | undefined): string {
  if (!card) {
    return `<section class="workspace-card" data-testid="workspace-card">
  <h3>Workspace (Northstar)</h3>
  <p class="muted" data-testid="workspace-card-empty">No workspace detected yet. Open a folder; the card auto-detects the workspace root on open.</p>
</section>`;
  }
  const items = card.lines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");
  const note = card.gitProbeNote
    ? `  <p class="muted" data-testid="workspace-card-git-note">Git facts: not-checked — ${escapeHtml(card.gitProbeNote)}</p>`
    : "";
  return `<section class="workspace-card" data-testid="workspace-card">
  <h3>Workspace (Northstar)</h3>
  <ul>
${items}
  </ul>
${note}
  <p class="muted">Read-only, auto-detected on open. The card reads no file and spawns nothing; it never modifies the workspace.</p>
</section>`;
}

// Northstar Phase N2 — STATE-MODEL (ladder) section. Shows the current Northstar
// state and the single recommended next safe step. Read-only guidance only: it
// runs no apply/package/push/pr/merge and auto-advances past no human gate.
function northstarBlock(view: NorthstarLadderView | null | undefined): string {
  if (!view) {
    return "";
  }
  const gate = view.requiresRealTty
    ? `  <p data-testid="northstar-gate">Next gate is a REAL terminal (human-typed); the panel never crosses it for you.</p>`
    : "";
  return `<section class="northstar" data-testid="northstar-state">
  <h3>Northstar state (read-only)</h3>
  <p data-testid="northstar-state-value">state: <code>${escapeHtml(view.state)}</code> <span class="muted">(${escapeHtml(view.stateClass)})</span></p>
  <p data-testid="northstar-next-step">next safe step: ${escapeHtml(view.stepLabel)} <span class="muted">[${escapeHtml(view.stepKind)}; automatable=${view.automatable ? "yes" : "no"}]</span></p>
${gate}
  <p class="muted">Read-only state model. It reflects observed state and recommends one safe step; it never applies, packages, pushes, opens, or merges. Merge is human-only.</p>
</section>`;
}

// DISCOVERY section. Lists every discovered path before it is used.
function discoveryBlock(discovery: DiscoveryView | null | undefined): string {
  if (!discovery || discovery.lines.length === 0) {
    return "";
  }
  const items = discovery.lines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");
  return `<section class="discovery" data-testid="discovery">
  <h3>Discovered (read-only)</h3>
  <ul>
${items}
  </ul>
  <p class="muted">Discovered paths are shown before use. A field is auto-filled only when there is exactly one unambiguous candidate; otherwise you pick it.</p>
</section>`;
}

// EVIDENCE TIMELINE section (read-only, session-local).
function timelineBlock(lines: string[] | null | undefined): string {
  if (!lines || lines.length === 0) {
    return "";
  }
  const items = lines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");
  return `<section class="timeline" data-testid="evidence-timeline">
  <h3>Evidence timeline</h3>
  <ol>
${items}
  </ol>
  <p class="muted">Read-only, session-local. Print steps are recorded as printed, not run. No file is written.</p>
</section>`;
}

// ---- A2 Local Coding Agent Foundation v0 sections (read-only) --------------
//
// These sections are status-only. They add NO action buttons — no agent-run,
// agent-execute, or chain-control button exists here. They make the control
// plane legible: the current permission tier, agent readiness (honest tri-state),
// the denied-command registry vocabulary, the agent evidence ledger, and the
// proposed next agent lane (which, in v0, enables no mutation).

function foundationReadinessBlock(f: FoundationView): string {
  const rows = f.readiness.rows
    .map(
      (r) =>
        `    <tr data-readiness="${escapeHtml(r.label)}"><th>${escapeHtml(r.label)}</th><td data-readiness-value="${escapeHtml(r.label)}">${escapeHtml(r.value)}</td></tr>`,
    )
    .join("\n");
  const warn = f.readiness.dirtyWarning
    ? `  <p class="muted" data-testid="dirty-checkout-warning"><strong>Dirty checkout warning:</strong> the workspace reports uncommitted changes. No mutation lane is enabled in v0; resolve before any future mutation lane.</p>`
    : "";
  const note = f.readiness.gitProbeNote
    ? `  <p class="muted" data-testid="git-probe-note">Git readiness: not-checked — ${escapeHtml(f.readiness.gitProbeNote)}</p>`
    : "";
  return `<section class="agent-readiness" data-testid="agent-readiness">
  <h3>Agent Readiness</h3>
  <table>
${rows}
  </table>
${warn}
${note}
  <p class="muted">Honest tri-state. Git/dirty state is shown as <code>not-checked</code> rather than fabricated when no guard-safe probe is wired (v0).</p>
</section>`;
}

function permissionTierBlock(f: FoundationView): string {
  const rows = f.tiers
    .map((t) => {
      const flags: string[] = [];
      if (t.current) {
        flags.push("current");
      }
      if (t.deniedByDefault) {
        flags.push("denied-by-default");
      }
      if (t.requiresExplicitApproval) {
        flags.push("requires-explicit-approval");
      }
      const flagText = flags.length > 0 ? ` (${flags.join(", ")})` : "";
      return `    <li data-tier="${escapeHtml(String(t.id))}"${t.current ? ' data-current-tier="true"' : ""}><code>Tier ${escapeHtml(String(t.id))}</code> — ${escapeHtml(t.name)}${escapeHtml(flagText)}: ${escapeHtml(t.summary)}</li>`;
    })
    .join("\n");
  return `<section class="permission-tier" data-testid="permission-tier">
  <h3>Permission Tier</h3>
  <p data-testid="current-tier">Current effective tier: <code>Tier ${escapeHtml(String(f.currentTier))}</code> (read-only). Tier 5 (runtime / model / service) is denied by default and external to this cockpit.</p>
  <ul>
${rows}
  </ul>
</section>`;
}

function deniedRegistryBlock(f: FoundationView): string {
  const items = f.deniedFamilies.map((d) => `    <li>${escapeHtml(d)}</li>`).join("\n");
  return `<section class="denied-registry" data-testid="denied-command-registry">
  <h3>Denied Command Registry</h3>
  <p class="muted">These command families are denied globally regardless of tier — denials win over any allowlist.</p>
  <ul>
${items}
  </ul>
</section>`;
}

function agentLedgerBlock(f: FoundationView): string {
  const items = f.ledgerLines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");
  return `<section class="agent-ledger" data-testid="agent-evidence-ledger">
  <h3>Agent Evidence Ledger</h3>
  <ol>
${items}
  </ol>
  <p class="muted">Read-only, session-local. Print-only steps are marked <code>printed-not-run</code>. No file is written.</p>
</section>`;
}

function nextAgentLaneBlock(f: FoundationView): string {
  const blocked = f.nextLane.blocked.map((b) => `    <li>${escapeHtml(b)}</li>`).join("\n");
  return `<section class="next-agent-lane" data-testid="proposed-next-agent-lane">
  <h3>Proposed Next Agent Lane</h3>
  <p data-testid="next-agent-lane-name"><strong>${escapeHtml(f.nextLane.name)}</strong></p>
  <p>${escapeHtml(f.nextLane.summary)}</p>
  <p data-testid="mutation-enabled">Mutation enabled: <code>${f.nextLane.mutationEnabled ? "yes" : "no"}</code> — No mutation lane is enabled in v0. No autonomous source edits, no live A2 chain execution, no PR packaging here.</p>
  <p class="muted">Still blocked in v0:</p>
  <ul>
${blocked}
  </ul>
</section>`;
}

function foundationBlock(foundation: FoundationView | null | undefined): string {
  if (!foundation) {
    return `<section class="foundation" data-testid="agent-foundation">
  <h3>A2 Local Coding Agent Foundation v0</h3>
  <p class="muted" data-testid="agent-foundation-empty">Foundation control plane not computed yet. It renders the current Permission Tier, Agent Readiness, the Denied Command Registry, the Agent Evidence Ledger, and the Proposed Next Agent Lane — all read-only.</p>
</section>`;
  }
  return `<section class="foundation" data-testid="agent-foundation">
  <h3>A2 Local Coding Agent Foundation v0 (read-only)</h3>
${permissionTierBlock(foundation)}
${foundationReadinessBlock(foundation)}
${deniedRegistryBlock(foundation)}
${agentLedgerBlock(foundation)}
${nextAgentLaneBlock(foundation)}
</section>`;
}

// ---- Tier 3 Foundation v0 sections (read-only) -----------------------------
//
// Status-only sections for the disposable worktree mutation path. They add NO
// mutation executor, NO worktree-creation control, and NO write button. They
// make the Tier 3 control plane legible: readiness, the disposable worktree
// plan, the declared touched files, the approval gate, diff/validation
// placeholders, rollback/abandon guidance, and the mutation evidence ledger.

function tier3Block(t: Tier3View | null | undefined): string {
  if (!t) {
    return `<section class="tier3" data-testid="tier3-foundation">
  <h3>Tier 3 — Disposable Worktree Mutation (Foundation v0, read-only)</h3>
  <p class="muted" data-testid="tier3-foundation-empty">Tier 3 control plane not computed yet. It renders Tier 3 Readiness, the Disposable Worktree Plan, Declared Touched Files, the Mutation Approval Gate, Diff Summary, Validation Results, Rollback/Abandon guidance, and the Mutation Evidence Ledger — all read-only. No mutation is enabled in v0.</p>
</section>`;
  }
  const readinessRows = t.readinessRows
    .map(
      (r) =>
        `    <tr data-tier3-readiness="${escapeHtml(r.label)}"><th>${escapeHtml(r.label)}</th><td data-tier3-readiness-value="${escapeHtml(r.label)}">${escapeHtml(r.value)}</td></tr>`,
    )
    .join("\n");
  const dirtyWarn = t.dirtyControlCheckoutBlock
    ? `  <p class="muted" data-testid="tier3-dirty-block"><strong>Blocked:</strong> the control checkout reports uncommitted changes. Tier 3 requires a clean control checkout; resolve before any (future) mutation lane.</p>`
    : "";
  const probe = t.probeNote
    ? `  <p class="muted" data-testid="tier3-probe-note">Tier 3 readiness: not-checked — ${escapeHtml(t.probeNote)}</p>`
    : "";
  const planProblems =
    t.planProblems.length > 0
      ? `  <ul data-testid="tier3-plan-problems">\n${t.planProblems.map((p) => `    <li>${escapeHtml(p)}</li>`).join("\n")}\n  </ul>`
      : "";
  const declared =
    t.declaredPaths.length > 0
      ? t.declaredPaths.map((p) => `    <li>${escapeHtml(p)}</li>`).join("\n")
      : `    <li class="muted">(none declared)</li>`;
  const ledger = t.ledgerLines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");

  return `<section class="tier3" data-testid="tier3-foundation">
  <h3>Tier 3 — Disposable Worktree Mutation (Foundation v0, read-only)</h3>

  <section data-testid="tier3-readiness">
  <h4>Tier 3 Readiness</h4>
  <table>
${readinessRows}
  </table>
  <p data-testid="tier3-overall">Overall: <code>${escapeHtml(t.overall)}</code></p>
${dirtyWarn}
${probe}
  <p class="muted">Honest tri-state. Control-checkout/origin/worktree/branch state is shown as <code>not-checked</code> rather than fabricated when no guard-safe probe is wired (v0).</p>
  </section>

  <section data-testid="tier3-worktree-plan">
  <h4>Disposable Worktree Plan</h4>
  <ul>
${t.planLines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n")}
  </ul>
  <p data-testid="tier3-plan-valid">Plan valid: <code>${t.planValid ? "yes" : "no"}</code> — creation is not performed (plan only in v0).</p>
${planProblems}
  </section>

  <section data-testid="tier3-declared-files">
  <h4>Declared Touched Files</h4>
  <ul>
${declared}
  </ul>
  <p class="muted">Mutation is limited to this exact declared set, inside the disposable worktree. Paths outside it are denied.</p>
  </section>

  <section data-testid="tier3-approval-gate">
  <h4>Mutation Approval Gate</h4>
  <p data-testid="tier3-operator-approved">Operator approved this exact lane: <code>${t.operatorApproved ? "yes" : "no"}</code>.</p>
  <p class="muted">Read-only until the operator explicitly approves the exact lane. No mutation lane is enabled in v0; there is no agent-run / agent-execute / apply / approve control here. ${escapeHtml(t.policyInvariant)}</p>
  </section>

  <section data-testid="tier3-diff-summary">
  <h4>Diff Summary</h4>
  <p class="muted" data-testid="tier3-diff-placeholder">(no diff — no mutation has occurred; a diff summary would be computed inside the disposable worktree and shown before any apply.)</p>
  </section>

  <section data-testid="tier3-validation-results">
  <h4>Validation Results</h4>
  <p class="muted" data-testid="tier3-validation-placeholder">(no validation run — only explicitly-approved validation commands would run inside the disposable worktree.)</p>
  </section>

  <section data-testid="tier3-rollback">
  <h4>Rollback / Abandon Worktree Guidance</h4>
  <p class="muted">Rollback prefers abandoning the disposable worktree (leave it for a separate, safe, non-force cleanup lane). The cockpit never force-removes a worktree and never force-deletes a branch.</p>
  </section>

  <section data-testid="tier3-mutation-ledger">
  <h4>Mutation Evidence Ledger</h4>
  <ol>
${ledger}
  </ol>
  <p class="muted">Read-only, session-local. Checkpoint/print-only steps are marked <code>printed-not-run</code>. No file is written.</p>
  </section>
</section>`;
}

// ---- Tier 3 Mutation Executor v0 (dry-run) section (read-only) -------------
//
// Status-only. It PRINTS the exact external dry-run command (operator-run) and
// renders the dry-run result + evidence. It adds NO executor spawn, NO
// worktree-creation control, and NO write button. The dry-run creates nothing
// and writes nothing.
function executorDryRunBlock(v: ExecutorDryRunView | null | undefined): string {
  if (!v) {
    return `<section class="executor-dryrun" data-testid="executor-dryrun">
  <h3>Proposed Executor Plan (Tier 3 Mutation Executor v0 — dry-run, read-only)</h3>
  <p class="muted" data-testid="executor-dryrun-empty">No approved lane proposed. When one is approved, this section would PRINT the external dry-run command and render what the executor WOULD do — it creates no worktree and writes nothing. The panel never spawns the executor.</p>
</section>`;
  }
  const result = v.resultLines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");
  const evidence = v.evidenceLines.map((l) => `    <li>${escapeHtml(l)}</li>`).join("\n");
  return `<section class="executor-dryrun" data-testid="executor-dryrun">
  <h3>Proposed Executor Plan (Tier 3 Mutation Executor v0 — dry-run, read-only)</h3>
  <p data-testid="executor-dryrun-summary">${escapeHtml(v.summary)}</p>
  <p data-testid="executor-dryrun-would">would create worktree: <code>${v.wouldCreateWorktree ? "yes" : "no"}</code> · would write files: <code>${v.wouldWriteFiles ? "yes" : "no"}</code> — dry-run creates nothing and writes nothing in v0.</p>
  <h4>External dry-run command (operator-run; printed only)</h4>
  <pre data-testid="executor-dryrun-command">${escapeHtml(v.printedCommand)}</pre>
  <h4>Dry-run result</h4>
  <ul data-testid="executor-dryrun-result">
${result}
  </ul>
  <h4>Dry-run evidence</h4>
  <ol data-testid="executor-dryrun-evidence">
${evidence}
  </ol>
  <p class="muted">Read-only. The panel PRINTS the command for you to run an external tool yourself; it never spawns the executor, creates a worktree, or writes a file. Print/checkpoint steps are marked printed-not-run.</p>
</section>`;
}

// ---- Tier 3 read-only evidence snapshot section ---------------------------
//
// Status-only. When a parsed view is present it embeds the pure renderer's
// read-only fragment (renderEvidenceSnapshotHtml — which exposes ZERO controls
// and is fail-closed); when absent it renders a muted placeholder whose body is
// guidance text, NOT a control. The snapshot is acquired as operator-provided
// text; the panel obtains it by no spawn and renders it read-only.
function evidenceSnapshotBlock(view: EvidenceSnapshotView | null | undefined): string {
  if (!view) {
    return `<section class="evidence-snapshot" data-testid="evidence-snapshot">
  <h3>Tier 3 evidence snapshot (read-only)</h3>
  <p class="muted" data-testid="evidence-snapshot-empty">No snapshot provided. Either run <em>A2 Harness: Refresh Tier 3 Evidence Snapshot</em> (read-only — runs the writes-nothing collector through the helper and renders its output here) or run the collector yourself and paste its <code>a2-tier3-evidence-snapshot.v0</code> output (A2 Harness: Paste Tier 3 Evidence Snapshot). This section then renders it read-only and shows no control here.</p>
</section>`;
  }
  // The pure renderer emits the read-only snapshot fragment (zero controls,
  // fail-closed). Append a constant, descriptive refresh affordance — NOT a
  // control — that states the refresh path is read-only: it runs only the
  // writes-nothing collector, so it creates no worktree and writes no file.
  return `${renderEvidenceSnapshotHtml(view)}
<p class="muted" data-testid="evidence-snapshot-refresh-affordance">Refresh path is read-only — would-create-worktree: no, would-write-files: no.</p>`;
}

export function renderHtml(model: RenderModel): string {
  const i = model.inputs;
  const inputRows = [
    inputRow("workspace", "workspace", i.workspace),
    inputRow("plan", "plan", i.plan),
    inputRow("preview-bundle", "preview-bundle", i.previewBundle),
    inputRow("generator-result", "preview-generator-result", i.generatorResult),
    inputRow("approval-result", "approval-result", i.approvalResult),
    inputRow("approval-output", "approval-output", i.approvalOutput),
    inputRow("apply-bundle", "apply-bundle", i.applyBundle),
    inputRow("target", "target", i.target),
    inputRow("after-sha", "after-sha", i.afterSha),
  ].join("\n");

  // Field-setter controls render next to the field table (discoverability);
  // helper + workflow actions render in the Actions section.
  const fieldButtons = fieldSetterButtons().map(buttonHtml).join("\n");
  const actionButtons = [...helperButtons(), ...workflowUiButtons()]
    .map((b) => buttonHtml(b as PanelButton))
    .join("\n");
  const notice = model.notice
    ? `<section class="notice" data-testid="notice"><p>${escapeHtml(model.notice)}</p></section>`
    : "";

  return `<!doctype html>
<html><head><meta charset="utf-8"><title>A2 Harness Panel</title>
<style>
  body { font-family: var(--vscode-font-family, sans-serif); padding: 1rem; }
  h2 { margin-top: 0; }
  .safety { background: var(--vscode-inputValidation-warningBackground, #4d3800); color: var(--vscode-inputValidation-warningForeground, #fff); border: 2px solid var(--vscode-inputValidation-warningBorder, #b89500); padding: 0.75rem 1rem; font-weight: 500; }
  .safety code { font-family: monospace; }
  .inputs table { border-collapse: collapse; }
  .inputs th { text-align: left; padding-right: 1rem; font-family: monospace; }
  .inputs td { font-family: monospace; user-select: all; }
  .output pre { background: var(--vscode-textCodeBlock-background, #1e1e1e); padding: 0.75rem; white-space: pre-wrap; word-break: break-word; }
  .output.nonzero h3 { color: var(--vscode-editorError-foreground, #f48771); }
  .muted { color: var(--vscode-descriptionForeground, #999); }
  .btn { margin: 0.2rem; }
  .actions { margin: 0.5rem 0; }
  .setup table, .inputs table { border-collapse: collapse; }
  .setup th { text-align: left; padding-right: 1rem; font-family: monospace; }
  .setup td { font-family: monospace; }
  .nav { border-left: 3px solid var(--vscode-textLink-foreground, #3794ff); padding-left: 0.75rem; }
</style>
</head><body data-testid="a2-harness-panel">
<h2>A2 Harness Panel</h2>
<p class="muted">Visual driver for the print/validate-only A2 IDE harness v0. Each button runs one read-only/print helper subcommand or copies its printed command — nothing executes the A2 chain.</p>
${safetyBlock()}
${workspaceCardBlock(model.workspaceCard)}
${northstarBlock(model.northstar)}
${setupBlock(model.setup)}
${navBlock(model.nav)}
${discoveryBlock(model.discovery)}
<section class="inputs" data-testid="inputs">
  <h3>Workspace / Plan / Artifact selection</h3>
  <table>
${inputRows}
  </table>
  <div class="field-setters" data-testid="field-setters">
    <p class="muted">Set a field, then run the action that needs it. These controls set fields only — they never run a chain command.</p>
${fieldButtons}
  </div>
</section>
<section class="actions" data-testid="actions">
  <h3>Actions</h3>
${actionButtons}
</section>
${notice}
${outputBlock(model.output)}
${timelineBlock(model.timeline)}
${foundationBlock(model.foundation)}
${tier3Block(model.tier3)}
${executorDryRunBlock(model.executorDryRun)}
${evidenceSnapshotBlock(model.evidenceSnapshot)}
</body></html>`;
}
