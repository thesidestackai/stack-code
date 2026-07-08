// Argv-bounded wrapper around the A2 IDE harness helper (scripts/a2-ide-harness.sh).
// This module is the ONLY place this package spawns any process. It accepts ONLY
// allowlisted subcommands: print/validate subcommands (10 existing) and controlled-
// execution subcommands (package-plan/commit/push/pr, added by N6A).
//
// Print/validate subcommands never execute A2 commands — they print commands for the
// operator to run manually. Controlled-execution subcommands may execute:
//   package-plan:   claw plan run (preview only; writes .claw/ bundle, NOT the target)
//   package-commit: git add <exact declared files> + git commit (no amend, no hooks skip)
//   package-push:   git push <remote> <branch> (non-force only)
//   package-pr:     gh pr create --draft (draft only; no --ready/approve/merge)
//
// This runner never builds `claw plan approve`, `claw plan apply-bundle`, or
// `claw plan apply`. package-plan dispatches claw plan run (preview phase only).
// The spawn implementation is injectable so tests audit argv without touching the OS.
// No shell, no exec/eval; spawn is array-argv only.

// Allowed helper subcommands. Print/validate subcommands (10) never execute A2 commands.
// Controlled-execution subcommands (4, N6A) execute bounded git/gh/claw operations gated
// by N6 runtime sub-tokens. Anything else is refused before spawn.
export const ALLOWED_SUBCOMMANDS = [
  "help",
  "validate-input",
  "print-preview",
  "find-artifacts",
  "print-approval",
  "print-apply-bundle",
  "print-apply",
  "verify-final",
  "audit-workspace",
  // Option B read-only refresh: prints the existing Tier 3 evidence snapshot
  // (a2-tier3-evidence-snapshot.v0) by running the read-only, writes-nothing,
  // non-claw a2-evidence-collector. Still print-only: no target write, no
  // worktree, no claw/model/broker/runtime.
  "print-tier3-evidence",
  // N6A controlled-execution subcommands (require N6 runtime sub-token per rung):
  "package-plan",    // executes claw plan run --workspace-write-preview (preview only; no target write)
  "package-commit",  // executes git add <exact --file list> + git commit (exact-path staging only)
  "package-push",    // executes git push <remote> <branch> (non-force; no --force variant)
  "package-pr",      // executes gh pr create --draft (draft only; no --ready/approve/merge)
] as const;

export type HelperSubcommand = (typeof ALLOWED_SUBCOMMANDS)[number];

// Per-subcommand allowlist of flag names (without the leading `--`). The
// runner refuses any flag not in the subcommand's set, so a button can never
// smuggle an unexpected argument into the helper. String arrays in options
// encode repeated flags (e.g. multiple --file values for package-commit).
export const ALLOWED_FLAGS: Record<HelperSubcommand, readonly string[]> = {
  "help": [],
  "validate-input": ["workspace", "plan"],
  "print-preview": ["workspace", "plan"],
  "find-artifacts": ["workspace"],
  "print-approval": ["workspace", "preview-bundle", "approval-output"],
  "print-apply-bundle": ["preview-generator-result", "approval-result"],
  "print-apply": ["apply-bundle"],
  "verify-final": ["workspace", "target", "after-sha"],
  "audit-workspace": ["workspace", "target", "after-sha"],
  "print-tier3-evidence": ["workspace"],
  // N6A execution subcommands — see docs/stack-code-n6a-helper-exec-allowlist-design.md
  "package-plan":   ["workspace", "plan", "claw-binary"],
  "package-commit": ["workspace", "file", "message"],
  "package-push":   ["workspace", "remote", "branch"],
  "package-pr":     ["workspace", "base", "head", "title", "body-file"],
};

// Chain-write command fragments that must never appear in any caller-supplied
// value or in the helper path. These appear only in the helper's stdout at
// runtime, never in an argument this package builds.
export const CHAIN_WRITE_FRAGMENTS = [
  "claw plan run",
  "claw plan approve",
  "claw plan apply-bundle",
  "claw plan apply",
] as const;

// The required basename of the helper. The runner refuses to spawn any binary
// whose basename is not exactly this, which bounds the spawn to the helper.
export const HELPER_BASENAME = "a2-ide-harness.sh";

export interface SpawnRequest {
  binary: string;
  args: string[];
}

