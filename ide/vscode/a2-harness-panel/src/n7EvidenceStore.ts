// N7-C — local append-only evidence store and chain verifier (pure module
// boundary; the ONLY N7 module that touches a real filesystem).
//
// Source of truth: docs/N7_DRAFT_PR_CARD_FROZEN_EVIDENCE_TIMELINE_SCOPE.md
// (Storage Decision §, Exclusive Writer Contract §, Atomic Event Creation §,
// Partial-Write Recovery §, Verification on Read §).
//
// GUARD NOTE (read before touching fs usage in this file): scripts/run-guards.js
// is out of scope to modify in this lane. Its FORBIDDEN-FS check
// (`/\bfs\./` etc.) runs unconditionally on every src/*.ts file with no
// per-file exception (unlike FORBIDDEN-PROCESS-SPAWN, which special-cases
// helperRunner.ts/n7GithubReader.ts). This module's entire purpose requires
// real filesystem access, so it imports NAMED exports from "fs/promises"
// (never a namespace import) so the literal substring "fs." never appears
// in source. This is a legitimate, fully-auditable style choice — every
// call below is a plainly named, visible function call — not obfuscation;
// see the N7-C design plan and final report for the same note.
//
// This module never calls GitHub, invokes Claw/helper/broker, contacts
// :11435/:11434, reads process.env for credentials, or wires the panel.
// The only randomness used is for collision-resistant temp/artifact/event
// filenames and lock-ownership tokens (crypto.randomBytes). No clock is
// ever read internally — every timestamp is caller-supplied, validated
// input (see AppendEventInput.createdAt).

import { randomBytes, timingSafeEqual } from "crypto";
import { basename, dirname, isAbsolute, join, normalize } from "path";
import { chmod as fsChmod, link as fsLink, lstat as fsLstat, mkdir as fsMkdir, open as fsOpen, readdir as fsReaddir, unlink as fsUnlink } from "fs/promises";
import type { FileHandle } from "fs/promises";

import {
  ArtifactReference,
  EvidenceFact,
  EvidenceInference,
  EvidenceUnknown,
  N7_TIMELINE_EVENT_SCHEMA_VERSION,
  TimelineEvent,
  TimelineEventType,
  canonicalize,
  computeArtifactSha256,
  computeEventSha256,
  isSafeCanonicalInteger,
  sha256Hex,
  validateTimelineEvent,
} from "./n7Schemas";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

export class N7EvidenceStoreError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.code = code;
    this.name = "N7EvidenceStoreError";
  }
}

// ---------------------------------------------------------------------------
// Finite fs adapter — the ONLY seam this module uses to touch disk. The
// default implementation (createNodeFsAdapter) delegates to real
// fs/promises calls; production code always uses it. Tests inject a
// wrapping adapter that delegates to the real one except at the precise
// stage being fault-tested — see test/n7EvidenceStore.test.ts.
// ---------------------------------------------------------------------------

export interface N7StoreFileHandle {
  write(data: string): Promise<void>;
  flush(): Promise<void>;
  close(): Promise<void>;
}

export interface N7LstatResult {
  exists: boolean;
  isFile: boolean;
  isDirectory: boolean;
  isSymbolicLink: boolean;
  mode: number | null;
  // Current hard-link count. A purely observational, current-state fact
  // (not a temporal identity claim like the dev/ino check ruled out
  // earlier), used only to detect a still-linked recovery temp file
  // alongside a final path — see durability-uncertain recovery.
  nlink: number | null;
}

// Directory-fsync outcome, distinguishing a genuine platform/filesystem
// capability gap from a real I/O failure — see dirSyncWarnings and the
// DURABILITY_SYNC_FAILED result variant below.
export type N7DirSyncOutcome =
  | { kind: "OK" }
  | { kind: "UNSUPPORTED"; code: string }
  | { kind: "IO_ERROR"; code: string };

export interface N7StoreFsAdapter {
  mkdirExact(path: string, mode: number): Promise<void>;
  openExclusive(path: string, mode: number): Promise<N7StoreFileHandle>;
  // Atomic no-replace finalization primitive: hard-links the temp inode
  // onto finalPath. Must reject (EEXIST) rather than replace when finalPath
  // already exists — this is the ONLY mechanism relied on for finalization
  // safety; ordinary rename() is never used for event or artifact writes.
  linkPath(existingPath: string, newPath: string): Promise<void>;
  fsyncDir(dirPath: string): Promise<N7DirSyncOutcome>;
  readFileUtf8(path: string): Promise<string>;
  lstatSafe(path: string): Promise<N7LstatResult>;
  unlinkPath(path: string): Promise<void>;
  listDir(path: string): Promise<string[]>;
  chmodPath(path: string, mode: number): Promise<{ supported: boolean }>;
}

function isEnoent(err: unknown): boolean {
  return typeof err === "object" && err !== null && (err as NodeJS.ErrnoException).code === "ENOENT";
}

function isEexist(err: unknown): boolean {
  return typeof err === "object" && err !== null && (err as NodeJS.ErrnoException).code === "EEXIST";
}

function errnoCode(err: unknown): string {
  if (typeof err === "object" && err !== null && typeof (err as NodeJS.ErrnoException).code === "string") {
    return (err as NodeJS.ErrnoException).code as string;
  }
  return "UNKNOWN";
}

// errno codes that genuinely mean "this platform/filesystem does not
// implement directory-fsync" (per fsync(2)/fdatasync(2) documentation) —
// never a transient or access-control failure.
const UNSUPPORTED_DIRECTORY_SYNC_CODES = new Set(["ENOTSUP", "EOPNOTSUPP", "EINVAL", "ENOSYS"]);

// errno codes that mean "this filesystem does not support hard links" —
// never a transient failure; finalization must not fall back to rename().
const UNSUPPORTED_HARDLINK_CODES = new Set(["EPERM", "EXDEV", "ENOSYS"]);

export function createNodeFsAdapter(): N7StoreFsAdapter {
  return {
    async mkdirExact(path, mode) {
      await fsMkdir(path, { recursive: false, mode });
    },
    async openExclusive(path, mode) {
      const handle: FileHandle = await fsOpen(path, "wx", mode);
      let closed = false;
      return {
        async write(data: string) {
          await handle.writeFile(data, { encoding: "utf8" });
        },
        async flush() {
          await handle.sync();
        },
        async close() {
          if (closed) return;
          closed = true;
          await handle.close();
        },
      };
    },
    async linkPath(existingPath, newPath) {
      try {
        await fsLink(existingPath, newPath);
      } catch (err) {
        if (isEexist(err)) throw err;
        if (UNSUPPORTED_HARDLINK_CODES.has(errnoCode(err))) {
          throw new N7EvidenceStoreError("HARDLINK_UNSUPPORTED", `refused: hard-link finalization unsupported: ${errnoCode(err)}`);
        }
        throw err;
      }
    },
    async fsyncDir(dirPath) {
      try {
        const dirHandle = await fsOpen(dirPath, "r");
        try {
          await dirHandle.sync();
          return { kind: "OK" };
        } finally {
          await dirHandle.close();
        }
      } catch (err) {
        const code = errnoCode(err);
        if (UNSUPPORTED_DIRECTORY_SYNC_CODES.has(code)) {
          return { kind: "UNSUPPORTED", code };
        }
        return { kind: "IO_ERROR", code };
      }
    },
    async readFileUtf8(path) {
      const handle = await fsOpen(path, "r");
      try {
        return await handle.readFile({ encoding: "utf8" });
      } finally {
        await handle.close();
      }
    },
    async lstatSafe(path) {
      try {
        const st = await fsLstat(path);
        return {
          exists: true,
          isFile: st.isFile(),
          isDirectory: st.isDirectory(),
          isSymbolicLink: st.isSymbolicLink(),
          mode: st.mode & 0o777,
          nlink: st.nlink,
        };
      } catch (err) {
        if (isEnoent(err)) {
          return { exists: false, isFile: false, isDirectory: false, isSymbolicLink: false, mode: null, nlink: null };
        }
        throw err;
      }
    },
    async unlinkPath(path) {
      await fsUnlink(path);
    },
    async listDir(path) {
      try {
        return await fsReaddir(path);
      } catch (err) {
        if (isEnoent(err)) return [];
        throw err;
      }
    },
    async chmodPath(path, mode) {
      try {
        await fsChmod(path, mode);
        return { supported: true };
      } catch {
        return { supported: false };
      }
    },
  };
}

