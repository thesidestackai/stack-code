// Agent session model (pure) — A2 Local Coding Agent Foundation v0.
//
// A non-persistent, in-memory description of a cockpit session
// (docs/a2-local-coding-agent-foundation-scope.md §7). This module is PURE: it
// builds and validates a session shape. It holds NO secrets, performs NO IO,
// and is NOT persisted. It introduces no capability — it is the spine the panel
// renders and the evidence ledger references.

import { TierId } from "./permissionTiers";

export type AgentSessionStatus = "ready" | "observing" | "blocked";

export interface AgentSession {
  sessionId: string;
  // Caller-supplied creation marker. NOT fabricated here (no wall-clock is read
  // — consistent with the existing evidence module's no-timestamp discipline).
  createdAt: string | null;
  objective: string | null;
  workspaceRoot: string | null;
  sourceRepo: string | null;
  currentBranch: string | null;
  targetBranch: string | null;
  targetWorktree: string | null;
  touchedSurfaces: string[];
  allowedTier: TierId;
  status: AgentSessionStatus;
  // Pre-formatted, render-ready evidence ledger lines (see agentEvidence.ts).
  evidenceLedger: string[];
}

export interface NewAgentSessionInput {
  sessionId: string;
  createdAt?: string | null;
  objective?: string | null;
  workspaceRoot?: string | null;
  sourceRepo?: string | null;
  currentBranch?: string | null;
  targetBranch?: string | null;
  targetWorktree?: string | null;
  touchedSurfaces?: string[];
  // Defaults to read-only (Tier 1). v0 never defaults to a mutation tier.
  allowedTier?: TierId;
  status?: AgentSessionStatus;
  evidenceLedger?: string[];
}

// Field names that must never appear on a session object — a structural
// guarantee that the session model carries no secret material. The session
// test asserts none of these keys are present.
export const FORBIDDEN_SESSION_KEYS: readonly string[] = [
  "password",
  "token",
  "secret",
  "secrets",
  "apiKey",
  "api_key",
  "bearer",
  "cookie",
  "credential",
  "credentials",
];

export function newAgentSession(input: NewAgentSessionInput): AgentSession {
  if (!input || typeof input.sessionId !== "string" || input.sessionId.trim().length === 0) {
    throw new Error("agent session requires a non-empty sessionId");
  }
  return {
    sessionId: input.sessionId,
    createdAt: input.createdAt ?? null,
    objective: input.objective ?? null,
    workspaceRoot: input.workspaceRoot ?? null,
    sourceRepo: input.sourceRepo ?? null,
    currentBranch: input.currentBranch ?? null,
    targetBranch: input.targetBranch ?? null,
    targetWorktree: input.targetWorktree ?? null,
    touchedSurfaces: input.touchedSurfaces ? [...input.touchedSurfaces] : [],
    // Read-only by default; raising the tier requires an explicit grant.
    allowedTier: typeof input.allowedTier === "number" ? input.allowedTier : 1,
    status: input.status ?? "observing",
    evidenceLedger: input.evidenceLedger ? [...input.evidenceLedger] : [],
  };
}

// True when the object carries no forbidden (secret-like) key. Used by the
// tests as the structural no-secret guarantee.
export function hasNoSecretFields(session: object): boolean {
  if (!session || typeof session !== "object") {
    return true;
  }
  const keys = Object.keys(session as Record<string, unknown>).map((k) => k.toLowerCase());
  for (const forbidden of FORBIDDEN_SESSION_KEYS) {
    if (keys.includes(forbidden.toLowerCase())) {
      return false;
    }
  }
  return true;
}

// Render-ready summary lines for the session (no secrets, never persisted).
export function summarizeSession(session: AgentSession): string[] {
  return [
    "session: " + session.sessionId,
    "objective: " + (session.objective ?? "(not set)"),
    "workspace root: " + (session.workspaceRoot ?? "(not set)"),
    "source repo: " + (session.sourceRepo ?? "(not set)"),
    "current branch: " + (session.currentBranch ?? "(not set)"),
    "target branch: " + (session.targetBranch ?? "(not set)"),
    "target worktree: " + (session.targetWorktree ?? "(none)"),
    "touched surfaces: " + (session.touchedSurfaces.length > 0 ? session.touchedSurfaces.join(", ") : "(none)"),
    "allowed tier: Tier " + String(session.allowedTier),
    "status: " + session.status,
  ];
}