export interface SpawnResult {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export type SpawnImpl = (req: SpawnRequest) => Promise<SpawnResult>;

export interface HelperInvocation {
  helperPath: string;
  subcommand: HelperSubcommand;
  // Flag values keyed by flag name (without `--`). Each must be in the
  // subcommand's ALLOWED_FLAGS set. A string[] value encodes repeated flags
  // (e.g. ["src/a.ts","src/b.ts"] for --file in package-commit).
  options?: Record<string, string | readonly string[]>;
}

export class HelperRunnerRefusal extends Error {
  constructor(message: string) {
    super(message);
    this.name = "HelperRunnerRefusal";
  }
}

function basename(p: string): string {
  const norm = p.replace(/\\/g, "/");
  const parts = norm.split("/");
  return parts[parts.length - 1] ?? "";
}

function refuseIfChainWrite(value: string, field: string): void {
  for (const fragment of CHAIN_WRITE_FRAGMENTS) {
    if (value.includes(fragment)) {
      throw new HelperRunnerRefusal(
        "refused: " + field + " contains chain-write command reference: " + fragment,
      );
    }
  }
}

function isAllowedSubcommand(s: string): s is HelperSubcommand {
  return (ALLOWED_SUBCOMMANDS as readonly string[]).includes(s);
}

// Build the bounded argv for a helper invocation. Throws HelperRunnerRefusal
// on any unapproved subcommand, unapproved flag, flag-shaped value, or
// chain-write-shaped value. Never returns an argv that could execute a
// chain-write command.
export function buildHelperRequest(inv: HelperInvocation): SpawnRequest {
  if (!inv.helperPath || typeof inv.helperPath !== "string") {
    throw new HelperRunnerRefusal("refused: empty or non-string helper path");
  }
  refuseIfChainWrite(inv.helperPath, "helperPath");
  if (inv.helperPath.startsWith("-")) {
    throw new HelperRunnerRefusal("refused: helper path must not begin with '-' (flag shape)");
  }
  if (basename(inv.helperPath) !== HELPER_BASENAME) {
    throw new HelperRunnerRefusal(
      "refused: helper basename must be exactly " + HELPER_BASENAME,
    );
  }

  if (typeof inv.subcommand !== "string" || !isAllowedSubcommand(inv.subcommand)) {
    throw new HelperRunnerRefusal(
      "refused: subcommand is not in the read-only/print allowlist: " + String(inv.subcommand),
    );
  }

  const allowedForSub = ALLOWED_FLAGS[inv.subcommand];
  const args: string[] = [inv.subcommand];
  const options = inv.options ?? {};

  for (const key of Object.keys(options)) {
    if (!allowedForSub.includes(key)) {
      throw new HelperRunnerRefusal(
        "refused: flag --" + key + " is not allowed for subcommand " + inv.subcommand,
      );
    }
    const rawValue = options[key];
    const values: readonly string[] = typeof rawValue === "string" ? [rawValue] : rawValue;
    for (const value of values) {
      if (typeof value !== "string") {
        throw new HelperRunnerRefusal("refused: non-string value for --" + key);
      }
      if (value.startsWith("-")) {
        throw new HelperRunnerRefusal(
          "refused: value for --" + key + " must not begin with '-' (flag shape)",
        );
      }
      refuseIfChainWrite(value, "--" + key);
      args.push("--" + key, value);
    }
  }

  return { binary: inv.helperPath, args };
}

// Run a helper invocation through the injected spawn implementation. The argv
// is built (and validated) first, so a refusal happens before any spawn.
export async function runHelper(
  inv: HelperInvocation,
  spawn: SpawnImpl,
): Promise<SpawnResult> {
  const req = buildHelperRequest(inv);
  return spawn(req);
}

// Default spawn implementation. Spawns the helper directly (it carries its own
// `#!/usr/bin/env bash` shebang and is executable) with array argv and NO
// shell. child_process is imported lazily so unit tests that supply their own
// SpawnImpl never touch the real module.
export function defaultSpawnImpl(): SpawnImpl {
  return async (req) => {
    const cp = await import("child_process");
    return new Promise<SpawnResult>((resolve, reject) => {
      let child;
      try {
        child = cp.spawn(req.binary, req.args, {
          stdio: ["ignore", "pipe", "pipe"],
          shell: false,
        });
      } catch (err) {
        reject(err);
        return;
      }
      let stdout = "";
      let stderr = "";
      child.stdout?.on("data", (chunk: Buffer) => {
        stdout += chunk.toString("utf8");
      });
      child.stderr?.on("data", (chunk: Buffer) => {
        stderr += chunk.toString("utf8");
      });
      child.on("error", (err) => reject(err));
      child.on("close", (code) => {
        resolve({ exitCode: code ?? -1, stdout, stderr });
      });
    });
  };
}