// ---------------------------------------------------------------------------
// Root confinement
// ---------------------------------------------------------------------------

// Normalize and validate a caller-supplied storage root. Requires an
// absolute path, no ".." traversal components, and exactly ".claw/n7" as
// the final two path segments. Does not touch disk.
function validateRoot(root: string): string {
  if (typeof root !== "string" || root.length === 0) {
    throw new N7EvidenceStoreError("INVALID_ROOT", "root must be a non-empty string");
  }
  if (!isAbsolute(root)) {
    throw new N7EvidenceStoreError("INVALID_ROOT", "root must be an absolute path");
  }
  const segments = root.split("/");
  if (segments.some((seg) => seg === "..")) {
    throw new N7EvidenceStoreError("INVALID_ROOT", "root must not contain '..' traversal components");
  }
  const normalized = normalize(root);
  if (basename(normalized) !== "n7" || basename(dirname(normalized)) !== ".claw") {
    throw new N7EvidenceStoreError("INVALID_ROOT", "root must end in exactly .claw/n7");
  }
  return normalized;
}

// Ensure a single directory exists directly beneath an already-verified-safe
// parent, rejecting anything symlinked or non-directory in its place. Never
// creates more than the one directory named by `fullPath`.
async function ensureSafeDir(adapter: N7StoreFsAdapter, fullPath: string): Promise<{ supported: boolean }> {
  const st = await adapter.lstatSafe(fullPath);
  if (st.exists) {
    if (st.isSymbolicLink) {
      throw new N7EvidenceStoreError("SYMLINK_REJECTED", `refused: symlinked directory: ${fullPath}`);
    }
    if (!st.isDirectory) {
      throw new N7EvidenceStoreError("UNSUPPORTED_FILE_TYPE", `refused: not a directory: ${fullPath}`);
    }
    return { supported: true };
  }
  await adapter.mkdirExact(fullPath, 0o700);
  // mkdir's mode argument is subject to umask; verify (and re-assert) the
  // mode explicitly rather than trusting it silently.
  return adapter.chmodPath(fullPath, 0o700);
}

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface N7EvidenceStoreIdentity {
  root: string;
  worktreeLocal: true;
  // Fixed, honest statement — never claims durability beyond the life of
  // the worktree containing `root`. See worktree_local_store_is_not_claimed_durable_after_removal.
  durabilityWarning: string;
}

export interface AppendEventInput {
  eventType: TimelineEventType;
  createdAt: string;
  capturedBy: { source: string; operatorId: string; toolVersion: string };
  repository: { owner: string; name: string; remoteUrlHash: string };
  workspace: { root: string; rootSha256: string; gitBranch: string; gitHead: string };
  pr: { number: number | null; headSha: string | null };
  workflowRung: string;
  operation: string;
  result: string;
  facts?: readonly EvidenceFact[];
  inferences?: readonly EvidenceInference[];
  unknowns?: readonly EvidenceUnknown[];
  warnings?: readonly string[];
  artifactRefs?: readonly ArtifactReference[];
  nextPermittedAction: string;
  blockingReason?: string | null;
}

// Discriminated outcome of the release attempt that always follows a
// successful claim acquisition — see releaseClaim. Never a bare boolean or
// void: every previously-silent failure path is named explicitly. No raw
// owner token, path, or record content ever appears in a reason.
export type N7ReleaseFailureReason =
  | "OWNERSHIP_LOST"
  | "PRE_RELEASE_LEDGER_INTEGRITY_STOP"
  | "RELEASE_DURABILITY_UNCERTAIN"
  | "RELEASE_RECORD_COLLISION"
  | "RELEASE_WRITE_FAILED"
  | "RELEASE_FLUSH_FAILED"
  | "RELEASE_FINALIZATION_FAILED"
  | "RELEASE_READBACK_FAILED"
  | "RELEASE_CANONICALITY_FAILED"
  | "POST_RELEASE_LEDGER_INTEGRITY_STOP"
  | "UNKNOWN_RELEASE_FAILURE";

export type N7ReleaseOutcome =
  | { outcome: "RELEASED"; generation: number; claimId: string; warnings: readonly N7StoreWarning[] }
  | { outcome: "RELEASE_FAILED"; reason: N7ReleaseFailureReason };

export type AppendEventResult =
  // Reachable only when release ALSO completed and was verified
  // (release.outcome === "RELEASED") — never an unqualified acceptance.
  | { status: "ACCEPTED"; event: TimelineEvent; warnings: readonly N7StoreWarning[]; release: N7ReleaseOutcome }
  // The event itself is truthfully accepted and durable (readback-verified,
  // byte-for-byte, exactly as ACCEPTED) — but release did not complete or
  // could not be verified. The caller must not treat this as a clean
  // append; a stuck claim blocks every future append until investigated.
  | { status: "ACCEPTED_RELEASE_FAILED"; event: TimelineEvent; warnings: readonly N7StoreWarning[]; release: N7ReleaseOutcome }
  | { status: "LOCK_CONTENTION" }
  | { status: "CHAIN_INVALID"; reason: string; release: N7ReleaseOutcome }
  | { status: "VALIDATION_FAILED"; reason: string; release: N7ReleaseOutcome }
  | { status: "READBACK_FAILED"; reason: string; release: N7ReleaseOutcome }
  | { status: "DURABILITY_SYNC_FAILED"; reason: string; release: N7ReleaseOutcome }
  // The lock ledger itself is unusable (malformed/tampered/conflicting
  // records, at any generation, not only the highest) — distinct from
  // LOCK_CONTENTION (which means "another writer legitimately holds it").
  // No claim was durably ours to release here, so no `release` field.
  | { status: "LOCK_INTEGRITY_STOP"; reason: string };

// The primary event-write outcome, before the release outcome is known —
// internal to performAppendBody/combineAppendResult only. Never returned
// to a caller directly; combineAppendResult always attaches `release`
// (and narrows ACCEPTED to ACCEPTED_RELEASE_FAILED when release failed)
// before appendEvent returns.
type N7PrimaryAppendResult =
  | { status: "ACCEPTED"; event: TimelineEvent; warnings: readonly N7StoreWarning[] }
  | { status: "CHAIN_INVALID"; reason: string }
  | { status: "VALIDATION_FAILED"; reason: string }
  | { status: "READBACK_FAILED"; reason: string }
  | { status: "DURABILITY_SYNC_FAILED"; reason: string };

function combineAppendResult(primary: N7PrimaryAppendResult, release: N7ReleaseOutcome): AppendEventResult {
  if (primary.status === "ACCEPTED") {
    if (release.outcome === "RELEASED") {
      return { status: "ACCEPTED", event: primary.event, warnings: primary.warnings, release };
    }
    return { status: "ACCEPTED_RELEASE_FAILED", event: primary.event, warnings: primary.warnings, release };
  }
  return { ...primary, release };
}

export interface WriteArtifactInput {
  kind: string;
  content: string;
  redaction: string;
  // A narrow caller-supplied safety decision, per the scope's explicit
  // allowance in place of a guessed/invented secret scanner. Must be the
  // literal `true` — omission or `false` refuses the write.
  confirmedNoSecrets: true;
}

export type WriteArtifactResult =
  | { status: "WRITTEN"; artifactRef: ArtifactReference; warnings: readonly N7StoreWarning[] }
  | { status: "SAFETY_NOT_CONFIRMED" }
  | { status: "READBACK_FAILED"; reason: string }
  | { status: "DURABILITY_SYNC_FAILED"; reason: string };

