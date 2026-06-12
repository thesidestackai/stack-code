// Option B read-only refresh — pure outcome mapping.
//
// Given the SpawnResult of a `print-tier3-evidence` helper invocation (which
// runs the read-only, writes-nothing, non-claw collector and prints the
// a2-tier3-evidence-snapshot.v0 JSON to stdout), decide what the panel should
// store as the evidence-snapshot text and what notice (if any) to show.
//
// This module performs NO IO — no fs, no spawn, no network. It only maps a
// result to a fail-closed outcome. The snapshot's own schema validity is judged
// later by the pure tier3EvidenceSnapshot parser; here we only fail closed when
// the helper itself failed (non-zero exit) or produced no output.

export interface HelperResultLike {
  exitCode: number;
  stdout: string;
  stderr: string;
}

export interface RefreshOutcome {
  // Text to store as the evidence snapshot (fed to the existing pure parser).
  // null means "no snapshot" — the section renders its muted placeholder.
  snapshotText: string | null;
  // A user-facing notice, or null on success.
  notice: string | null;
}

function firstLine(s: string): string {
  const line = s.split("\n").find((l) => l.trim().length > 0);
  return line ? line.trim() : "";
}

export function refreshOutcomeFromResult(result: HelperResultLike): RefreshOutcome {
  const stdout = typeof result.stdout === "string" ? result.stdout.trim() : "";
  if (result.exitCode === 0 && stdout.length > 0) {
    return { snapshotText: stdout, notice: null };
  }
  const cause = firstLine(result.stderr || "") || "no snapshot output";
  return {
    snapshotText: null,
    notice: `Tier 3 evidence refresh failed (exit ${result.exitCode}): ${cause}`,
  };
}
