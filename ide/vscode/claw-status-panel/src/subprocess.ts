// Argv-bounded wrapper around `claw plan status`. The subprocess invoker is
// the only place this package spawns any process. It accepts ONLY the two
// A2-L2d positional arguments, NEVER any flags, and NEVER any other binary
// name. The spawn implementation is injectable so tests can audit argv
// without touching the real OS.

import { _CHAIN_WRITE_FRAGMENTS } from "./envelope";

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

export interface ClawStatusInvocation {
  binary: string;
  workspace: string;
  approvalResultPath?: string;
}

export class SubprocessRefusal extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SubprocessRefusal";
  }
}

function refuseIfWriteShape(value: string, field: string): void {
  for (const fragment of _CHAIN_WRITE_FRAGMENTS) {
    if (value.includes(fragment)) {
      throw new SubprocessRefusal(
        "refused: " + field + " contains chain-write subcommand reference: " + fragment,
      );
    }
  }
}

export function buildSpawnRequest(
  inv: ClawStatusInvocation,
): SpawnRequest {
  if (!inv.binary || typeof inv.binary !== "string") {
    throw new SubprocessRefusal("refused: empty or non-string binary");
  }
  if (!inv.workspace || typeof inv.workspace !== "string") {
    throw new SubprocessRefusal("refused: empty or non-string workspace");
  }
  refuseIfWriteShape(inv.binary, "binary");
  refuseIfWriteShape(inv.workspace, "workspace");
  if (inv.approvalResultPath !== undefined) {
    if (typeof inv.approvalResultPath !== "string") {
      throw new SubprocessRefusal(
        "refused: non-string approval-result path",
      );
    }
    refuseIfWriteShape(inv.approvalResultPath, "approvalResultPath");
  }

  // Reject any caller-supplied flag-shaped argument.
  if (inv.workspace.startsWith("-")) {
    throw new SubprocessRefusal(
      "refused: workspace must not begin with '-' (flag shape)",
    );
  }
  if (
    inv.approvalResultPath !== undefined &&
    inv.approvalResultPath.startsWith("-")
  ) {
    throw new SubprocessRefusal(
      "refused: approval-result path must not begin with '-' (flag shape)",
    );
  }

  const args: string[] = ["plan", "status", inv.workspace];
  if (inv.approvalResultPath !== undefined) {
    args.push(inv.approvalResultPath);
  }
  return { binary: inv.binary, args };
}

export async function runClawStatus(
  inv: ClawStatusInvocation,
  spawn: SpawnImpl,
): Promise<SpawnResult> {
  const req = buildSpawnRequest(inv);
  return spawn(req);
}

export function defaultSpawnImpl(): SpawnImpl {
  return async (req) => {
    // Imported lazily so unit tests that supply their own SpawnImpl never
    // touch the real child_process module.
    const cp = await import("child_process");
    return new Promise<SpawnResult>((resolve, reject) => {
      let child;
      try {
        child = cp.spawn(req.binary, req.args, {
          stdio: ["ignore", "pipe", "pipe"],
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