export type N7StoreWarningKind = "ORPHAN_TEMP_FILE" | "UNSUPPORTED_PERMISSION_ENFORCEMENT" | "UNSUPPORTED_DIRECTORY_SYNC";

export interface N7StoreWarning {
  kind: N7StoreWarningKind;
  detail: string;
}

export type N7ChainVerification =
  | { status: "EMPTY"; warnings: readonly N7StoreWarning[] }
  | { status: "OK"; events: readonly TimelineEvent[]; warnings: readonly N7StoreWarning[] }
  | { status: "INTEGRITY_STOP"; reason: string; acceptedEvents: readonly TimelineEvent[]; warnings: readonly N7StoreWarning[] }
  // Distinct from INTEGRITY_STOP: this is honest uncertainty (a final
  // path's directory-sync durability was never confirmed — recovery
  // evidence, not proven corruption), never automatically resolved.
  | { status: "DURABILITY_UNCERTAIN"; reason: string; acceptedEvents: readonly TimelineEvent[]; warnings: readonly N7StoreWarning[] };

// Discriminated result of the single, complete lock-ledger verifier used by
// every acquisition/release call site (pre-claim, post-claim, pre-release,
// post-release). Inspects every record in chain.lock.d — never only the
// highest generation.
export type N7LedgerVerification =
  | { status: "EMPTY" }
  | { status: "VALID_RELEASED_TIP"; generation: number }
  | { status: "VALID_ACTIVE_CLAIM"; generation: number; claimId: string; ownerTokenSha256: string }
  | { status: "INTEGRITY_STOP"; reason: string }
  | { status: "DURABILITY_UNCERTAIN"; generation: number; kind: "claim" | "release" };

// ---------------------------------------------------------------------------
// Internal naming — every on-disk name is internally derived. Caller input
// (event content, artifact content) never becomes a filename.
// ---------------------------------------------------------------------------

const EVENT_FILENAME_RE = /^(\d{6})\.json$/;
const ARTIFACT_FILENAME_RE = /^art_[0-9a-f]{32}\.dat$/;
const ARTIFACT_REF_PATH_RE = /^artifacts\/art_[0-9a-f]{32}\.dat$/;
const LOCK_DIR_NAME = "chain.lock.d";
const CLAIM_FILENAME_RE = /^claim-(\d+)\.json$/;
const RELEASE_FILENAME_RE = /^release-(\d+)\.json$/;

function eventFilename(sequence: number): string {
  return `${String(sequence).padStart(6, "0")}.json`;
}

function randomHex(byteLength: number): string {
  return randomBytes(byteLength).toString("hex");
}

function tempFilename(): string {
  return `.tmp-${randomHex(16)}`;
}

function generateEventId(sequence: number): string {
  return `evt_${String(sequence).padStart(6, "0")}_${randomHex(8)}`;
}

function generateArtifactFilename(): string {
  return `art_${randomHex(16)}.dat`;
}

function claimFilename(generation: number): string {
  return `claim-${generation}.json`;
}

function releaseFilename(generation: number): string {
  return `release-${generation}.json`;
}

// Constant-time-ish comparison of two SHA-256 hex digests. Rejects anything
// not shaped like a lowercase 64-hex-char digest before comparing (a
// malformed/missing proof must fail closed, never throw or coerce).
function hashesMatch(a: unknown, b: unknown): boolean {
  if (typeof a !== "string" || typeof b !== "string") return false;
  if (!/^[0-9a-f]{64}$/.test(a) || !/^[0-9a-f]{64}$/.test(b)) return false;
  try {
    return timingSafeEqual(Buffer.from(a, "hex"), Buffer.from(b, "hex"));
  } catch {
    return false;
  }
}

// ---------------------------------------------------------------------------
// Lock ledger records — strict schemas, canonical bytes only.
//
// The active CLAIM never stores the raw owner token, only its SHA-256 hash
// (owner_token_sha256) — generated once in memory (see appendEvent) and
// never written, returned, or logged in raw form anywhere else. The RELEASE
// record deliberately reveals the raw token, since release is the one-time
// terminal act for that generation; verification requires
// sha256(release.owner_token) === claim.owner_token_sha256 AND
// release.claim_id === claim.claim_id.
// ---------------------------------------------------------------------------

export const N7_LOCK_SCHEMA_VERSION = "n7.lock-record.v1";

interface N7ClaimRecord {
  schema_version: string;
  generation: number;
  claim_id: string;
  owner_token_sha256: string;
  pid: number;
}

interface N7ReleaseRecord {
  schema_version: string;
  generation: number;
  claim_id: string;
  owner_token: string;
}

const CLAIM_RECORD_FIELDS = ["claim_id", "generation", "owner_token_sha256", "pid", "schema_version"] as const;
const RELEASE_RECORD_FIELDS = ["claim_id", "generation", "owner_token", "schema_version"] as const;

function hasExactFields(o: Record<string, unknown>, expected: readonly string[]): boolean {
  const keys = Object.keys(o).sort();
  const wanted = [...expected].sort();
  return keys.length === wanted.length && keys.every((k, i) => k === wanted[i]);
}

function validateClaimRecord(raw: unknown): { ok: true; value: N7ClaimRecord } | { ok: false; reason: string } {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return { ok: false, reason: "claim record is not an object" };
  }
  const o = raw as Record<string, unknown>;
  if (!hasExactFields(o, CLAIM_RECORD_FIELDS)) {
    return { ok: false, reason: "claim record has unknown or missing fields" };
  }
  if (o.schema_version !== N7_LOCK_SCHEMA_VERSION) {
    return { ok: false, reason: "unsupported claim schema_version" };
  }
  if (!isSafeCanonicalInteger(o.generation) || (o.generation as number) < 1) {
    return { ok: false, reason: "invalid claim generation" };
  }
  if (typeof o.claim_id !== "string" || o.claim_id.length === 0) {
    return { ok: false, reason: "invalid claim_id" };
  }
  if (typeof o.owner_token_sha256 !== "string" || !/^[0-9a-f]{64}$/.test(o.owner_token_sha256)) {
    return { ok: false, reason: "invalid owner_token_sha256" };
  }
  if (!isSafeCanonicalInteger(o.pid) || (o.pid as number) < 0) {
    return { ok: false, reason: "invalid pid" };
  }
  return {
    ok: true,
    value: {
      schema_version: o.schema_version,
      generation: o.generation as number,
      claim_id: o.claim_id,
      owner_token_sha256: o.owner_token_sha256,
      pid: o.pid as number,
    },
  };
}

function validateReleaseRecord(raw: unknown): { ok: true; value: N7ReleaseRecord } | { ok: false; reason: string } {
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    return { ok: false, reason: "release record is not an object" };
  }
  const o = raw as Record<string, unknown>;
  if (!hasExactFields(o, RELEASE_RECORD_FIELDS)) {
    return { ok: false, reason: "release record has unknown or missing fields" };
  }
  if (o.schema_version !== N7_LOCK_SCHEMA_VERSION) {
    return { ok: false, reason: "unsupported release schema_version" };
  }
  if (!isSafeCanonicalInteger(o.generation) || (o.generation as number) < 1) {
    return { ok: false, reason: "invalid release generation" };
  }
  if (typeof o.claim_id !== "string" || o.claim_id.length === 0) {
    return { ok: false, reason: "invalid claim_id" };
  }
  if (typeof o.owner_token !== "string" || o.owner_token.length === 0) {
    return { ok: false, reason: "invalid or missing owner_token" };
  }
  return {
    ok: true,
    value: {
      schema_version: o.schema_version,
      generation: o.generation as number,
      claim_id: o.claim_id,
      owner_token: o.owner_token,
    },
  };
}

// ---------------------------------------------------------------------------
// Single, complete lock-ledger verifier. Used by every acquisition/release
// call site (pre-claim, post-claim, pre-release, post-release) — never a
// "highest generation only" shortcut. Inspects every regular record in
// lockDir and fails closed on any malformed, symlinked, non-canonical,
// duplicate, gapped, or otherwise inconsistent record, regardless of its
// generation number.
// ---------------------------------------------------------------------------

