// Northstar Phase N3 — SAFE TARGET BOUNDARY + RISK CLASSIFIER (pure).
//
// Source of truth: docs/stack-code-northstar-ux-phase-n3-task-intake-plan-draft-scope-2026-06-17.md
// §10 (safe target boundary) + §11 (risk classification).
//
// PURE: no fs, no spawn, no network, no watcher, no polling. Operates only over
// declared-path strings the operator typed. Deny-list (forbidden_paths) ALWAYS
// wins over declared_target_paths. STOP / UNKNOWN risk fails closed.

export type RiskCategory =
  | "READ_ONLY"
  | "DOCS_ONLY"
  | "DISPOSABLE_FIXTURE"
  | "SOURCE_EDIT"
  | "RUNTIME_CONFIG"
  | "SECRETS_OR_VAULT"
  | "DESTRUCTIVE_OR_FORCE"
  | "UNKNOWN";

export const RISK_CATEGORIES: readonly RiskCategory[] = [
  "READ_ONLY",
  "DOCS_ONLY",
  "DISPOSABLE_FIXTURE",
  "SOURCE_EDIT",
  "RUNTIME_CONFIG",
  "SECRETS_OR_VAULT",
  "DESTRUCTIVE_OR_FORCE",
  "UNKNOWN",
];

// What a category may do in N3.
//   proceed              — READ_ONLY / DOCS_ONLY may go to draft review.
//   requires-future-lane — DISPOSABLE_FIXTURE / SOURCE_EDIT: draft only in N3.
//   stop                 — RUNTIME_CONFIG / SECRETS_OR_VAULT / DESTRUCTIVE_OR_FORCE / UNKNOWN.
export type RiskDisposition = "proceed" | "requires-future-lane" | "stop";

export function riskDisposition(cat: RiskCategory): RiskDisposition {
  switch (cat) {
    case "READ_ONLY":
    case "DOCS_ONLY":
      return "proceed";
    case "DISPOSABLE_FIXTURE":
    case "SOURCE_EDIT":
      return "requires-future-lane";
    case "RUNTIME_CONFIG":
    case "SECRETS_OR_VAULT":
    case "DESTRUCTIVE_OR_FORCE":
    case "UNKNOWN":
    default:
      return "stop";
  }
}

export function isStopRisk(cat: RiskCategory): boolean {
  return riskDisposition(cat) === "stop";
}

// Path families that are ALWAYS denied — must always appear in forbidden_paths
// and must never be a declared target. Matched as path segments / suffixes.
export const ALWAYS_FORBIDDEN_MARKERS: readonly string[] = [
  "runtime",
  "services",
  "hq",
  "vault",
  "secret",
  "secrets",
  ".env",
];

// Glob / wildcard characters that make a declared path non-exact.
const GLOB_CHARS = ["*", "?", "[", "]", "{", "}"];

export interface PathCheck {
  ok: boolean;
  reason: string | null;
}

function trimmed(p: string | null | undefined): string {
  return typeof p === "string" ? p.trim() : "";
}

// A declared target path must be exact + workspace-relative: non-empty, no glob
// chars, not absolute, not escaping the workspace (no "..").
export function validateDeclaredPath(path: string | null | undefined): PathCheck {
  const p = trimmed(path);
  if (p.length === 0) {
    return { ok: false, reason: "empty path" };
  }
  for (const g of GLOB_CHARS) {
    if (p.includes(g)) {
      return { ok: false, reason: `glob/wildcard char not allowed: ${g}` };
    }
  }
  if (p.startsWith("/") || /^[A-Za-z]:[\\/]/.test(p)) {
    return { ok: false, reason: "absolute path not allowed (workspace-relative only)" };
  }
  const segments = p.split(/[\\/]/);
  if (segments.includes("..")) {
    return { ok: false, reason: "path escapes the workspace (..)" };
  }
  if (p.endsWith("/") || p.endsWith("\\")) {
    return { ok: false, reason: "trailing-slash directory glob not allowed (declare exact files)" };
  }
  return { ok: true, reason: null };
}

// Does a path fall under an always-forbidden family?
export function isForbiddenFamily(path: string | null | undefined): boolean {
  const p = trimmed(path).toLowerCase();
  if (p.length === 0) {
    return false;
  }
  const segments = p.split(/[\\/]/);
  for (const m of ALWAYS_FORBIDDEN_MARKERS) {
    if (segments.includes(m) || p.endsWith("/" + m) || p === m) {
      return true;
    }
  }
  return false;
}

export interface BoundaryCheck {
  ok: boolean;
  problems: string[];
}

