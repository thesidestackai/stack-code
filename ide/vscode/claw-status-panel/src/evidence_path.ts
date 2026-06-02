import * as path from "path";
import * as fs from "fs";

export type EvidenceLocation = "in-workspace" | "out-of-workspace";

export interface EvidenceClassification {
  raw: string;
  location: EvidenceLocation;
  exists: boolean;
}

function normalizeForPrefixCheck(p: string): string {
  if (p.length > 1 && p.endsWith(path.sep)) {
    return p.slice(0, -1);
  }
  return p;
}

export function classifyEvidencePath(
  rawPath: string,
  workspaceRoot: string,
): EvidenceClassification {
  const wsNorm = normalizeForPrefixCheck(workspaceRoot);
  const absRaw = path.isAbsolute(rawPath)
    ? rawPath
    : path.resolve(wsNorm, rawPath);
  const absRawNorm = normalizeForPrefixCheck(absRaw);

  const isInside =
    absRawNorm === wsNorm ||
    absRawNorm.startsWith(wsNorm + path.sep);

  let exists = false;
  try {
    exists = fs.existsSync(absRawNorm);
  } catch {
    exists = false;
  }

  return {
    raw: rawPath,
    location: isInside ? "in-workspace" : "out-of-workspace",
    exists,
  };
}

export function classifyAll(
  paths: string[],
  workspaceRoot: string,
): EvidenceClassification[] {
  return paths.map((p) => classifyEvidencePath(p, workspaceRoot));
}