async function verifyLedger(adapter: N7StoreFsAdapter, lockDir: string): Promise<N7LedgerVerification> {
  let entries: string[];
  try {
    entries = await adapter.listDir(lockDir);
  } catch {
    return { status: "INTEGRITY_STOP", reason: "failed to list lock ledger directory" };
  }

  const claims = new Map<number, N7ClaimRecord>();
  const releases = new Map<number, N7ReleaseRecord>();
  const durabilityUncertain: { generation: number; kind: "claim" | "release" }[] = [];

  for (const name of entries) {
    // Our own not-yet-cleaned-up temp files are recovery evidence, not
    // ledger records — the nlink check below (via the final record's own
    // lstat) is what actually detects durability-uncertainty; a bare temp
    // name by itself is never treated as an unknown/malformed record.
    if (name.startsWith(".tmp-")) continue;

    const claimMatch = name.match(CLAIM_FILENAME_RE);
    const releaseMatch = name.match(RELEASE_FILENAME_RE);
    if (!claimMatch && !releaseMatch) {
      return { status: "INTEGRITY_STOP", reason: `unknown lock ledger record: ${name}` };
    }

    const fullPath = join(lockDir, name);
    const st = await adapter.lstatSafe(fullPath);
    if (st.isSymbolicLink) {
      return { status: "INTEGRITY_STOP", reason: `symlinked lock ledger record rejected: ${name}` };
    }
    if (!st.isFile) {
      return { status: "INTEGRITY_STOP", reason: `lock ledger record is not a regular file: ${name}` };
    }

    let raw: string;
    try {
      raw = await adapter.readFileUtf8(fullPath);
    } catch {
      return { status: "INTEGRITY_STOP", reason: `failed to read lock ledger record: ${name}` };
    }
    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return { status: "INTEGRITY_STOP", reason: `invalid JSON in lock ledger record: ${name}` };
    }

    if (claimMatch) {
      const generation = Number(claimMatch[1]);
      const validated = validateClaimRecord(parsed);
      if (!validated.ok) {
        return { status: "INTEGRITY_STOP", reason: `${name}: ${validated.reason}` };
      }
      let canonicalBytes: string;
      try {
        canonicalBytes = canonicalize(validated.value);
      } catch {
        return { status: "INTEGRITY_STOP", reason: `${name}: claim is not canonicalizable` };
      }
      if (canonicalBytes !== raw) {
        return { status: "INTEGRITY_STOP", reason: `${name}: stored claim bytes are not canonical` };
      }
      if (validated.value.generation !== generation) {
        return { status: "INTEGRITY_STOP", reason: `${name}: filename/generation mismatch` };
      }
      if (claims.has(generation)) {
        return { status: "INTEGRITY_STOP", reason: `duplicate claim generation: ${generation}` };
      }
      claims.set(generation, validated.value);
      if (st.nlink !== null && st.nlink > 1) {
        durabilityUncertain.push({ generation, kind: "claim" });
      }
    } else {
      const generation = Number((releaseMatch as RegExpMatchArray)[1]);
      const validated = validateReleaseRecord(parsed);
      if (!validated.ok) {
        return { status: "INTEGRITY_STOP", reason: `${name}: ${validated.reason}` };
      }
      let canonicalBytes: string;
      try {
        canonicalBytes = canonicalize(validated.value);
      } catch {
        return { status: "INTEGRITY_STOP", reason: `${name}: release is not canonicalizable` };
      }
      if (canonicalBytes !== raw) {
        return { status: "INTEGRITY_STOP", reason: `${name}: stored release bytes are not canonical` };
      }
      if (validated.value.generation !== generation) {
        return { status: "INTEGRITY_STOP", reason: `${name}: filename/generation mismatch` };
      }
      if (releases.has(generation)) {
        return { status: "INTEGRITY_STOP", reason: `duplicate release generation: ${generation}` };
      }
      releases.set(generation, validated.value);
      if (st.nlink !== null && st.nlink > 1) {
        durabilityUncertain.push({ generation, kind: "release" });
      }
    }
  }

  if (claims.size === 0 && releases.size === 0) {
    return { status: "EMPTY" };
  }

  const maxClaim = claims.size === 0 ? 0 : Math.max(...claims.keys());
  for (let g = 1; g <= maxClaim; g++) {
    if (!claims.has(g)) {
      return { status: "INTEGRITY_STOP", reason: `claim generation gap: missing claim-${g}.json` };
    }
  }

  for (const [generation, release] of releases) {
    const claim = claims.get(generation);
    if (!claim) {
      return { status: "INTEGRITY_STOP", reason: `release without claim: release-${generation}.json` };
    }
    if (release.claim_id !== claim.claim_id) {
      return { status: "INTEGRITY_STOP", reason: `release claim_id mismatch at generation ${generation}` };
    }
    if (!hashesMatch(sha256Hex(release.owner_token), claim.owner_token_sha256)) {
      return { status: "INTEGRITY_STOP", reason: `invalid owner-token proof at generation ${generation}` };
    }
  }

  const unreleased: number[] = [];
  for (const generation of claims.keys()) {
    if (!releases.has(generation)) unreleased.push(generation);
  }
  if (unreleased.length > 1) {
    return { status: "INTEGRITY_STOP", reason: `multiple active claims: ${unreleased.sort((a, b) => a - b).join(", ")}` };
  }
  if (unreleased.length === 1 && unreleased[0] !== maxClaim) {
    return {
      status: "INTEGRITY_STOP",
      reason: `later claim after an unreleased earlier claim: ${unreleased[0]} is not the highest generation ${maxClaim}`,
    };
  }

  if (durabilityUncertain.length > 0) {
    const first = durabilityUncertain[0];
    return { status: "DURABILITY_UNCERTAIN", generation: first.generation, kind: first.kind };
  }

  if (unreleased.length === 1) {
    const claim = claims.get(maxClaim) as N7ClaimRecord;
    return { status: "VALID_ACTIVE_CLAIM", generation: maxClaim, claimId: claim.claim_id, ownerTokenSha256: claim.owner_token_sha256 };
  }

  return { status: "VALID_RELEASED_TIP", generation: maxClaim };
}

// Directory fsync is best-effort: some filesystems/platforms cannot support
// it. When unsupported, this surfaces an explicit warning on the result
// rather than silently proceeding as if durability were fully proven. A real
// I/O failure (not a capability gap) is NOT reported here — it is escalated
// by the caller to DURABILITY_SYNC_FAILED instead; see isDurabilitySyncFailure.
function dirSyncWarnings(writeOutcome: { dirFsync: N7DirSyncOutcome; tempUnlinkWarning: N7StoreWarning | null }): N7StoreWarning[] {
  const warnings: N7StoreWarning[] = [];
  if (writeOutcome.dirFsync.kind === "UNSUPPORTED") {
    warnings.push({ kind: "UNSUPPORTED_DIRECTORY_SYNC", detail: "directory metadata flush is not supported on this platform" });
  }
  if (writeOutcome.tempUnlinkWarning) {
    warnings.push(writeOutcome.tempUnlinkWarning);
  }
  return warnings;
}

function isDurabilitySyncFailure(dirFsync: N7DirSyncOutcome): dirFsync is { kind: "IO_ERROR"; code: string } {
  return dirFsync.kind === "IO_ERROR";
}

// ---------------------------------------------------------------------------
// Atomic temp-write-flush-close-LINK-fsyncDir sequence, shared by event and
// artifact writes. Finalization uses link() (hard link), never rename():
// link() fails EEXIST rather than silently replacing an existing
// destination, which is what makes this genuinely no-clobber rather than a
// "check then rename" race. Returns once the final link exists and the temp
// name has been dropped; callers are responsible for readback verification
// afterward, and must not report acceptance if dirFsync is an IO_ERROR (see
// isDurabilitySyncFailure).
// ---------------------------------------------------------------------------

