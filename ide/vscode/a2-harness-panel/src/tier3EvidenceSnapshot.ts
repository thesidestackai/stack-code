// Tier 3 read-only evidence-snapshot view + renderer (pure, guard-safe).
//
// Renders the `a2-tier3-evidence-snapshot.v0` produced by the read-only
// collector (rust/crates/a2-evidence-collector), per
// docs/a2-tier3-status-panel-scope-card.md. This module is PURE and read-only:
//
//   - The snapshot is the SOLE input. It is passed in as already-acquired text
//     or object; this module performs NO IO — no fs, no process spawn, no
//     network, no watcher, no timer. (The panel guards forbid all of those.)
//   - It NEVER gathers evidence itself and NEVER invokes the collector/claw.
//   - It renders the snapshot read-only and exposes ZERO execution controls
//     (no Run / Apply / Approve / Create Worktree / Cleanup button of any kind).
//   - It is fail-closed: a missing/mismatched schema_version or unparseable
//     input renders a single "unsupported snapshot" notice and nothing else.
//   - Unknown / null values render as "UNKNOWN" (status-like) or "—" (paths/
//     values); readiness is NEVER fabricated.

export const EVIDENCE_SNAPSHOT_SCHEMA = "a2-tier3-evidence-snapshot.v0";

// Placeholders for unobservable values (contract §6 fail-to-UNKNOWN).
export const UNKNOWN = "UNKNOWN";
export const ABSENT = "—";

export interface EvidenceSnapshotFields {
  last_successful_smoke_at: string | null;
  canonical_success_worktree: string | null;
  last_written_file: string | null;
  approval_result_path: string | null;
  apply_bundle_path: string | null;
  checkpoint_manifest_path: string | null;
  payload_sha256: string | null;
  apply_result_mode: string;
  control_checkout_status: string;
  partial_smoke_count: number;
  next_safe_action: string;
  blocked_reason: string | null;
}

export interface SnapshotSubject {
  subject: string;
  status: string;
}

export interface EvidenceSnapshot {
  schema_version: string;
  generated_from: { control_checkout: string; named_worktree: string | null };
  tier3_status: string;
  fields: EvidenceSnapshotFields;
  subjects: SnapshotSubject[];
  links: { closure_doc: string; runbook: string };
  caveats: string[];
}

export interface ViewRow {
  label: string;
  value: string;
}

export interface ViewLink {
  label: string;
  href: string;
}

// Read-only view model the panel renders. When `unsupported` is true the model
// is fail-closed: only `unsupportedReason` is meaningful and the renderer shows
// nothing but that notice.
export interface EvidenceSnapshotView {
  unsupported: boolean;
  unsupportedReason: string | null;
  tier3Status: string;
  rows: ViewRow[];
  subjects: ViewRow[];
  caveats: string[];
  links: ViewLink[];
  nextSafeAction: string;
  blockedReason: string | null;
}

// HTML-escape using split/join only (no regex) so the panel guard's naive
// string-stripper is never confused by quote characters inside a regex literal.
function escapeHtml(value: string): string {
  return value
    .split("&").join("&amp;")
    .split("<").join("&lt;")
    .split(">").join("&gt;")
    .split('"').join("&quot;")
    .split("'").join("&#39;");
}

function unsupportedView(reason: string): EvidenceSnapshotView {
  return {
    unsupported: true,
    unsupportedReason: reason,
    tier3Status: UNKNOWN,
    rows: [],
    subjects: [],
    caveats: [],
    links: [],
    nextSafeAction: "",
    blockedReason: null,
  };
}

// A nullable string path/value renders as ABSENT ("—") when missing.
function pathVal(v: unknown): string {
  return typeof v === "string" && v.length > 0 ? v : ABSENT;
}

// A status-like field renders as UNKNOWN when missing/non-string.
function statusVal(v: unknown): string {
  return typeof v === "string" && v.length > 0 ? v : UNKNOWN;
}

