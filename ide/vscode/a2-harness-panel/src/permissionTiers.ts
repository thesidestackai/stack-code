// Permission tier model (pure) — A2 Local Coding Agent Foundation v0.
//
// Defines the bounded Tier 0–5 control plane from the merged scope
// (docs/a2-local-coding-agent-foundation-scope.md §6). This module is PURE:
// it declares static tier descriptions and classification helpers. It grants no
// capability, spawns nothing, reads nothing, and performs no IO. The panel
// renders these descriptions so the operator can always see the current
// effective tier and what it allows/denies.
//
// Safety invariants encoded here:
//   - Tier 5 (runtime/model/service) is DENIED BY DEFAULT and is external to the
//     cockpit — it is never the effective tier.
//   - Tiers 3 (disposable worktree mutation) and 4 (PR packaging) require
//     explicit future approval gates; they are described, not enabled, by v0.
//   - The default effective tier is read-only (Tier 1 — print commands only),
//     and may only rise to Tier 2 when an already-allowlisted read-only helper
//     call justifies it. No tier here implies hidden command execution.

export type TierId = 0 | 1 | 2 | 3 | 4 | 5;

export interface PermissionTier {
  id: TierId;
  name: string;
  summary: string;
  allowedActions: string[];
  deniedActions: string[];
  requiredGates: string[];
  evidenceRequired: string[];
  // True when this tier is denied by default (Tier 5 only) — never the
  // effective tier of the cockpit.
  deniedByDefault: boolean;
  // True when reaching this tier requires an explicit, separate approval gate
  // that v0 does not grant (Tiers 3 and 4).
  requiresExplicitApproval: boolean;
}

export const PERMISSION_TIERS: readonly PermissionTier[] = [
  {
    id: 0,
    name: "Observe Only",
    summary:
      "Render setup/readiness status, show discovered paths, show the current tier and the evidence ledger.",
    allowedActions: [
      "render setup/readiness status",
      "show discovered paths",
      "show the current permission tier",
      "render the evidence ledger",
    ],
    deniedActions: ["any process spawn", "any file read/write", "any command execution"],
    requiredGates: ["none (default observe state)"],
    evidenceRequired: ["ledger entry for each read-only observation gesture"],
    deniedByDefault: false,
    requiresExplicitApproval: false,
  },
  {
    id: 1,
    name: "Print Commands Only",
    summary:
      "Print the exact command the operator would run, copy it to the clipboard, and print proposed scoped-plan text for human review.",
    allowedActions: [
      "print the exact command the operator would run",
      "copy a printed command to the clipboard",
      "print proposed scoped-plan text for human review",
    ],
    deniedActions: [
      "executing any printed command",
      "spawning claw",
      "any mutation",
    ],
    requiredGates: ["Tier 0 satisfied"],
    evidenceRequired: ["ledger entry recorded as printed-not-run (command text captured)"],
    deniedByDefault: false,
    requiresExplicitApproval: false,
  },
  {
    id: 2,
    name: "Safe Read-Only Execution",
    summary:
      "Run allowlisted read-only/print helper subcommands through the single spawn boundary, plus a read-only repo/git status probe.",
    allowedActions: [
      "run allowlisted read-only/print helper subcommands (validate-input, audit-workspace, find-artifacts, verify-final, help)",
      "read-only repo/git status probe",
    ],
    deniedActions: [
      "any command that writes a target, .claw artifact, or repo file",
      "any model/broker/runtime call",
      "any raw :11434 inference",
    ],
    requiredGates: [
      "Tiers 0-1 satisfied",
      "helper path resolved",
      "argv-bounded wrapper",
    ],
    evidenceRequired: ["ledger entry with subcommand, argv, exit code (read-only)"],
    deniedByDefault: false,
    requiresExplicitApproval: false,
  },
  {
    id: 3,
    name: "Disposable Worktree Mutation",
    summary:
      "Within an isolated, disposable worktree only: scoped file edits to the approved file set and allowlisted local build/test commands.",
    allowedActions: [
      "scoped file edits to the approved file set (disposable worktree only)",
      "allowlisted local build/test commands (disposable worktree only)",
    ],
    deniedActions: [
      "mutation in the control checkout",
      "touching unapproved paths",
      "destructive commands",
      "runtime/model/service actions",
    ],
    requiredGates: [
      "clean control checkout verified",
      "isolated worktree created from origin/main",
      "explicit operator approval of the exact file set and commands",
      "exact-path scoping enforced",
    ],
    evidenceRequired: [
      "checkpoint before mutation",
      "per-command structured evidence",
      "diff summary of the proposed change before any apply",
    ],
    deniedByDefault: false,
    requiresExplicitApproval: true,
  },
  {
    id: 4,
    name: "PR Packaging",
    summary:
      "Stage exact approved paths in the disposable worktree, compose a commit, and open a PR for operator review.",
    allowedActions: [
      "stage exact approved paths in the disposable worktree",
      "compose a commit",
      "open a PR for operator review",
    ],
    deniedActions: [
      "merging",
      "force-push",
      "force-deleting a branch",
      "history rewrite",
      "packaging from a dirty or ambiguous checkout",
    ],
    requiredGates: [
      "Tier 3 satisfied",
      "tests/build green with evidence",
      "diff reviewed",
      "explicit operator approval to package",
    ],
    evidenceRequired: ["commit summary", "PR link", "full diff and test/build evidence in the ledger"],
    deniedByDefault: false,
    requiresExplicitApproval: true,
  },
  {
    id: 5,
    name: "Runtime / Model / Service Actions",
    summary:
      "Out of scope for the agent cockpit and denied by default. Model load/unload, service start/stop/restart, broker calls, and live :11434 inference are external.",
    allowedActions: [],
    deniedActions: [
      "model load/unload",
      "service start/stop/restart",
      "broker calls",
      "live :11434 inference",
    ],
    requiredGates: [
      "explicit token-gated lane outside this cockpit",
      "separate operator authorization",
      "not reachable from the panel",
    ],
    evidenceRequired: ["N/A in this cockpit — Tier 5 is intentionally external"],
    deniedByDefault: true,
    requiresExplicitApproval: true,
  },
] as const;

export function tierById(id: TierId): PermissionTier {
  const hit = PERMISSION_TIERS.find((t) => t.id === id);
  if (!hit) {
    throw new Error("unknown permission tier id: " + String(id));
  }
  return hit;
}

// The cockpit's default effective tier. v0 is print/validate-only, so the
// default is Tier 1 (print commands only). It can rise to Tier 2 ONLY when an
// already-allowlisted read-only helper call has been exercised this session;
// it can never default to a mutation tier.
export function defaultEffectiveTier(readOnlyHelperUsed: boolean): TierId {
  return readOnlyHelperUsed ? 2 : 1;
}

// Invariant guard: the effective tier of the cockpit must be a read-only tier
// (0-2). Tiers 3-5 require explicit approval / are external and must never be
// returned as the live effective tier in v0. Throws on violation; exercised by
// the unit tests.
export function assertEffectiveTierSafe(id: TierId): TierId {
  if (id < 0 || id > 2) {
    throw new Error("unsafe effective tier (mutation/runtime tier not allowed in v0): " + String(id));
  }
  if (tierById(id).deniedByDefault) {
    throw new Error("unsafe effective tier (denied-by-default): " + String(id));
  }
  return id;
}