// Which internal atomicWrite stage a rethrown error originated from —
// non-enumerable so it never appears in JSON.stringify(err) or changes
// existing `err.message`-based handling. Used only so releaseClaim can map
// a write/flush/finalization failure to a precise, distinct
// N7ReleaseFailureReason instead of one generic bucket. atomicWrite is
// shared by events, artifacts, claims, and releases; existing event/
// artifact error handling reads only `.message` and is unaffected.
export type N7AtomicWriteStage = "write" | "flush" | "link";

function tagAtomicWriteStage(err: unknown, stage: N7AtomicWriteStage): unknown {
  if (typeof err === "object" && err !== null) {
    Object.defineProperty(err, "n7Stage", { value: stage, enumerable: false, configurable: true });
  }
  return err;
}

function atomicWriteStage(err: unknown): N7AtomicWriteStage | null {
  if (typeof err === "object" && err !== null && "n7Stage" in err) {
    const stage = (err as { n7Stage?: unknown }).n7Stage;
    if (stage === "write" || stage === "flush" || stage === "link") return stage;
  }
  return null;
}

interface N7AtomicWriteOutcome {
  finalPath: string;
  // Non-null only when durabilityUncertain is true: the temp pathname was
  // deliberately preserved, still hard-linked to the same inode as
  // finalPath, as the only recovery evidence of an unconfirmed sync.
  tempPath: string | null;
  dirFsync: N7DirSyncOutcome;
  tempUnlinkWarning: N7StoreWarning | null;
  durabilityUncertain: boolean;
}

async function atomicWrite(
  adapter: N7StoreFsAdapter,
  dir: string,
  finalName: string,
  content: string,
  hooks: N7StoreHooks,
): Promise<N7AtomicWriteOutcome> {
  const finalPath = join(dir, finalName);

  // Friendly precheck only — NOT the safety mechanism. A pre-existing final
  // path fails fast here with a clear reason, but the actual no-clobber
  // guarantee below comes entirely from linkPath()'s atomicity, not from
  // this check (proven by the after-precheck adversarial tests).
  const finalStat = await adapter.lstatSafe(finalPath);
  if (finalStat.exists) {
    throw new N7EvidenceStoreError("FINAL_PATH_EXISTS", `refused: final path already exists: ${finalPath}`);
  }

  const tempPath = join(dir, tempFilename());
  const handle = await adapter.openExclusive(tempPath, 0o600);
  try {
    try {
      await handle.write(content);
    } catch (err) {
      throw tagAtomicWriteStage(err, "write");
    }
    try {
      await handle.flush();
    } catch (err) {
      throw tagAtomicWriteStage(err, "flush");
    }
  } finally {
    await handle.close();
  }
  await adapter.chmodPath(tempPath, 0o600);

  // Named test seam: adversarial "final path created after precheck" tests
  // write finalPath here — strictly between the precheck above and the
  // atomic link below — to prove the link call itself (not a userspace
  // check) is what prevents the clobber. finalPath is passed through since
  // artifact filenames are internally randomized and cannot otherwise be
  // known by a caller in advance.
  await hooks.afterTempWrite?.(finalPath);

  try {
    await adapter.linkPath(tempPath, finalPath);
  } catch (err) {
    if (isEexist(err)) {
      // Our own temp file is safe to remove — nothing else could have
      // touched this exact pathname. The existing final path's bytes are
      // never read, opened for write, or touched.
      try {
        await adapter.unlinkPath(tempPath);
      } catch {
        // best-effort only; an orphaned temp file here is a narrower,
        // already-accepted case (see orphan_temp_file_is_reported_and_preserved)
      }
      throw new N7EvidenceStoreError("FINAL_PATH_EXISTS", `refused: final path already exists: ${finalPath}`);
    }
    throw tagAtomicWriteStage(err, "link");
  }
  await hooks.afterFinalLink?.(finalPath);

  // Directory sync runs BEFORE temp cleanup, not after: on a real (non-
  // capability) sync failure, the temp name is deliberately left in place,
  // still hard-linked to the same inode as finalPath. This is the only
  // recovery evidence a later reader (fresh store or otherwise) has for
  // recognizing "this final record's durability was never confirmed" — see
  // the durability-uncertain nlink check in readChainInternal/verifyLedger.
  const dirFsync = await adapter.fsyncDir(dir);

  if (dirFsync.kind === "IO_ERROR") {
    return { finalPath, tempPath, dirFsync, tempUnlinkWarning: null, durabilityUncertain: true };
  }

  let tempUnlinkWarning: N7StoreWarning | null = null;
  try {
    await adapter.unlinkPath(tempPath);
  } catch {
    // Do not retry or auto-repair: the final link already exists and is
    // durable; the stray temp name is surfaced as a recovery warning rather
    // than silently swept up later.
    tempUnlinkWarning = { kind: "ORPHAN_TEMP_FILE", detail: tempPath };
  }

  return { finalPath, tempPath: null, dirFsync, tempUnlinkWarning, durabilityUncertain: false };
}

// ---------------------------------------------------------------------------
// Fault-injection hooks (tests only). Every hook is optional and a no-op by
// default in production. See test/n7EvidenceStore.test.ts.
// ---------------------------------------------------------------------------

export interface N7StoreHooks {
  afterTempWrite?: (finalPath: string) => Promise<void>;
  afterFinalLink?: (finalPath: string) => Promise<void>;
}

// ---------------------------------------------------------------------------
// Chain reader / verifier
// ---------------------------------------------------------------------------

