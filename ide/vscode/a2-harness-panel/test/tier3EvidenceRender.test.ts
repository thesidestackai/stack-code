import * as assert from "assert";
import {
  EVIDENCE_SNAPSHOT_SCHEMA,
  viewFromSnapshot,
  renderEvidenceSnapshotHtml,
} from "../src/tier3EvidenceSnapshot";

function completeSnapshot(): any {
  return {
    schema_version: EVIDENCE_SNAPSHOT_SCHEMA,
    generated_from: { control_checkout: "/home/suki/stack-code", named_worktree: "/wt/canonical" },
    tier3_status: "READY_WITH_NOTES",
    fields: {
      last_successful_smoke_at: "2026-06-09",
      canonical_success_worktree: "/wt/canonical",
      last_written_file: "SMOKE_NOTES.md",
      approval_result_path: "/wt/canonical/.claw/approval-result.json",
      apply_bundle_path: "/wt/canonical/.claw/apply-bundle.json",
      checkpoint_manifest_path: "/wt/canonical/.claw/manifest.json",
      payload_sha256: "cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8",
      apply_result_mode: "stdout_only",
      control_checkout_status: "READY",
      partial_smoke_count: 11,
      next_safe_action: "Review evidence",
      blocked_reason: null,
    },
    subjects: [{ subject: "approval gate", status: "DO_NOT_RUN" }],
    links: { closure_doc: "handoffs/closure.md", runbook: "handoffs/runbook.md" },
    caveats: ["apply-result evidenced on stdout only; no persisted apply-result.json file on this build"],
  };
}

describe("tier3 evidence render — read-only sections", () => {
  it("renders the snapshot section with status, evidence, and caveats", () => {
    const html = renderEvidenceSnapshotHtml(viewFromSnapshot(completeSnapshot()));
    assert.ok(html.includes('id="tier3-evidence-snapshot"'), "section id missing");
    assert.ok(html.includes("READY_WITH_NOTES"), "tier3_status missing");
    assert.ok(
      html.includes("cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8"),
      "payload sha missing",
    );
    assert.ok(/stdout only/i.test(html), "stdout-only caveat missing");
    assert.ok(html.includes("Review evidence"), "next safe action text missing");
  });

  it("renders doc/runbook links as read-only navigation", () => {
    const html = renderEvidenceSnapshotHtml(viewFromSnapshot(completeSnapshot()));
    assert.ok(html.includes("handoffs/closure.md"), "closure link missing");
    assert.ok(html.includes("handoffs/runbook.md"), "runbook link missing");
  });

  it("exposes ZERO execution controls (no button / onclick / command wiring)", () => {
    const html = renderEvidenceSnapshotHtml(viewFromSnapshot(completeSnapshot()));
    assert.ok(!/<button/i.test(html), "must not render any button");
    assert.ok(!/onclick/i.test(html), "must not wire onclick handlers");
    assert.ok(!/command:/i.test(html), "must not wire a command: link");
    assert.ok(!/postMessage/i.test(html), "must not post messages from the snapshot view");
  });

  it("FAIL-CLOSED: an unsupported snapshot renders only the notice, no evidence rows", () => {
    const snap = completeSnapshot();
    snap.schema_version = "a2-tier3-evidence-snapshot.v999";
    const html = renderEvidenceSnapshotHtml(viewFromSnapshot(snap));
    assert.ok(/unsupported snapshot version/i.test(html), "fail-closed notice missing");
    // none of the evidence values leak into the fail-closed render
    assert.ok(
      !html.includes("cde471a929cde57fd6e0b3fd83e304352ebe715a2ef72397a348311373679aa8"),
      "payload sha must not appear in fail-closed render",
    );
    assert.ok(!html.includes("Review evidence"), "next-action must not appear in fail-closed render");
  });

  it("renders a blocked_reason banner for a BLOCKED snapshot (still no controls)", () => {
    const snap = completeSnapshot();
    snap.tier3_status = "BLOCKED";
    snap.fields.blocked_reason = "control checkout dirty";
    snap.fields.next_safe_action = "Do not run — evidence incomplete";
    const html = renderEvidenceSnapshotHtml(viewFromSnapshot(snap));
    assert.ok(html.includes("BLOCKED"), "blocked status missing");
    assert.ok(/control checkout dirty/i.test(html), "blocked reason missing");
    assert.ok(!/<button/i.test(html), "must not render any button even when blocked");
  });

  it("escapes HTML in snapshot values (no raw injection)", () => {
    const snap = completeSnapshot();
    snap.fields.last_written_file = "<img src=x onerror=alert(1)>";
    const html = renderEvidenceSnapshotHtml(viewFromSnapshot(snap));
    assert.ok(!html.includes("<img src=x"), "raw HTML from snapshot must be escaped");
    assert.ok(html.includes("&lt;img"), "value should be HTML-escaped");
  });
});
