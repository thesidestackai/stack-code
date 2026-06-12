import * as assert from "assert";
import { renderHtml, emptyInputs, RenderModel } from "../src/render";
import { EVIDENCE_SNAPSHOT_SCHEMA, parseEvidenceSnapshot } from "../src/tier3EvidenceSnapshot";

function baseModel(): RenderModel {
  return { inputs: emptyInputs(), output: null, notice: null };
}

function snapshotJson(): string {
  return JSON.stringify({
    schema_version: EVIDENCE_SNAPSHOT_SCHEMA,
    generated_from: { control_checkout: "/home/suki/stack-code", named_worktree: null },
    tier3_status: "READY_WITH_NOTES",
    fields: {
      last_successful_smoke_at: "2026-06-09",
      canonical_success_worktree: "/wt/canonical",
      last_written_file: "SMOKE_NOTES.md",
      approval_result_path: null,
      apply_bundle_path: null,
      checkpoint_manifest_path: null,
      payload_sha256: "abc123def456payloadsha",
      apply_result_mode: "stdout_only",
      control_checkout_status: "READY",
      partial_smoke_count: 3,
      next_safe_action: "Review evidence",
      blocked_reason: null,
    },
    subjects: [{ subject: "approval gate", status: "DO_NOT_RUN" }],
    links: { closure_doc: "handoffs/closure.md", runbook: "handoffs/runbook.md" },
    caveats: ["stdout-only apply evidence"],
  });
}

describe("evidence snapshot render — read-only integration", () => {
  it("renders the snapshot section when a parsed view is present", () => {
    const view = parseEvidenceSnapshot(snapshotJson());
    const html = renderHtml({ ...baseModel(), evidenceSnapshot: view });
    assert.ok(html.includes('id="tier3-evidence-snapshot"'), "snapshot section missing");
    assert.ok(html.includes("READY_WITH_NOTES"), "tier3 status missing");
    assert.ok(html.includes("abc123def456payloadsha"), "payload sha missing");
    assert.ok(html.includes("Review evidence"), "next safe action missing");
  });

  it("renders a muted placeholder (no control) when no snapshot is provided", () => {
    const html = renderHtml(baseModel());
    assert.ok(html.includes('data-testid="evidence-snapshot"'), "section wrapper missing");
    assert.ok(html.includes('data-testid="evidence-snapshot-empty"'), "placeholder missing");
    const sectionStart = html.indexOf('data-testid="evidence-snapshot"');
    const section = html.slice(sectionStart, sectionStart + 800);
    assert.ok(!/<button/i.test(section), "placeholder must not contain a button");
    assert.ok(!/data-subcommand/i.test(section), "placeholder must not wire a subcommand");
    assert.ok(!/data-ui-action/i.test(section), "placeholder must not wire a ui-action");
  });

  it("FAIL-CLOSED: a bad/mismatched snapshot renders only the unsupported notice", () => {
    const view = parseEvidenceSnapshot('{"schema_version":"a2-tier3-evidence-snapshot.v999"}');
    const html = renderHtml({ ...baseModel(), evidenceSnapshot: view });
    assert.ok(/unsupported snapshot version/i.test(html), "fail-closed notice missing");
    assert.ok(!html.includes("Review evidence"), "no evidence should leak in fail-closed render");
  });

  it("renders NO execution control in the snapshot section", () => {
    const view = parseEvidenceSnapshot(snapshotJson());
    const html = renderHtml({ ...baseModel(), evidenceSnapshot: view });
    const idx = html.indexOf('id="tier3-evidence-snapshot"');
    const section = html.slice(idx, idx + 4000);
    assert.ok(!/<button/i.test(section), "no button in snapshot section");
    assert.ok(!/onclick/i.test(section), "no onclick in snapshot section");
    assert.ok(!/data-subcommand/i.test(section), "no helper subcommand in snapshot section");
    assert.ok(!/data-ui-action/i.test(section), "no ui-action in snapshot section");
    assert.ok(!/postMessage/i.test(section), "no postMessage in snapshot section");
  });

  it("empty placeholder mentions the read-only Refresh command (Option B)", () => {
    const html = renderHtml(baseModel());
    const idx = html.indexOf('data-testid="evidence-snapshot-empty"');
    const section = html.slice(idx, idx + 800);
    assert.ok(/Refresh Tier 3 Evidence Snapshot/i.test(section), "refresh command hint missing");
  });

  it("surfaces a read-only refresh affordance (no worktree, no write) when a view is present", () => {
    const view = parseEvidenceSnapshot(snapshotJson());
    const html = renderHtml({ ...baseModel(), evidenceSnapshot: view });
    const idx = html.indexOf('id="tier3-evidence-snapshot"');
    const section = html.slice(idx, idx + 4000);
    assert.ok(/would-create-worktree:\s*no/i.test(section), "would-create-worktree affordance missing");
    assert.ok(/would-write-files:\s*no/i.test(section), "would-write-files affordance missing");
    // The affordance is descriptive text, never a control.
    assert.ok(!/<button/i.test(section), "affordance must not add a button");
    assert.ok(!/data-subcommand/i.test(section), "affordance must not wire a subcommand");
    assert.ok(!/data-ui-action/i.test(section), "affordance must not wire a ui-action");
  });

  it("keeps the field-setter ordering invariant with the snapshot section present", () => {
    const view = parseEvidenceSnapshot(snapshotJson());
    const html = renderHtml({ ...baseModel(), evidenceSnapshot: view });
    const inputsIdx = html.indexOf('data-testid="inputs"');
    const actionsIdx = html.indexOf('data-testid="actions"');
    const targetIdx = html.indexOf('data-ui-action="selectTarget"');
    assert.ok(targetIdx > inputsIdx && targetIdx < actionsIdx, "field-setter ordering broke");
  });
});