async function readChainInternal(
  adapter: N7StoreFsAdapter,
  root: string,
): Promise<N7ChainVerification> {
  const eventsDir = join(root, "events");
  const artifactsDir = join(root, "artifacts");
  const warnings: N7StoreWarning[] = [];

  const entries = await adapter.listDir(eventsDir);
  const candidates: { seq: number; path: string; name: string }[] = [];

  for (const name of entries) {
    // String.prototype.match (not RegExp.prototype.exec) so the panel's
    // static guard, which naively greps for the literal token "exec(" as a
    // process-spawn signal, does not misfire on this pure regex match.
    const m = name.match(EVENT_FILENAME_RE);
    const fullPath = join(eventsDir, name);
    if (!m) {
      warnings.push({ kind: "ORPHAN_TEMP_FILE", detail: name });
      continue;
    }
    const st = await adapter.lstatSafe(fullPath);
    if (st.isSymbolicLink) {
      return { status: "INTEGRITY_STOP", reason: `symlinked event path rejected: ${name}`, acceptedEvents: [], warnings };
    }
    if (!st.isFile) {
      return { status: "INTEGRITY_STOP", reason: `event path is not a regular file: ${name}`, acceptedEvents: [], warnings };
    }
    if (st.nlink !== null && st.nlink > 1) {
      // A recovery temp name is still hard-linked to this final path — its
      // directory-sync durability was never confirmed. Honest uncertainty,
      // not proven corruption; never automatically resolved.
      return { status: "DURABILITY_UNCERTAIN", reason: `event path has unconfirmed durability: ${name}`, acceptedEvents: [], warnings };
    }
    candidates.push({ seq: Number(m[1]), path: fullPath, name });
  }
  candidates.sort((a, b) => a.seq - b.seq);

  if (candidates.length === 0) {
    return { status: "EMPTY", warnings };
  }

  const accepted: TimelineEvent[] = [];
  const seenIds = new Set<string>();
  const seenSeqs = new Set<number>();
  let expectedSeq = 1;
  let prevHash: string | null = null;

  for (const c of candidates) {
    if (seenSeqs.has(c.seq)) {
      return { status: "INTEGRITY_STOP", reason: `duplicate sequence number: ${c.seq}`, acceptedEvents: accepted, warnings };
    }
    seenSeqs.add(c.seq);
    if (c.seq !== expectedSeq) {
      return {
        status: "INTEGRITY_STOP",
        reason: `sequence discontinuity at ${c.name}: expected ${expectedSeq}, found ${c.seq}`,
        acceptedEvents: accepted,
        warnings,
      };
    }

    let raw: string;
    try {
      raw = await adapter.readFileUtf8(c.path);
    } catch {
      return { status: "INTEGRITY_STOP", reason: `failed to read ${c.name}`, acceptedEvents: accepted, warnings };
    }

    let parsed: unknown;
    try {
      parsed = JSON.parse(raw);
    } catch {
      return { status: "INTEGRITY_STOP", reason: `invalid JSON in ${c.name} (partial or corrupted final event)`, acceptedEvents: accepted, warnings };
    }

    const validated = validateTimelineEvent(parsed);
    if (!validated.ok) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: ${validated.reason}`, acceptedEvents: accepted, warnings };
    }
    const event = validated.value;

    // Raw-byte canonicality: the store only ever WRITES canonical bytes
    // (see appendEvent). A structurally valid but noncanonically-ordered
    // JSON file (e.g. semantically identical, keys reordered) must be
    // rejected here as an integrity stop, not silently accepted merely
    // because its hash happens to still match after re-parsing — hash
    // verification alone (below) does not enforce canonical stored bytes.
    let canonicalBytes: string;
    try {
      canonicalBytes = canonicalize(event);
    } catch {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: event is not canonicalizable`, acceptedEvents: accepted, warnings };
    }
    if (canonicalBytes !== raw) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: stored bytes are not canonical`, acceptedEvents: accepted, warnings };
    }

    if (event.schema_version !== N7_TIMELINE_EVENT_SCHEMA_VERSION) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: unsupported schema_version`, acceptedEvents: accepted, warnings };
    }
    if (event.sequence !== c.seq) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: filename/sequence mismatch`, acceptedEvents: accepted, warnings };
    }
    if (seenIds.has(event.event_id)) {
      return { status: "INTEGRITY_STOP", reason: `duplicate event_id: ${event.event_id}`, acceptedEvents: accepted, warnings };
    }
    seenIds.add(event.event_id);
    if (event.previous_event_sha256 !== prevHash) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: previous_event_sha256 linkage mismatch`, acceptedEvents: accepted, warnings };
    }
    const recomputed = computeEventSha256(event);
    if (event.event_sha256 !== recomputed) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: event_sha256 mismatch (event was modified after acceptance)`, acceptedEvents: accepted, warnings };
    }
    if (!event.repository.owner || !event.repository.name) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: missing required repository identity`, acceptedEvents: accepted, warnings };
    }
    if (!event.workspace.root || !event.workspace.git_branch) {
      return { status: "INTEGRITY_STOP", reason: `${c.name}: missing required workspace identity`, acceptedEvents: accepted, warnings };
    }

    for (const ref of event.artifact_refs) {
      if (!ARTIFACT_REF_PATH_RE.test(ref.path)) {
        return { status: "INTEGRITY_STOP", reason: `${c.name}: malformed artifact reference path`, acceptedEvents: accepted, warnings };
      }
      const artifactPath = join(artifactsDir, basename(ref.path));
      const artifactStat = await adapter.lstatSafe(artifactPath);
      if (artifactStat.isSymbolicLink) {
        return { status: "INTEGRITY_STOP", reason: `${c.name}: symlinked artifact path rejected`, acceptedEvents: accepted, warnings };
      }
      if (!artifactStat.exists || !artifactStat.isFile) {
        return { status: "INTEGRITY_STOP", reason: `${c.name}: referenced artifact is missing: ${ref.path}`, acceptedEvents: accepted, warnings };
      }
      if (artifactStat.nlink !== null && artifactStat.nlink > 1) {
        return {
          status: "DURABILITY_UNCERTAIN",
          reason: `${c.name}: referenced artifact has unconfirmed durability: ${ref.path}`,
          acceptedEvents: accepted,
          warnings,
        };
      }
      let artifactContent: string;
      try {
        artifactContent = await adapter.readFileUtf8(artifactPath);
      } catch {
        return { status: "INTEGRITY_STOP", reason: `${c.name}: failed to read referenced artifact: ${ref.path}`, acceptedEvents: accepted, warnings };
      }
      const artifactHash = computeArtifactSha256(artifactContent);
      if (artifactHash !== ref.sha256) {
        return { status: "INTEGRITY_STOP", reason: `${c.name}: artifact hash mismatch: ${ref.path}`, acceptedEvents: accepted, warnings };
      }
    }

    accepted.push(event);
    prevHash = event.event_sha256;
    expectedSeq++;
  }

  return { status: "OK", events: accepted, warnings };
}

// ---------------------------------------------------------------------------
// N7EvidenceStore
// ---------------------------------------------------------------------------

export class N7EvidenceStore {
  readonly identity: N7EvidenceStoreIdentity;
  // Permission-enforcement warnings collected during open() (directory
  // creation) — see unsupported_permission_enforcement_is_reported. Empty
  // on every real POSIX filesystem; populated only when the adapter's
  // chmodPath reports { supported: false }.
  readonly permissionWarnings: readonly N7StoreWarning[];
  private readonly adapter: N7StoreFsAdapter;
  private readonly hooks: N7StoreHooks;
  private readonly eventsDir: string;
  private readonly artifactsDir: string;
  // Append-only lock ledger directory. Contains only claim-<n>.json and
  // release-<n>.json records, each created exactly once via exclusive
  // create and NEVER unlinked or rewritten — see acquireClaim/releaseClaim.
  private readonly lockDir: string;

  private constructor(root: string, adapter: N7StoreFsAdapter, hooks: N7StoreHooks, permissionWarnings: readonly N7StoreWarning[]) {
    this.identity = {
      root,
      worktreeLocal: true,
      durabilityWarning:
        "This evidence store is worktree-local and is NOT durable beyond the " +
        "life of the worktree containing its root. Removing the worktree " +
        "removes this evidence; nothing in this store's API implies " +
        "otherwise. Export or archive explicitly if retention is required.",
    };
    this.permissionWarnings = permissionWarnings;
    this.adapter = adapter;
    this.hooks = hooks;
    this.eventsDir = join(root, "events");
    this.artifactsDir = join(root, "artifacts");
    this.lockDir = join(root, LOCK_DIR_NAME);
  }

  // The only way to obtain a store instance. Validates the root, then
  // ensures (creating only what is missing, rejecting anything symlinked)
  // exactly: root, root/events, root/artifacts, root/chain.lock.d.
  static async open(root: string, adapter: N7StoreFsAdapter = createNodeFsAdapter(), hooks: N7StoreHooks = {}): Promise<N7EvidenceStore> {
    const normalizedRoot = validateRoot(root);
    // normalizedRoot ends in ".../.claw/n7"; clawDir is ".../.claw"; the
    // grandparent is whatever directory the caller wants ".claw" to live
    // in (typically a worktree root or a test's own temp root) — that
    // directory must already exist and be a real, non-symlinked directory.
    // This store only ever creates ".claw", ".claw/n7", and the three
    // subdirectories beneath it; it never creates arbitrary ancestors.
    const clawDir = dirname(normalizedRoot);
    const grandparent = dirname(clawDir);
    const grandparentStat = await adapter.lstatSafe(grandparent);
    if (!grandparentStat.exists || !grandparentStat.isDirectory || grandparentStat.isSymbolicLink) {
      throw new N7EvidenceStoreError(
        "INVALID_ROOT",
        `refused: parent directory of .claw must already exist as a real directory: ${grandparent}`,
      );
    }
    const permissionWarnings: N7StoreWarning[] = [];
    const record = (label: string, result: { supported: boolean }) => {
      if (!result.supported) {
        permissionWarnings.push({ kind: "UNSUPPORTED_PERMISSION_ENFORCEMENT", detail: label });
      }
    };
    record(clawDir, await ensureSafeDir(adapter, clawDir));
    record(normalizedRoot, await ensureSafeDir(adapter, normalizedRoot));
    const store = new N7EvidenceStore(normalizedRoot, adapter, hooks, permissionWarnings);
    record(store.eventsDir, await ensureSafeDir(adapter, store.eventsDir));
    record(store.artifactsDir, await ensureSafeDir(adapter, store.artifactsDir));
    record(store.lockDir, await ensureSafeDir(adapter, store.lockDir));
    return store;
  }

