// Single-field copy helper. The clipboard payload is the verbatim envelope
// field value; no concatenation, no decoration, no terminal-prefixing, no
// shell-quoting changes. The implementation is injectable so tests assert
// the exact payload string passed to the host clipboard API.

export type ClipboardWriter = (payload: string) => Promise<void>;

export type CopyKind =
  | "next_operator_command"
  | "evidence_path"
  | "raw_envelope";

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
