import { ParsedStatus, StopReason } from "./parser";
import { EvidenceClassification } from "./evidence_path";

export interface RenderModel {
  isStop: boolean;
  stopReasons: StopReason[];
  schemaVersionDisplay: string;
  workspaceRootDisplay: string;
  runIdDisplay: string;
  stepIdDisplay: string;
  phaseDisplay: string;
  nextOperatorCommandDisplay: string;
  isApprovableDisplay: string;
  isApplyReadyDisplay: string;
  beforeShaDisplay: string;
  afterShaDisplay: string;
  payloadShaDisplay: string;
  liveTargetShaDisplay: string;
  stopConditionDisplay: string;
  evidencePaths: EvidenceClassification[];
  auditMarkersDisplay: string[];
  readOnlyInvariantDisplay: string;
  rawStdout: string;
  exitCodeDisplay: string;
}

const NONE_PLACEHOLDER = "(none)";
const ABSENT_PLACEHOLDER = "(absent)";

function nullable(value: string | null | undefined): string {
  if (value === null || value === undefined) {
    return NONE_PLACEHOLDER;
  }
  return value;
}

function nullableSha(value: string | null | undefined): string {
  if (value === null || value === undefined) {
    return NONE_PLACEHOLDER;
  }
  return value;
}

export function buildRenderModel(
  parsed: ParsedStatus,
  evidence: EvidenceClassification[],
): RenderModel {
  const env = parsed.envelope;
  const observedPhase = parsed.phaseObserved;
  const observedStop = parsed.stopConditionObserved;
  const observedNoc = parsed.nextOperatorCommandObserved;
  const observedSv = parsed.schemaVersionObserved;
  const observedRoi = parsed.readOnlyInvariantObserved;

  return {
    isStop: parsed.isStop,
    stopReasons: parsed.stopReasons,
    schemaVersionDisplay: nullable(observedSv ?? env?.schema_version ?? null),
    workspaceRootDisplay: nullable(env?.workspace_root ?? null),
    runIdDisplay: nullable(env?.run_id ?? null),
    stepIdDisplay: nullable(env?.step_id ?? null),
    phaseDisplay: nullable(observedPhase ?? env?.phase ?? null),
    nextOperatorCommandDisplay: nullable(observedNoc ?? env?.next_operator_command ?? null),
    isApprovableDisplay: env ? String(env.is_approvable) : NONE_PLACEHOLDER,
    isApplyReadyDisplay: env ? String(env.is_apply_ready) : NONE_PLACEHOLDER,
    beforeShaDisplay: nullableSha(env?.before_sha256 ?? null),
    afterShaDisplay: nullableSha(env?.after_sha256 ?? null),
    payloadShaDisplay: nullableSha(env?.payload_sha256 ?? null),
    liveTargetShaDisplay: nullableSha(env?.live_target_sha256 ?? null),
    stopConditionDisplay: observedStop === null || observedStop === undefined
      ? NONE_PLACEHOLDER
      : observedStop,
    evidencePaths: evidence,
    auditMarkersDisplay: env ? [...env.audit_markers] : [],
    readOnlyInvariantDisplay: observedRoi === null
      ? (env?.read_only_invariant === undefined ? ABSENT_PLACEHOLDER : env.read_only_invariant)
      : observedRoi,
    rawStdout: parsed.rawStdout,
    exitCodeDisplay: String(parsed.exitCode),
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

export function renderHtml(model: RenderModel): string {
  const stopClass = model.isStop ? "stop" : "normal";
  const stopBanner = model.isStop
    ? `<section class="stop-banner stop" data-testid="stop-banner">
  <h2>STOP — escalate</h2>
  <ul>
${model.stopReasons
  .map(
    (r) =>
      `    <li data-stop-kind="${escapeHtml(r.kind)}"><span class="kind">${escapeHtml(r.kind)}</span>: <span class="literal">${escapeHtml(r.literal)}</span></li>`,
  )
  .join("\n")}
  </ul>
</section>`
    : `<section class="ok-banner normal" data-testid="ok-banner">
  <h2>Status: OK</h2>
</section>`;

  const fieldsBlock = `<section class="fields ${stopClass}" data-testid="fields">
  <dl>
    <dt>schema_version</dt><dd data-field="schema_version">${escapeHtml(model.schemaVersionDisplay)}</dd>
    <dt>workspace_root</dt><dd data-field="workspace_root">${escapeHtml(model.workspaceRootDisplay)}</dd>
    <dt>run_id</dt><dd data-field="run_id">${escapeHtml(model.runIdDisplay)}</dd>
    <dt>step_id</dt><dd data-field="step_id">${escapeHtml(model.stepIdDisplay)}</dd>
    <dt>phase</dt><dd data-field="phase">${escapeHtml(model.phaseDisplay)}</dd>
    <dt>next_operator_command</dt><dd data-field="next_operator_command" class="copyable" data-copy-target="next_operator_command">${escapeHtml(model.nextOperatorCommandDisplay)}</dd>
    <dt>is_approvable</dt><dd data-field="is_approvable">${escapeHtml(model.isApprovableDisplay)}</dd>
    <dt>is_apply_ready</dt><dd data-field="is_apply_ready">${escapeHtml(model.isApplyReadyDisplay)}</dd>
    <dt>before_sha256</dt><dd data-field="before_sha256">${escapeHtml(model.beforeShaDisplay)}</dd>
    <dt>after_sha256</dt><dd data-field="after_sha256">${escapeHtml(model.afterShaDisplay)}</dd>
    <dt>payload_sha256</dt><dd data-field="payload_sha256">${escapeHtml(model.payloadShaDisplay)}</dd>
    <dt>live_target_sha256</dt><dd data-field="live_target_sha256">${escapeHtml(model.liveTargetShaDisplay)}</dd>
    <dt>stop_condition</dt><dd data-field="stop_condition" class="${model.stopConditionDisplay === "(none)" ? "" : "stop-condition"}">${escapeHtml(model.stopConditionDisplay)}</dd>
    <dt>read_only_invariant</dt><dd data-field="read_only_invariant" class="invariant">${escapeHtml(model.readOnlyInvariantDisplay)}</dd>
  </dl>
</section>`;

  const evidenceItems = model.evidencePaths
    .map((e) => {
      const oow = e.location === "out-of-workspace"
        ? ` <span class="warning" data-testid="out-of-workspace-warning">[out-of-workspace]</span>`
        : "";
      const missing = e.exists === false
        ? ` <span class="missing" data-testid="missing-indicator">[missing]</span>`
        : "";
      return `    <li data-evidence-location="${e.location}" data-evidence-exists="${String(e.exists)}"><a class="evidence-link" data-evidence-path="${escapeHtml(e.raw)}" href="#">${escapeHtml(e.raw)}</a>${oow}${missing} <button class="copy-evidence" data-copy-evidence="${escapeHtml(e.raw)}">copy</button></li>`;
    })
    .join("\n");

  const evidenceBlock = `<section class="evidence ${stopClass}" data-testid="evidence">
  <h3>evidence_paths</h3>
  <ul>
${evidenceItems}
  </ul>
</section>`;

  const markersBlock = `<section class="markers" data-testid="markers">
  <h3>audit_markers</h3>
  <ul>
${model.auditMarkersDisplay
  .map((m) => `    <li data-marker="${escapeHtml(m)}">${escapeHtml(m)}</li>`)
  .join("\n")}
  </ul>
</section>`;

  const rawBlock = `<section class="raw" data-testid="raw">
  <details>
    <summary>raw status JSON (exit ${escapeHtml(model.exitCodeDisplay)})</summary>
    <pre data-testid="raw-stdout">${escapeHtml(model.rawStdout)}</pre>
    <button class="copy-raw" data-copy-raw="true">copy raw envelope</button>
  </details>
</section>`;

  const refreshBlock = `<section class="refresh">
  <button class="refresh-btn" data-action="refresh">Refresh</button>
</section>`;

  return `<!doctype html>
<html><head><meta charset="utf-8"><title>Claw Status</title>
<style>
  body { font-family: var(--vscode-font-family, sans-serif); padding: 1rem; }
  .stop-banner.stop { background: var(--vscode-inputValidation-errorBackground, #5a1d1d); color: var(--vscode-inputValidation-errorForeground, #fff); padding: 1rem; border: 2px solid var(--vscode-inputValidation-errorBorder, #be1100); font-weight: bold; }
  .ok-banner.normal { background: var(--vscode-editor-background, transparent); padding: 1rem; border: 1px solid var(--vscode-panel-border, #444); }
  /* STOP rendering parity: stop banner takes the same or greater vertical
     prominence as the ok banner; never collapsed, never muted. */
  .fields.stop { border-left: 4px solid var(--vscode-inputValidation-errorBorder, #be1100); padding-left: 0.75rem; }
  .invariant { font-family: monospace; }
  .stop-condition { font-weight: bold; }
  .warning { color: var(--vscode-editorWarning-foreground, #f5a623); font-weight: bold; }
  .missing { color: var(--vscode-editorError-foreground, #f48771); font-weight: bold; }
  .copyable { font-family: monospace; user-select: all; }
  button { margin: 0.25rem; }
</style>
</head><body data-stop="${String(model.isStop)}">
${refreshBlock}
${stopBanner}
${fieldsBlock}
${evidenceBlock}
${markersBlock}
${rawBlock}
</body></html>`;
}