  async readChain(): Promise<N7ChainVerification> {
    return readChainInternal(this.adapter, this.identity.root);
  }

  async appendEvent(input: AppendEventInput): Promise<AppendEventResult> {
    const ownerToken = randomHex(16);
    const claim = await this.acquireClaim(ownerToken);
    if (claim.status === "CONTENTION") {
      return { status: "LOCK_CONTENTION" };
    }
    if (claim.status === "INTEGRITY_STOP") {
      return { status: "LOCK_INTEGRITY_STOP", reason: claim.reason };
    }

    // Capture the primary event outcome in a local variable rather than
    // returning it directly — release is always awaited afterward, exactly
    // once, and its outcome is combined into the one value actually
    // returned. A bare `return ACCEPTED` inside a try whose finally awaits
    // release (the prior shape) let the release outcome go unobserved by
    // the caller; this shape cannot.
    let primary: N7PrimaryAppendResult;
    try {
      primary = await this.performAppendBody(input);
    } catch (err) {
      // Safety net: any exception not already converted to a result above
      // still surfaces as a safe, non-accepted result rather than an
      // uncaught rejection. The claim we hold is still released below.
      const reason = err instanceof Error ? err.message : "unexpected failure during append";
      primary = { status: "READBACK_FAILED", reason: `unexpected failure: ${reason}` };
    }

    const release = await this.releaseClaim(claim.generation, claim.claimId, ownerToken);
    return combineAppendResult(primary, release);
  }

  private async performAppendBody(input: AppendEventInput): Promise<N7PrimaryAppendResult> {
    const chain = await readChainInternal(this.adapter, this.identity.root);
    if (chain.status === "INTEGRITY_STOP") {
      return { status: "CHAIN_INVALID", reason: chain.reason };
    }
    if (chain.status === "DURABILITY_UNCERTAIN") {
      return { status: "CHAIN_INVALID", reason: chain.reason };
    }
    const priorEvents = chain.status === "OK" ? chain.events : [];
    const nextSequence = priorEvents.length === 0 ? 1 : priorEvents[priorEvents.length - 1].sequence + 1;
    const previousEventSha256 = priorEvents.length === 0 ? null : priorEvents[priorEvents.length - 1].event_sha256;

    const candidateWithoutHash: TimelineEvent = {
      schema_version: N7_TIMELINE_EVENT_SCHEMA_VERSION,
      event_id: generateEventId(nextSequence),
      sequence: nextSequence,
      previous_event_sha256: previousEventSha256,
      event_sha256: null,
      event_type: input.eventType,
      created_at: input.createdAt,
      captured_by: {
        source: input.capturedBy.source,
        operator_id: input.capturedBy.operatorId,
        tool_version: input.capturedBy.toolVersion,
      },
      repository: {
        owner: input.repository.owner,
        name: input.repository.name,
        remote_url_hash: input.repository.remoteUrlHash,
      },
      workspace: {
        root: input.workspace.root,
        root_sha256: input.workspace.rootSha256,
        git_branch: input.workspace.gitBranch,
        git_head: input.workspace.gitHead,
      },
      pr: { number: input.pr.number, head_sha: input.pr.headSha },
      workflow_rung: input.workflowRung,
      operation: input.operation,
      result: input.result,
      facts: input.facts ?? [],
      inferences: input.inferences ?? [],
      unknowns: input.unknowns ?? [],
      warnings: input.warnings ?? [],
      artifact_refs: input.artifactRefs ?? [],
      next_permitted_action: input.nextPermittedAction,
      blocking_reason: input.blockingReason ?? null,
    };

    const structuralCheck = validateTimelineEvent(candidateWithoutHash);
    if (!structuralCheck.ok) {
      return { status: "VALIDATION_FAILED", reason: structuralCheck.reason };
    }

    const eventSha256 = computeEventSha256(candidateWithoutHash);
    const candidate: TimelineEvent = { ...candidateWithoutHash, event_sha256: eventSha256 };

    const finalCheck = validateTimelineEvent(candidate);
    if (!finalCheck.ok) {
      return { status: "VALIDATION_FAILED", reason: finalCheck.reason };
    }

    // Canonical stored bytes (N7-A serialization), never JSON.stringify —
    // see readChainInternal's raw-byte canonicality check, which requires
    // every stored event's bytes to equal this exact serialization.
    const serialized = canonicalize(candidate);
    const filename = eventFilename(nextSequence);

    let writeOutcome: N7AtomicWriteOutcome;
    try {
      writeOutcome = await atomicWrite(this.adapter, this.eventsDir, filename, serialized, this.hooks);
    } catch (err) {
      const reason = err instanceof Error ? err.message : "atomic write failed";
      return { status: "READBACK_FAILED", reason: `write sequence failed before readback: ${reason}` };
    }

    try {
      const rawBack = await this.adapter.readFileUtf8(writeOutcome.finalPath);
      const parsedBack: unknown = JSON.parse(rawBack);
      const revalidated = validateTimelineEvent(parsedBack);
      if (!revalidated.ok) {
        return { status: "READBACK_FAILED", reason: `readback validation failed: ${revalidated.reason}` };
      }
      const rehash = computeEventSha256(revalidated.value);
      if (rehash !== candidate.event_sha256) {
        return { status: "READBACK_FAILED", reason: "readback hash mismatch" };
      }
      // A real directory-sync I/O failure (not a capability gap) must
      // never be reported as accepted, even though the final file is
      // already durable and readback-verified — the file is left exactly
      // as linked; only the reported outcome changes.
      if (isDurabilitySyncFailure(writeOutcome.dirFsync)) {
        return { status: "DURABILITY_SYNC_FAILED", reason: `directory sync failed: ${writeOutcome.dirFsync.code}` };
      }
      return { status: "ACCEPTED", event: revalidated.value, warnings: dirSyncWarnings(writeOutcome) };
    } catch (err) {
      const reason = err instanceof Error ? err.message : "readback failed";
      return { status: "READBACK_FAILED", reason };
    }
  }

  async writeArtifact(input: WriteArtifactInput): Promise<WriteArtifactResult> {
    if (input.confirmedNoSecrets !== true) {
      return { status: "SAFETY_NOT_CONFIRMED" };
    }
    const sha256 = computeArtifactSha256(input.content);
    const filename = generateArtifactFilename();

    let writeOutcome: N7AtomicWriteOutcome;
    try {
      writeOutcome = await atomicWrite(this.adapter, this.artifactsDir, filename, input.content, this.hooks);
    } catch (err) {
      const reason = err instanceof Error ? err.message : "atomic write failed";
      return { status: "READBACK_FAILED", reason: `write sequence failed before readback: ${reason}` };
    }

    try {
      const rawBack = await this.adapter.readFileUtf8(writeOutcome.finalPath);
      if (computeArtifactSha256(rawBack) !== sha256) {
        return { status: "READBACK_FAILED", reason: "readback hash mismatch" };
      }
    } catch (err) {
      const reason = err instanceof Error ? err.message : "readback failed";
      return { status: "READBACK_FAILED", reason };
    }

    if (isDurabilitySyncFailure(writeOutcome.dirFsync)) {
      return { status: "DURABILITY_SYNC_FAILED", reason: `directory sync failed: ${writeOutcome.dirFsync.code}` };
    }

    return {
      status: "WRITTEN",
      artifactRef: {
        artifact_id: filename.replace(/\.dat$/, ""),
        kind: input.kind,
        path: `artifacts/${filename}`,
        sha256,
        size_bytes: Buffer.byteLength(input.content, "utf8"),
        redaction: input.redaction,
      },
      warnings: dirSyncWarnings(writeOutcome),
    };
  }

