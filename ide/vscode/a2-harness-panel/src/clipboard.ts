// Single-field copy helper. The clipboard payload is a verbatim string the
// helper already printed (a command line, an evidence path, or the raw helper
// stdout). No concatenation, no decoration, no terminal-prefixing, no
// shell-quoting changes, and never a composed approval line. The writer is
// injectable so tests assert the exact payload passed to the host clipboard.

export type ClipboardWriter = (payload: string) => Promise<void>;

export type CopyKind = "helper_command" | "evidence_path" | "raw_stdout";

export interface CopyRequest {
  kind: CopyKind;
  payload: string;
}

export function buildCopyRequest(kind: CopyKind, payload: string): CopyRequest {
  if (typeof payload !== "string") {
    throw new Error("clipboard payload must be a string");
  }
  return { kind, payload };
}

export async function copySingleField(
  req: CopyRequest,
  writer: ClipboardWriter,
): Promise<void> {
  await writer(req.payload);
}