export function viewFromSnapshot(snap: unknown): EvidenceSnapshotView {
  if (snap === null || typeof snap !== "object" || Array.isArray(snap)) {
    return unsupportedView("unsupported snapshot version: (missing)");
  }
  const s = snap as Record<string, unknown>;
  const version = s.schema_version;
  if (version !== EVIDENCE_SNAPSHOT_SCHEMA) {
    const shown = typeof version === "string" ? version : "(missing)";
    return unsupportedView("unsupported snapshot version: " + shown);
  }

  const f = (typeof s.fields === "object" && s.fields !== null
    ? (s.fields as Record<string, unknown>)
    : {}) as Record<string, unknown>;

  const rows: ViewRow[] = [
    { label: "Last proven run", value: pathVal(f.last_successful_smoke_at) },
    { label: "Evidence worktree", value: pathVal(f.canonical_success_worktree) },
    { label: "Written file", value: pathVal(f.last_written_file) },
    { label: "approval-result", value: pathVal(f.approval_result_path) },
    { label: "apply-bundle", value: pathVal(f.apply_bundle_path) },
    { label: "checkpoint manifest", value: pathVal(f.checkpoint_manifest_path) },
    { label: "payload sha256", value: pathVal(f.payload_sha256) },
    { label: "apply-result mode", value: statusVal(f.apply_result_mode) },
    { label: "control checkout", value: statusVal(f.control_checkout_status) },
    {
      label: "partial smoke worktrees",
      value: typeof f.partial_smoke_count === "number" ? String(f.partial_smoke_count) : UNKNOWN,
    },
  ];

  const subjectsRaw = Array.isArray(s.subjects) ? (s.subjects as unknown[]) : [];
  const subjects: ViewRow[] = subjectsRaw
    .filter((x) => x !== null && typeof x === "object")
    .map((x) => {
      const o = x as Record<string, unknown>;
      return {
        label: typeof o.subject === "string" ? o.subject : "(unknown subject)",
        value: statusVal(o.status),
      };
    });

  const caveats: string[] = Array.isArray(s.caveats)
    ? (s.caveats as unknown[]).filter((c): c is string => typeof c === "string")
    : [];

  const linksObj = (typeof s.links === "object" && s.links !== null
    ? (s.links as Record<string, unknown>)
    : {}) as Record<string, unknown>;
  const links: ViewLink[] = [];
  if (typeof linksObj.closure_doc === "string") {
    links.push({ label: "closure doc", href: linksObj.closure_doc });
  }
  if (typeof linksObj.runbook === "string") {
    links.push({ label: "runbook", href: linksObj.runbook });
  }

  return {
    unsupported: false,
    unsupportedReason: null,
    tier3Status: statusVal(s.tier3_status),
    rows,
    subjects,
    caveats,
    links,
    nextSafeAction: typeof f.next_safe_action === "string" ? f.next_safe_action : "",
    blockedReason: typeof f.blocked_reason === "string" ? f.blocked_reason : null,
  };
}

export function parseEvidenceSnapshot(raw: string): EvidenceSnapshotView {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch (_e) {
    return unsupportedView("snapshot is not valid JSON");
  }
  return viewFromSnapshot(parsed);
}

export function renderEvidenceSnapshotHtml(view: EvidenceSnapshotView): string {
  // Fail-closed: render ONLY the notice; no evidence values leak through.
  if (view.unsupported) {
    const reason = escapeHtml(view.unsupportedReason || "unsupported snapshot");
    return (
      '<section id="tier3-evidence-snapshot">' +
      '<p class="notice">' + reason + "</p>" +
      "</section>"
    );
  }

  const parts: string[] = [];
  parts.push('<section id="tier3-evidence-snapshot">');
  parts.push('<h3>Tier 3 evidence (read-only)</h3>');
  parts.push('<p class="status">Tier 3 status: <strong>' + escapeHtml(view.tier3Status) + "</strong></p>");

  if (view.blockedReason !== null) {
    parts.push('<p class="blocked">Blocked: ' + escapeHtml(view.blockedReason) + "</p>");
  }

  parts.push('<ul class="evidence-rows">');
  for (const row of view.rows) {
    parts.push(
      '<li><span class="label">' + escapeHtml(row.label) + '</span>: ' +
      '<span class="value">' + escapeHtml(row.value) + "</span></li>",
    );
  }
  parts.push("</ul>");

  if (view.subjects.length > 0) {
    parts.push('<ul class="subjects">');
    for (const sub of view.subjects) {
      parts.push(
        '<li><span class="label">' + escapeHtml(sub.label) + '</span>: ' +
        '<span class="value">' + escapeHtml(sub.value) + "</span></li>",
      );
    }
    parts.push("</ul>");
  }

  if (view.caveats.length > 0) {
    parts.push('<ul class="caveats">');
    for (const c of view.caveats) {
      parts.push("<li>" + escapeHtml(c) + "</li>");
    }
    parts.push("</ul>");
  }

  if (view.links.length > 0) {
    parts.push('<ul class="links">');
    for (const link of view.links) {
      parts.push(
        '<li><a href="' + escapeHtml(link.href) + '">' + escapeHtml(link.label) + "</a></li>",
      );
    }
    parts.push("</ul>");
  }

  // Next safe action is DISPLAY-ONLY text — never a control.
  parts.push('<p class="next-safe-action">Next safe action: ' + escapeHtml(view.nextSafeAction) + "</p>");
  parts.push("</section>");
  return parts.join("");
}