// Validate the declared/forbidden boundary set:
//   - every declared path is exact (validateDeclaredPath)
//   - no declared path is under an always-forbidden family
//   - no declared path intersects an explicitly declared forbidden path
//   - the always-forbidden families are all represented in forbidden_paths
export function validateBoundaries(
  declared: ReadonlyArray<string>,
  forbidden: ReadonlyArray<string>,
): BoundaryCheck {
  const problems: string[] = [];
  const declaredClean = declared.map(trimmed).filter((p) => p.length > 0);
  const forbiddenClean = forbidden.map(trimmed).filter((p) => p.length > 0);

  for (const d of declaredClean) {
    const chk = validateDeclaredPath(d);
    if (!chk.ok) {
      problems.push(`declared "${d}": ${chk.reason}`);
    }
    if (isForbiddenFamily(d)) {
      problems.push(`declared "${d}" is in an always-forbidden family (runtime/services/HQ/Vault/secrets)`);
    }
    if (forbiddenClean.some((f) => f === d)) {
      problems.push(`declared "${d}" intersects a forbidden path (deny-list wins)`);
    }
  }

  const lowerForbidden = forbiddenClean.map((f) => f.toLowerCase());
  for (const m of ALWAYS_FORBIDDEN_MARKERS) {
    if (!lowerForbidden.includes(m)) {
      problems.push(`forbidden_paths missing always-denied family: ${m}`);
    }
  }

  return { ok: problems.length === 0, problems };
}

// The default forbidden set N3 always seeds (the always-denied families).
export function defaultForbiddenPaths(): string[] {
  return [...ALWAYS_FORBIDDEN_MARKERS];
}

// Substrings whose presence in intent text marks a destructive/force intent.
const DESTRUCTIVE_MARKERS: readonly string[] = [
  "rm -rf",
  "--force",
  "force push",
  "push --force",
  "git reset --hard",
  "git clean",
  "branch -d",
  "delete branch",
  "drop table",
  "truncate",
];

function anySegmentIncludes(paths: ReadonlyArray<string>, marker: string): boolean {
  return paths.some((p) => p.toLowerCase().split(/[\\/]/).includes(marker));
}

function anyPathMatchesSuffix(paths: ReadonlyArray<string>, suffixes: ReadonlyArray<string>): boolean {
  return paths.some((p) => {
    const lp = p.toLowerCase();
    return suffixes.some((s) => lp.endsWith(s));
  });
}

// Classify risk from the declared paths + the free-text intent. Fail closed:
// anything not positively recognized is UNKNOWN (a STOP). Order matters: the
// most dangerous family wins.
export function classifyRisk(
  declaredPaths: ReadonlyArray<string>,
  intentText: string | null | undefined,
): RiskCategory {
  const paths = declaredPaths.map(trimmed).filter((p) => p.length > 0);
  const intent = trimmed(intentText).toLowerCase();

  // Destructive/force intent wins outright.
  if (DESTRUCTIVE_MARKERS.some((m) => intent.includes(m))) {
    return "DESTRUCTIVE_OR_FORCE";
  }

  // Secrets / Vault.
  if (
    anySegmentIncludes(paths, "vault") ||
    anySegmentIncludes(paths, "secret") ||
    anySegmentIncludes(paths, "secrets") ||
    anyPathMatchesSuffix(paths, [".env"])
  ) {
    return "SECRETS_OR_VAULT";
  }

  // Runtime / services config.
  if (
    anySegmentIncludes(paths, "runtime") ||
    anySegmentIncludes(paths, "services") ||
    anySegmentIncludes(paths, "hq") ||
    anyPathMatchesSuffix(paths, [".service", "dockerfile", "docker-compose.yml", "docker-compose.yaml"])
  ) {
    return "RUNTIME_CONFIG";
  }

  // No declared paths => nothing to touch => read-only.
  if (paths.length === 0) {
    return "READ_ONLY";
  }

  // Docs-only.
  if (paths.every((p) => p.toLowerCase().endsWith(".md") || p.toLowerCase().split(/[\\/]/).includes("docs") || p.toLowerCase().split(/[\\/]/).includes("handoffs"))) {
    return "DOCS_ONLY";
  }

  // Disposable fixture lanes (explicit markers).
  if (paths.every((p) => {
    const lp = p.toLowerCase();
    return lp.split(/[\\/]/).includes("fixture") || lp.split(/[\\/]/).includes("fixtures") || lp.includes("smoke_notes");
  })) {
    return "DISPOSABLE_FIXTURE";
  }

  // Source edits.
  if (anyPathMatchesSuffix(paths, [".ts", ".tsx", ".rs", ".py", ".js", ".jsx", ".sh"])) {
    return "SOURCE_EDIT";
  }

  // Anything else is not positively recognized — fail closed.
  return "UNKNOWN";
}