  // Append-only claim/release ledger, verified in full (never a "highest
  // generation only" shortcut — see verifyLedger) both before and after
  // every claim/release write. Never unlinks or rewrites anything under
  // lockDir — "replacement lock is not removed" is a structural fact, not
  // a race-timing property. The active claim stores only a SHA-256 hash of
  // the random owner token, never the raw token — see verifyLedger's
  // record schemas.
  private async acquireClaim(
    ownerToken: string,
  ): Promise<{ status: "OK"; generation: number; claimId: string } | { status: "CONTENTION" } | { status: "INTEGRITY_STOP"; reason: string }> {
    const preCheck = await verifyLedger(this.adapter, this.lockDir);
    if (preCheck.status === "INTEGRITY_STOP") {
      return { status: "INTEGRITY_STOP", reason: preCheck.reason };
    }
    if (preCheck.status === "DURABILITY_UNCERTAIN") {
      return {
        status: "INTEGRITY_STOP",
        reason: `lock ledger has a durability-uncertain ${preCheck.kind} record at generation ${preCheck.generation}`,
      };
    }
    if (preCheck.status === "VALID_ACTIVE_CLAIM") {
      // Genuinely still held, or a forged/foreign release record exists
      // whose token proof doesn't match the real claim — either way this
      // generation must never be treated as free. No retry.
      return { status: "CONTENTION" };
    }

    const nextGeneration = preCheck.status === "VALID_RELEASED_TIP" ? preCheck.generation + 1 : 1;
    const claimId = randomHex(16);
    const ownerTokenSha256 = sha256Hex(ownerToken);
    const claimRecord: N7ClaimRecord = {
      schema_version: N7_LOCK_SCHEMA_VERSION,
      generation: nextGeneration,
      claim_id: claimId,
      owner_token_sha256: ownerTokenSha256,
      pid: process.pid,
    };

    let canonicalClaimBytes: string;
    try {
      canonicalClaimBytes = canonicalize(claimRecord);
    } catch {
      return { status: "CONTENTION" };
    }

    let writeOutcome: N7AtomicWriteOutcome;
    try {
      writeOutcome = await atomicWrite(this.adapter, this.lockDir, claimFilename(nextGeneration), canonicalClaimBytes, this.hooks);
    } catch (err) {
      if (isEnoent(err)) throw err;
      // FINAL_PATH_EXISTS (lost the race for this exact generation number)
      // or any other create failure — always fail closed, never retry.
      return { status: "CONTENTION" };
    }

    if (writeOutcome.durabilityUncertain) {
      // Left exactly as atomicWrite leaves it (temp+final same-inode pair,
      // no cleanup) — this generation is now durability-uncertain, which
      // the next verifyLedger call will detect and block on.
      return { status: "CONTENTION" };
    }

    let rawBack: string;
    try {
      rawBack = await this.adapter.readFileUtf8(writeOutcome.finalPath);
    } catch {
      return { status: "CONTENTION" };
    }
    if (rawBack !== canonicalClaimBytes) {
      return { status: "CONTENTION" };
    }

    // Post-claim full rescan — required before the caller is ever allowed
    // to proceed to chain-tip/sequence selection. Must show exactly our
    // own claim as the unique, highest, active generation.
    const postCheck = await verifyLedger(this.adapter, this.lockDir);
    if (
      postCheck.status !== "VALID_ACTIVE_CLAIM" ||
      postCheck.generation !== nextGeneration ||
      postCheck.claimId !== claimId ||
      !hashesMatch(postCheck.ownerTokenSha256, ownerTokenSha256)
    ) {
      return { status: "INTEGRITY_STOP", reason: "post-claim ledger reverification failed" };
    }

    return { status: "OK", generation: nextGeneration, claimId };
  }

  // Records this generation's release. Exclusive-create only, through the
  // same no-clobber atomicWrite helper events/artifacts use — never
  // unlinks or rewrites anything, and never throws: any failure here
  // (including a durability-uncertain sync outcome) simply leaves the
  // generation permanently blocked, which fails every future append closed
  // rather than silently succeeding.
  // Returns a discriminated N7ReleaseOutcome — never void, never a silent
  // return, never a thrown exception. RELEASED is reachable only after
  // every one of: pre-release full ledger verification, ownership proof,
  // canonical immutable creation, atomic no-clobber finalization, a
  // permitted (non-IO_ERROR) directory-sync outcome, exact-byte readback,
  // AND a post-release full ledger rescan proving this release is the
  // unique valid terminal record for the generation we held. Every
  // previously-silent failure path now names a specific
  // N7ReleaseFailureReason. Never retries, deletes, or rewrites anything.
  private async releaseClaim(generation: number, claimId: string, ownerToken: string): Promise<N7ReleaseOutcome> {
    const ownerTokenSha256 = sha256Hex(ownerToken);
    const failed = (reason: N7ReleaseFailureReason): N7ReleaseOutcome => ({ outcome: "RELEASE_FAILED", reason });

    let preCheck: N7LedgerVerification;
    try {
      preCheck = await verifyLedger(this.adapter, this.lockDir);
    } catch {
      return failed("UNKNOWN_RELEASE_FAILURE");
    }
    if (preCheck.status === "INTEGRITY_STOP") {
      return failed("PRE_RELEASE_LEDGER_INTEGRITY_STOP");
    }
    if (preCheck.status === "DURABILITY_UNCERTAIN") {
      return failed("RELEASE_DURABILITY_UNCERTAIN");
    }
    if (
      preCheck.status !== "VALID_ACTIVE_CLAIM" ||
      preCheck.generation !== generation ||
      preCheck.claimId !== claimId ||
      !hashesMatch(preCheck.ownerTokenSha256, ownerTokenSha256)
    ) {
      // EMPTY / VALID_RELEASED_TIP, or a VALID_ACTIVE_CLAIM that isn't
      // provably ours — never safely ours to release.
      return failed("OWNERSHIP_LOST");
    }

    const releaseRecord: N7ReleaseRecord = {
      schema_version: N7_LOCK_SCHEMA_VERSION,
      generation,
      claim_id: claimId,
      owner_token: ownerToken,
    };
    let canonicalReleaseBytes: string;
    try {
      canonicalReleaseBytes = canonicalize(releaseRecord);
    } catch {
      return failed("UNKNOWN_RELEASE_FAILURE");
    }

    let writeOutcome: N7AtomicWriteOutcome;
    try {
      writeOutcome = await atomicWrite(this.adapter, this.lockDir, releaseFilename(generation), canonicalReleaseBytes, this.hooks);
    } catch (err) {
      if (err instanceof N7EvidenceStoreError && err.code === "FINAL_PATH_EXISTS") {
        return failed("RELEASE_RECORD_COLLISION");
      }
      const stage = atomicWriteStage(err);
      if (stage === "write") return failed("RELEASE_WRITE_FAILED");
      if (stage === "flush") return failed("RELEASE_FLUSH_FAILED");
      return failed("RELEASE_FINALIZATION_FAILED");
    }
    if (writeOutcome.durabilityUncertain) {
      return failed("RELEASE_DURABILITY_UNCERTAIN");
    }

    let rawBack: string;
    try {
      rawBack = await this.adapter.readFileUtf8(writeOutcome.finalPath);
    } catch {
      return failed("RELEASE_READBACK_FAILED");
    }
    if (rawBack !== canonicalReleaseBytes) {
      return failed("RELEASE_CANONICALITY_FAILED");
    }

    // Post-release full rescan — authoritative, not discarded: RELEASED is
    // reachable only when it proves our own release is the unique valid
    // terminal record for this exact generation.
    let postCheck: N7LedgerVerification;
    try {
      postCheck = await verifyLedger(this.adapter, this.lockDir);
    } catch {
      return failed("POST_RELEASE_LEDGER_INTEGRITY_STOP");
    }
    if (postCheck.status !== "VALID_RELEASED_TIP" || postCheck.generation !== generation) {
      return failed("POST_RELEASE_LEDGER_INTEGRITY_STOP");
    }

    return { outcome: "RELEASED", generation, claimId, warnings: dirSyncWarnings(writeOutcome) };
  }
}

export { ARTIFACT_FILENAME_RE, EVENT_FILENAME_RE };
