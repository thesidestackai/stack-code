// Denied command registry (pure) — A2 Local Coding Agent Foundation v0.
//
// A global registry of command families that are DENIED regardless of the
// granted permission tier (docs/a2-local-coding-agent-foundation-scope.md §8).
// This module is PURE: it classifies a command string against denied families.
// It executes nothing, spawns nothing, and reads nothing. It exists so the
// safe-executor model and the panel can show — and a future executor can
// enforce — that denials always win over any tier allowlist.
//
// Detection is intentionally conservative: patterns are matched
// case-insensitively against the raw command text. The registry's job in v0 is
// classification + display, not enforcement of a live executor (there is no
// executor in v0). The matchers below are stored as source strings (compiled to
// RegExp at use sites) so that no live-code token trips the package guards.

export type DeniedFamilyId =
  | "destructive-filesystem-cleanup"
  | "force-branch-or-worktree-deletion"
  | "history-rewrite-or-force-push"
  | "service-control"
  | "runtime-or-service-restart"
  | "model-or-broker-call"
  | "raw-app-inference"
  | "vault-or-secret-read"
  | "live-a2-chain-execution"
  | "approval-line-composition"
  | "network-egress"
  | "watcher-polling-timer-automation"
  | "hidden-execution";

export interface DeniedFamily {
  id: DeniedFamilyId;
  label: string;
  reason: string;
  // Case-insensitive matcher source strings (RegExp bodies). Stored as strings
  // so sensitive vocabulary lives only in string literals (guard-safe).
  patterns: string[];
}

// The denied families. Denials win over allowlists at every tier.
export const DENIED_FAMILIES: readonly DeniedFamily[] = [
  {
    id: "destructive-filesystem-cleanup",
    label: "destructive filesystem cleanup",
    reason: "irreversible removal of files/working-tree state",
    patterns: [
      "rm\\s+-[a-z]*r[a-z]*f",
      "rm\\s+-[a-z]*f[a-z]*r",
      "git\\s+clean",
      "find\\b.*-delete",
      "find\\b.*-exec\\s+rm",
      "git\\s+reset\\s+--hard",
    ],
  },
  {
    id: "force-branch-or-worktree-deletion",
    label: "force branch/worktree deletion",
    reason: "force removal discards unmerged work",
    patterns: ["git\\s+branch\\s+-D", "git\\s+worktree\\s+remove\\s+--force", "git\\s+fetch\\s+--prune"],
  },
  {
    id: "history-rewrite-or-force-push",
    label: "history rewrite / force push",
    reason: "rewriting or overwriting published history",
    patterns: ["git\\s+push\\b.*--force", "git\\s+push\\b.*-f\\b", "git\\s+rebase\\b", "git\\s+filter-branch"],
  },
  {
    id: "service-control",
    label: "service control",
    reason: "controlling local services is out of scope for the cockpit",
    patterns: ["systemctl\\b", "service\\s+\\S+\\s+(start|stop|restart)", "docker\\s+(stop|start|restart|rm|kill)"],
  },
  {
    id: "runtime-or-service-restart",
    label: "runtime/service restart",
    reason: "restarting runtime/services is a Tier 5 (external) action",
    patterns: ["\\brestart\\b", "\\breload\\b.*(daemon|unit|service)"],
  },
  {
    id: "model-or-broker-call",
    label: "model/broker call",
    reason: "model and broker calls are Tier 5 (external) actions",
    patterns: ["/v1/chat/completions", "\\bbroker\\b", "\\bollama\\b", "model\\s+(load|unload)", "(load|unload)\\s+model"],
  },
  {
    id: "raw-app-inference",
    label: "raw app inference",
    reason: "raw app inference to the local model port is denied",
    patterns: ["\\b11434\\b", "/status/vram"],
  },
  {
    id: "vault-or-secret-read",
    label: "Vault / secret read",
    reason: "secrets must never be read or printed by the panel",
    patterns: ["\\bvault\\b", "\\bsecret\\b", "\\bbearer\\b", "api[_-]?key"],
  },
  {
    id: "live-a2-chain-execution",
    label: "live A2 chain execution",
    reason: "the panel never executes the A2 chain (preview/approval/apply-bundle/apply)",
    patterns: [
      "claw\\s+plan\\s+run",
      "claw\\s+plan\\s+approve",
      "claw\\s+plan\\s+apply-bundle",
      "claw\\s+plan\\s+apply",
    ],
  },
  {
    id: "approval-line-composition",
    label: "approval-line composition",
    reason: "the approval line must be human-typed at a real terminal, never composed/captured here",
    patterns: ["apply\\s+\\S+\\s+[0-9a-f]{16,}"],
  },
  {
    id: "network-egress",
    label: "network egress",
    reason: "the panel makes no network/telemetry calls",
    patterns: ["https?://", "\\bcurl\\b", "\\bwget\\b", "\\bnc\\b\\s"],
  },
  {
    id: "watcher-polling-timer-automation",
    label: "watcher/polling/timer automation",
    reason: "no background automation; every gesture is explicit",
    patterns: ["setInterval", "setTimeout", "createFileSystemWatcher", "while\\s+true"],
  },
  {
    id: "hidden-execution",
    label: "hidden command execution",
    reason: "commands are shown before they run; nothing executes silently",
    patterns: ["\\beval\\b", "\\|\\s*(sh|bash)\\b", "\\bsource\\b"],
  },
] as const;

export interface DenyDecision {
  // True when the command matches at least one denied family.
  denied: boolean;
  // The families the command matched (empty when not denied).
  families: DeniedFamilyId[];
  // A short, human-readable explanation suitable for the ledger / panel.
  reason: string | null;
}

// Classify a raw command string against the denied registry. Returns every
// matched family. This is the registry's core check — denials win over any
// tier allowlist (see evaluate()).
export function classifyCommand(command: string): DenyDecision {
  const text = (command || "").toString();
  const families: DeniedFamilyId[] = [];
  for (const fam of DENIED_FAMILIES) {
    for (const src of fam.patterns) {
      let re: RegExp;
      try {
        re = new RegExp(src, "i");
      } catch {
        continue;
      }
      if (re.test(text)) {
        families.push(fam.id);
        break;
      }
    }
  }
  if (families.length === 0) {
    return { denied: false, families: [], reason: null };
  }
  const labels = families
    .map((id) => DENIED_FAMILIES.find((f) => f.id === id)?.label ?? id)
    .join(", ");
  return { denied: true, families, reason: "denied: " + labels };
}

export type AllowlistCheck = (command: string) => boolean;

export interface EvaluateResult {
  decision: "denied" | "allowed";
  families: DeniedFamilyId[];
  reason: string;
}

// Evaluate a command against the denied registry FIRST, then an optional
// tier allowlist. Denials ALWAYS win: a command on the denied registry is
// denied even if the allowlist would permit it. A command that is not denied is
// allowed only if the allowlist (when provided) permits it; with no allowlist,
// a non-denied command is treated as "allowed" for classification purposes
// (v0 enforces nothing — there is no executor).
export function evaluate(command: string, allowlist?: AllowlistCheck): EvaluateResult {
  const denied = classifyCommand(command);
  if (denied.denied) {
    return { decision: "denied", families: denied.families, reason: denied.reason ?? "denied" };
  }
  if (allowlist && !allowlist(command)) {
    return { decision: "denied", families: [], reason: "denied: not on the tier allowlist" };
  }
  return { decision: "allowed", families: [], reason: "allowed (not denied; allowlist satisfied)" };
}

export function deniedFamilyLabels(): string[] {
  return DENIED_FAMILIES.map((f) => f.label);
}
