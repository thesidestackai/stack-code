// Read-only discovery parsers for the workspace-first panel layer.
//
// This module is PURE: it parses the verbatim stdout the print/validate-only
// helper already emits (`audit-workspace`, `find-artifacts`, `help`) into
// structured discovery data. It performs NO IO of its own — it never reads a
// file, never spawns a process, never touches `.claw`. The extension wires the
// (already-allowlisted, read-only) helper invocations and passes their stdout
// here. The panel still inspects artifacts ONLY through the helper; this module
// just turns that text into fields the UI can present.
//
// Safety: parsing helper text introduces no new capability. Discovered paths
// are surfaced to the operator before use (extension.ts shows them in the field
// table + a discovery summary); a path is auto-filled only when it is the sole
// unambiguous candidate, never silently inferred.

export type ChainState =
  | "not-started"
  | "preview-ready"
  | "approval-ready"
  | "apply-bundle-ready"
  | "applied"
  | "unknown";

// The .claw artifact names the helper reports, in chain order.
export const ARTIFACT_NAMES = [
  "preview-bundle.json",
  "preview-generator-result.json",
  "approval-result.json",
  "apply-bundle.json",
  "apply-result.json",
] as const;

export type ArtifactName = (typeof ARTIFACT_NAMES)[number];

export interface ArtifactPresence {
  name: string;
  present: boolean;
  // The path the helper resolved (from `present : <name>  (<path>)`), or null.
  path: string | null;
}

export interface TargetHashCheck {
  checked: boolean;
  // true=MATCH, false=MISMATCH, null=not part of this audit.
  match: boolean | null;
}

export interface AuditParse {
  chainState: ChainState;
  artifacts: ArtifactPresence[];
  targetHash: TargetHashCheck;
}

const CHAIN_STATES: readonly ChainState[] = [
  "not-started",
  "preview-ready",
  "approval-ready",
  "apply-bundle-ready",
  "applied",
  "unknown",
];

function asChainState(s: string): ChainState {
  return (CHAIN_STATES as readonly string[]).includes(s) ? (s as ChainState) : "unknown";
}

// Parse `audit-workspace` stdout into chain state + artifact presence map +
// optional target-hash result. Resilient to extra lines; anchors on the exact
// helper line shapes.
export function parseAuditWorkspace(stdout: string): AuditParse {
  const lines = (stdout || "").split(/\r?\n/);

  let chainState: ChainState = "unknown";
  const presenceByName = new Map<string, ArtifactPresence>();
  for (const name of ARTIFACT_NAMES) {
    presenceByName.set(name, { name, present: false, path: null });
  }

  let targetChecked = false;
  let targetMatch: boolean | null = null;

  for (const raw of lines) {
    const line = raw.trimEnd();

    const stateM = line.match(/^\s*chain state:\s*(\S+)\s*$/);
    if (stateM) {
      chainState = asChainState(stateM[1]);
      continue;
    }

    // `  present : <name>  (<path>)`
    const presentM = line.match(/^\s*present\s*:\s*(\S+)\s*\((.+)\)\s*$/);
    if (presentM) {
      const name = presentM[1];
      const p = presentM[2].trim();
      if (presenceByName.has(name)) {
        presenceByName.set(name, { name, present: true, path: p.length > 0 ? p : null });
      }
      continue;
    }

    // `  absent  : <name>`
    const absentM = line.match(/^\s*absent\s*:\s*(\S+)\s*$/);
    if (absentM) {
      const name = absentM[1];
      if (presenceByName.has(name)) {
        presenceByName.set(name, { name, present: false, path: null });
      }
      continue;
    }

    if (/\bMATCH —/.test(line) || /^\s*MATCH\b/.test(line)) {
      targetChecked = true;
      targetMatch = true;
      continue;
    }
    if (/\bMISMATCH —/.test(line) || /^\s*MISMATCH\b/.test(line) || /MISMATCH/.test(line)) {
      targetChecked = true;
      targetMatch = false;
      continue;
    }
  }

  return {
    chainState,
    artifacts: ARTIFACT_NAMES.map((n) => presenceByName.get(n) as ArtifactPresence),
    targetHash: { checked: targetChecked, match: targetMatch },
  };
}

export interface FindArtifactGroup {
  name: string;
  paths: string[];
}

export interface FindParse {
  groups: FindArtifactGroup[];
}

// Parse `find-artifacts` stdout (sections of `## <name>` followed by
// `  path : <path>` lines or `  (none found)`).
export function parseFindArtifacts(stdout: string): FindParse {
  const lines = (stdout || "").split(/\r?\n/);
  const groups: FindArtifactGroup[] = [];
  let current: FindArtifactGroup | null = null;

  for (const raw of lines) {
    const line = raw.trimEnd();
    const headM = line.match(/^\s*##\s+(\S+\.json)\s*$/);
    if (headM) {
      current = { name: headM[1], paths: [] };
      groups.push(current);
      continue;
    }
    const pathM = line.match(/^\s*path\s*:\s*(.+)\s*$/);
    if (pathM && current) {
      const p = pathM[1].trim();
      if (p.length > 0) {
        current.paths.push(p);
      }
    }
  }
  return { groups };
}

// Parse the configured claw path the helper prints in its usage/help output
// (`current: <path>`). This is the CONFIGURED path only — the panel never
// verifies the binary exists (that would need fs/spawn, both forbidden) and
// never runs it. Returns null when no such line is present.
export function parseHelpClawPath(helpStdout: string): string | null {
  const lines = (helpStdout || "").split(/\r?\n/);
  for (const raw of lines) {
    const m = raw.match(/^\s*current:\s*(.+\S)\s*$/);
    if (m) {
      return m[1].trim();
    }
  }
  return null;
}

export type SelectMode = "auto" | "select-needed" | "none";

export interface CandidateSelection {
  mode: SelectMode;
  // The single unambiguous path, only when mode === "auto".
  path: string | null;
  candidates: string[];
}

// Decide whether a set of discovered paths yields an auto-selectable single
// candidate. Exactly one unique candidate => auto. Zero => none. Many =>
// select-needed (operator must pick; never silently inferred).
export function selectCandidate(paths: ReadonlyArray<string | null | undefined>): CandidateSelection {
  const uniq: string[] = [];
  for (const p of paths) {
    if (typeof p === "string") {
      const t = p.trim();
      if (t.length > 0 && !uniq.includes(t)) {
        uniq.push(t);
      }
    }
  }
  if (uniq.length === 0) {
    return { mode: "none", path: null, candidates: [] };
  }
  if (uniq.length === 1) {
    return { mode: "auto", path: uniq[0], candidates: uniq };
  }
  return { mode: "select-needed", path: null, candidates: uniq };
}

// Map an artifact name to the resolved single path the audit reported (if
// present). Used to auto-fill the corresponding input field.
export function auditPathFor(audit: AuditParse | null, name: ArtifactName): string | null {
  if (!audit) {
    return null;
  }
  const hit = audit.artifacts.find((a) => a.name === name);
  return hit && hit.present ? hit.path : null;
}
