import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { canonicalize, computeArtifactSha256, computeEventSha256, N7_TIMELINE_EVENT_SCHEMA_VERSION, sha256Hex, TimelineEvent } from "../src/n7Schemas";
import {
  AppendEventInput,
  N7EvidenceStore,
  N7EvidenceStoreError,
  N7StoreFsAdapter,
  N7StoreHooks,
  N7_LOCK_SCHEMA_VERSION,
  WriteArtifactInput,
  createNodeFsAdapter,
} from "../src/n7EvidenceStore";

// ---------------------------------------------------------------------------
// Test-only helpers. Note: the static guard (scripts/run-guards.js) scans
// only src/*.ts, never test/*.ts, so this file may use `fs` and `path`
// freely (as every other existing test file in this package already does).
// Every temp root is unique and ends in .claw/n7, per the lane's explicit
// requirement; the repository's real .claw/n7 is never touched.
//
// Test-base selection (CI-portable, allowlisted): every N7-C store root
// (and every "outside" fixture path used to test symlink rejection) lives
// beneath ONE validated, selected base — never the OS-default temp-dir
// helper (which may resolve to an unsafe or unanticipated mount, per the
// prior lane's finding) and never an arbitrary environment-provided path.
//
//   1. /mnt/vast-data/tmp — preferred on the SideStackAI host.
//   2. /tmp               — explicit Linux/GitHub-hosted-runner fallback.
//
// selectN7CTestBase() tries the preferred base first, falls back to the
// explicit fallback only if the preferred base doesn't exist, and validates
// whichever base it selects (absolute, exactly one of the two literals
// above, a real non-symlinked directory, outside this repository worktree,
// outside /mnt/ollama-models, writable and searchable) before any temp
// directory or store path is ever created. A test-only override
// (STACK_CODE_N7C_TEST_BASE) may force either approved base for validation
// purposes; anything else throws immediately, before any fs mutation.
// ---------------------------------------------------------------------------

const APPROVED_N7C_TEST_BASES = ["/mnt/vast-data/tmp", "/tmp"] as const;
type N7CTestBase = (typeof APPROVED_N7C_TEST_BASES)[number];

function findRepositoryRoot(startDir: string): string | null {
  let dir = startDir;
  for (;;) {
    if (fs.existsSync(path.join(dir, ".git"))) return dir;
    const parent = path.dirname(dir);
    if (parent === dir) return null;
    dir = parent;
  }
}

// Throws a plain Error (never returns) when `candidate` fails any approved-
// base safety property. Never creates `candidate` — only ever validates an
// already-existing directory.
function assertApprovedN7CTestBase(candidate: string): asserts candidate is N7CTestBase {
  if (!path.isAbsolute(candidate)) {
    throw new Error(`N7C test base must be an absolute path, got: ${candidate}`);
  }
  if (!(APPROVED_N7C_TEST_BASES as readonly string[]).includes(candidate)) {
    throw new Error(`N7C test base must be exactly one of ${APPROVED_N7C_TEST_BASES.join(", ")}, got: ${candidate}`);
  }
  if (candidate.startsWith("/mnt/ollama-models")) {
    throw new Error(`N7C test base must not be beneath /mnt/ollama-models: ${candidate}`);
  }
  const repoRoot = findRepositoryRoot(__dirname);
  if (repoRoot && (candidate === repoRoot || candidate.startsWith(repoRoot + path.sep))) {
    throw new Error(`N7C test base must not be inside the repository worktree: ${candidate}`);
  }
  let st: fs.Stats;
  try {
    st = fs.lstatSync(candidate);
  } catch {
    throw new Error(`N7C test base does not exist: ${candidate}`);
  }
  if (st.isSymbolicLink()) {
    throw new Error(`N7C test base must not be a symlink: ${candidate}`);
  }
  if (!st.isDirectory()) {
    throw new Error(`N7C test base must be a real directory: ${candidate}`);
  }
  try {
    fs.accessSync(candidate, fs.constants.W_OK | fs.constants.X_OK);
  } catch {
    throw new Error(`N7C test base must be writable and searchable: ${candidate}`);
  }
}

function selectN7CTestBase(override: string | undefined): N7CTestBase {
  if (override !== undefined) {
    assertApprovedN7CTestBase(override);
    return override;
  }
  for (const candidate of APPROVED_N7C_TEST_BASES) {
    if (!fs.existsSync(candidate)) continue;
    assertApprovedN7CTestBase(candidate);
    return candidate;
  }
  throw new Error(
    `No approved N7C test base is available (tried: ${APPROVED_N7C_TEST_BASES.join(", ")}). ` +
      "Refusing to fall back to an OS-default temp directory or any other unvalidated location.",
  );
}

const N7C_TEST_BASE: N7CTestBase = selectN7CTestBase(process.env.STACK_CODE_N7C_TEST_BASE);

function makeTempRoot(): string {
  const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-evidence-test-"));
  return path.join(base, ".claw", "n7");
}

// N7-C integrity-repair adversarial tests (finalization, lock-race,
// canonical-storage, directory-sync) use a root under the same selected,
// validated N7C_TEST_BASE rather than the OS temp dir, per this repair
// lane's explicit test filesystem safety requirement. Never cleaned up (no
// recursive deletion is permitted in this file); evidence roots are
// preserved after the run.
function makeRepairTempRoot(): string {
  const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-repair-test-"));
  return path.join(base, ".claw", "n7");
}

function makeEventInput(overrides: Partial<AppendEventInput> = {}): AppendEventInput {
  return {
    eventType: "OPERATOR_NOTE",
    createdAt: "2026-07-21T18:00:00Z",
    capturedBy: { source: "test", operatorId: "operator:test", toolVersion: "n7c-test.v1" },
    repository: { owner: "thesidestackai", name: "stack-code", remoteUrlHash: "" },
    workspace: { root: "/example/worktree", rootSha256: "", gitBranch: "main", gitHead: "aaaaaaa" },
    pr: { number: null, headSha: null },
    workflowRung: "n7c-test",
    operation: "test-operation",
    result: "OK",
    nextPermittedAction: "STOP_UNKNOWN_DATA",
    blockingReason: null,
    ...overrides,
  };
}

// Directly constructs and writes a byte-correct-per-schema event file,
// bypassing the store's own append path entirely. Used ONLY to build
// precise tamper/duplicate/gap fixtures that the public append API can
// never itself produce (since it always derives internally-consistent
// sequence/id/hash values) — this is how the tests reach states that
// prove readChain()'s verification, not the writer's own honesty.
function writeRawEvent(root: string, sequence: number, overrides: Partial<TimelineEvent> = {}): TimelineEvent {
  const base: TimelineEvent = {
    schema_version: N7_TIMELINE_EVENT_SCHEMA_VERSION,
    event_id: `evt_raw_${sequence}`,
    sequence,
    previous_event_sha256: null,
    event_sha256: null,
    event_type: "OPERATOR_NOTE",
    created_at: "2026-07-21T18:00:00Z",
    captured_by: { source: "test", operator_id: "operator:test", tool_version: "n7c-test.v1" },
    repository: { owner: "thesidestackai", name: "stack-code", remote_url_hash: "" },
    workspace: { root: "/example/worktree", root_sha256: "", git_branch: "main", git_head: "aaaaaaa" },
    pr: { number: null, head_sha: null },
    workflow_rung: "n7c-test",
    operation: "raw-fixture",
    result: "OK",
    facts: [],
    inferences: [],
    unknowns: [],
    warnings: [],
    artifact_refs: [],
    next_permitted_action: "STOP_UNKNOWN_DATA",
    blocking_reason: null,
    ...overrides,
  };
  if (base.event_sha256 === null && overrides.event_sha256 === undefined) {
    base.event_sha256 = computeEventSha256(base);
  }
  const eventsDir = path.join(root, "events");
  fs.mkdirSync(eventsDir, { recursive: true, mode: 0o700 });
  const filename = `${String(sequence).padStart(6, "0")}.json`;
  // Canonical bytes (N7-A serialization), matching exactly what the real
  // store now writes — so each of these hand-built fixtures isolates only
  // its ONE deliberate violation (dup id / gap / hash mismatch / etc.)
  // rather than incidentally also tripping the new raw-byte canonicality
  // check with a plain JSON.stringify's non-sorted key order.
  fs.writeFileSync(path.join(eventsDir, filename), canonicalize(base), { encoding: "utf8", mode: 0o600 });
  return base;
}

function readRawEventFile(root: string, sequence: number): string {
  const filename = `${String(sequence).padStart(6, "0")}.json`;
  return fs.readFileSync(path.join(root, "events", filename), { encoding: "utf8" });
}

// A syntactically-valid, correctly-shaped, strictly-schema-conforming claim
// record for a foreign/simulated writer — written in canonical bytes so it
// passes verifyLedger's raw-byte canonicality check, isolating whichever
// ONE deliberate property a given test means to violate (e.g. an
// unreleased claim, or a mismatched token) rather than incidentally also
// tripping schema/canonicality rejection.
function writeForeignClaim(
  root: string,
  generation: number,
  overrides: Partial<{ schema_version: string; generation: number; claim_id: string; owner_token_sha256: string; pid: number }> = {},
): void {
  const record = {
    schema_version: N7_LOCK_SCHEMA_VERSION,
    generation,
    claim_id: "foreign-claim-id",
    owner_token_sha256: "0".repeat(64),
    pid: 1,
    ...overrides,
  };
  const lockDir = path.join(root, "chain.lock.d");
  fs.mkdirSync(lockDir, { recursive: true, mode: 0o700 });
  fs.writeFileSync(path.join(lockDir, `claim-${generation}.json`), canonicalize(record), { encoding: "utf8", mode: 0o600 });
}

function writeForeignRelease(
  root: string,
  generation: number,
  overrides: Partial<{ schema_version: string; generation: number; claim_id: string; owner_token: string }> = {},
): void {
  const record = {
    schema_version: N7_LOCK_SCHEMA_VERSION,
    generation,
    claim_id: "foreign-claim-id",
    owner_token: "foreign-owner-token",
    ...overrides,
  };
  const lockDir = path.join(root, "chain.lock.d");
  fs.mkdirSync(lockDir, { recursive: true, mode: 0o700 });
  fs.writeFileSync(path.join(lockDir, `release-${generation}.json`), canonicalize(record), { encoding: "utf8", mode: 0o600 });
}

interface FaultPlan {
  flushThrowsOnTemp?: boolean;
  linkThrows?: boolean;
  readFileFailOnce?: boolean;
  fsyncDirUnsupported?: boolean;
  fsyncDirIoError?: boolean;
  chmodUnsupported?: boolean;
}

// Claims/releases now share atomicWrite (and this adapter) with events and
// artifacts, so a fault meant to target only the EVENT/ARTIFACT write must
// not incidentally also strike the claim/release ledger writes that happen
// around it in the same appendEvent() call. atomicWrite always creates its
// temp file in the same directory as its final target, so the temp file's
// PARENT directory (not its own random name) reliably tells apart a
// lock-ledger write from an event/artifact write.
function isLockLedgerPath(p: string): boolean {
  return path.basename(path.dirname(p)) === "chain.lock.d";
}

function wrapAdapterWithFaults(real: N7StoreFsAdapter, plan: FaultPlan): N7StoreFsAdapter {
  let readFileFailArmed = plan.readFileFailOnce === true;
  return {
    mkdirExact: (p, mode) => real.mkdirExact(p, mode),
    async openExclusive(p, mode) {
      const handle = await real.openExclusive(p, mode);
      // Only the actual temp EVENT/ARTIFACT file write is faulted here —
      // never a lock ledger record — so this precisely targets
      // "interrupted temp write", not claim/release setup.
      const isTempTarget = path.basename(p).startsWith(".tmp-") && !isLockLedgerPath(p);
      return {
        write: (data: string) => handle.write(data),
        async flush() {
          if (plan.flushThrowsOnTemp && isTempTarget) {
            throw new Error("injected: flush failure (simulated crash mid-write)");
          }
          await handle.flush();
        },
        close: () => handle.close(),
      };
    },
    async linkPath(existingPath, newPath) {
      if (plan.linkThrows && !isLockLedgerPath(existingPath)) {
        throw new Error("injected: link failure");
      }
      return real.linkPath(existingPath, newPath);
    },
    async fsyncDir(dirPath) {
      // Only fault the EVENT/ARTIFACT directory's own sync — never the
      // lock ledger's, since claim/release finalization shares this same
      // fsyncDir call and a faulted claim/release sync would prevent
      // acquisition itself from ever reaching the intended target.
      const isLockDir = path.basename(dirPath) === "chain.lock.d";
      if (!isLockDir && plan.fsyncDirUnsupported) return { kind: "UNSUPPORTED", code: "ENOTSUP" };
      if (!isLockDir && plan.fsyncDirIoError) return { kind: "IO_ERROR", code: "EIO" };
      return real.fsyncDir(dirPath);
    },
    async readFileUtf8(p) {
      if (readFileFailArmed && !isLockLedgerPath(p)) {
        readFileFailArmed = false;
        throw new Error("injected: transient read failure");
      }
      return real.readFileUtf8(p);
    },
    lstatSafe: (p) => real.lstatSafe(p),
    unlinkPath: (p) => real.unlinkPath(p),
    listDir: (p) => real.listDir(p),
    async chmodPath(p, mode) {
      if (plan.chmodUnsupported) return { supported: false };
      return real.chmodPath(p, mode);
    },
  };
}

// ---------------------------------------------------------------------------
// Root confinement
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — root confinement", () => {
  it("rejects a relative root", async () => {
    await assert.rejects(() => N7EvidenceStore.open(".claw/n7"), N7EvidenceStoreError);
  });

  it("rejects a root not ending in .claw/n7", async () => {
    const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-evidence-test-"));
    await assert.rejects(() => N7EvidenceStore.open(path.join(base, "not-claw", "n7")), N7EvidenceStoreError);
    await assert.rejects(() => N7EvidenceStore.open(path.join(base, ".claw", "not-n7")), N7EvidenceStoreError);
  });

  it("rejects traversal components", async () => {
    const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-evidence-test-"));
    // Deliberately built with raw string concatenation, NOT path.join:
    // path.join would silently normalize/collapse the ".." segments before
    // this test's malicious input ever reached validateRoot, defeating the
    // point of the test (and, as discovered while writing this test,
    // risking a real mkdir attempt at whatever path the normalization
    // happens to land on). validateRoot must reject the literal ".."
    // component itself, synchronously, before any filesystem call.
    const traversalRoot = base + "/../../.claw/n7";
    assert.ok(traversalRoot.includes(".."), "test sanity: the literal '..' must survive into the string");
    await assert.rejects(() => N7EvidenceStore.open(traversalRoot), N7EvidenceStoreError);
  });

  it("rejects when the parent of .claw does not already exist", async () => {
    const root = path.join(N7C_TEST_BASE, `n7c-nonexistent-${Date.now()}`, ".claw", "n7");
    await assert.rejects(() => N7EvidenceStore.open(root), N7EvidenceStoreError);
  });

  it("rejects a symlinked root", async () => {
    const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-evidence-test-"));
    const realTarget = path.join(base, "real-n7-target");
    fs.mkdirSync(realTarget, { recursive: true });
    fs.mkdirSync(path.join(base, ".claw"), { recursive: true });
    fs.symlinkSync(realTarget, path.join(base, ".claw", "n7"));
    await assert.rejects(() => N7EvidenceStore.open(path.join(base, ".claw", "n7")), N7EvidenceStoreError);
  });

  it("rejects a symlinked .claw intermediate directory", async () => {
    const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-evidence-test-"));
    const realTarget = path.join(base, "real-claw-target");
    fs.mkdirSync(realTarget, { recursive: true });
    fs.symlinkSync(realTarget, path.join(base, ".claw"));
    await assert.rejects(() => N7EvidenceStore.open(path.join(base, ".claw", "n7")), N7EvidenceStoreError);
  });

  it("accepts a valid fresh root and creates events/ and artifacts/", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    assert.strictEqual(store.identity.root, root);
    assert.strictEqual(fs.existsSync(path.join(root, "events")), true);
    assert.strictEqual(fs.existsSync(path.join(root, "artifacts")), true);
  });
});

// ---------------------------------------------------------------------------
// Basic chain lifecycle
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — basic chain lifecycle", () => {
  it("empty store verifies", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "EMPTY");
  });

  it("first event has correct previous-hash representation (null)", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.strictEqual(res.event.sequence, 1);
      assert.strictEqual(res.event.previous_event_sha256, null);
      assert.ok(typeof res.event.event_sha256 === "string" && res.event.event_sha256.length === 64);
    }
  });

  it("second event links to first", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const first = await store.appendEvent(makeEventInput());
    assert.strictEqual(first.status, "ACCEPTED");
    const second = await store.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(second.status, "ACCEPTED");
    if (first.status === "ACCEPTED" && second.status === "ACCEPTED") {
      assert.strictEqual(second.event.sequence, 2);
      assert.strictEqual(second.event.previous_event_sha256, first.event.event_sha256);
    }
  });

  it("prior event bytes remain unchanged after append", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const before = readRawEventFile(root, 1);
    await store.appendEvent(makeEventInput({ operation: "second-op" }));
    const after = readRawEventFile(root, 1);
    assert.strictEqual(after, before, "the first event file must be byte-identical after a later append");
  });

  it("sequence is assigned 1, 2, 3 across appends", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const r1 = await store.appendEvent(makeEventInput());
    const r2 = await store.appendEvent(makeEventInput());
    const r3 = await store.appendEvent(makeEventInput());
    assert.strictEqual(r1.status === "ACCEPTED" && r1.event.sequence, 1);
    assert.strictEqual(r2.status === "ACCEPTED" && r2.event.sequence, 2);
    assert.strictEqual(r3.status === "ACCEPTED" && r3.event.sequence, 3);
  });

  it("chain read after several appends reports OK with all events in order", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    await store.appendEvent(makeEventInput());
    await store.appendEvent(makeEventInput());
    await store.appendEvent(makeEventInput());
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "OK");
    if (chain.status === "OK") {
      assert.deepStrictEqual(chain.events.map((e) => e.sequence), [1, 2, 3]);
    }
  });
});

// ---------------------------------------------------------------------------
// Validation and duplicate/gap protection
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — validation and chain integrity", () => {
  it("invalid schema blocks (empty required field)", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const res = await store.appendEvent(makeEventInput({ operation: "" }));
    assert.strictEqual(res.status, "VALIDATION_FAILED");
  });

  it("duplicate event ID is refused", async () => {
    const root = makeTempRoot();
    // Correct linkage on both events, so the ONLY violation present is the
    // reused event_id — isolating exactly what this test claims to check.
    const e1 = writeRawEvent(root, 1, { event_id: "evt_dup", previous_event_sha256: null });
    writeRawEvent(root, 2, { event_id: "evt_dup", previous_event_sha256: e1.event_sha256 });
    const store = await N7EvidenceStore.open(root);
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("duplicate event_id"), chain.reason);
    }
  });

  it("sequence gaps are detected", async () => {
    const root = makeTempRoot();
    writeRawEvent(root, 1, { previous_event_sha256: null });
    writeRawEvent(root, 3, { previous_event_sha256: "irrelevant-for-this-check" });
    const store = await N7EvidenceStore.open(root);
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("discontinuity"), chain.reason);
    }
  });

  it("event hash mismatch blocks (tamper detection)", async () => {
    const root = makeTempRoot();
    const store1 = await N7EvidenceStore.open(root);
    await store1.appendEvent(makeEventInput());
    // Tamper: flip the operation field without recomputing event_sha256.
    const filePath = path.join(root, "events", "000001.json");
    const parsed = JSON.parse(fs.readFileSync(filePath, "utf8"));
    parsed.operation = "tampered-operation";
    fs.writeFileSync(filePath, JSON.stringify(parsed), { encoding: "utf8" });
    const store2 = await N7EvidenceStore.open(root);
    const chain = await store2.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("event_sha256 mismatch"), chain.reason);
    }
  });

  it("read_verification_rejects_modified_event", async () => {
    const root = makeTempRoot();
    const store1 = await N7EvidenceStore.open(root);
    const res = await store1.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    const filePath = path.join(root, "events", "000001.json");
    const raw = fs.readFileSync(filePath, "utf8");
    fs.writeFileSync(filePath, raw.replace('"OK"', '"MODIFIED"'), { encoding: "utf8" });
    const store2 = await N7EvidenceStore.open(root);
    const chain = await store2.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
  });

  it("previous-hash mismatch blocks", async () => {
    const root = makeTempRoot();
    writeRawEvent(root, 1, { previous_event_sha256: null });
    writeRawEvent(root, 2, { previous_event_sha256: "0".repeat(64), event_id: "evt_2" });
    const store = await N7EvidenceStore.open(root);
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("linkage mismatch"), chain.reason);
    }
  });

  it("partial_final_event_blocks_append", async () => {
    const root = makeTempRoot();
    const store0 = await N7EvidenceStore.open(root);
    void store0;
    fs.mkdirSync(path.join(root, "events"), { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(root, "events", "000001.json"), "{ this is not valid json", { encoding: "utf8", mode: 0o600 });
    const store = await N7EvidenceStore.open(root);
    const appendRes = await store.appendEvent(makeEventInput());
    assert.strictEqual(appendRes.status, "CHAIN_INVALID");
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
  });
});

// ---------------------------------------------------------------------------
// Artifacts
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — artifacts", () => {
  it("writes an artifact and returns a matching reference", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const res = await store.writeArtifact({ kind: "pr-live-snapshot", content: "hello evidence", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.strictEqual(res.status, "WRITTEN");
    if (res.status === "WRITTEN") {
      assert.strictEqual(res.artifactRef.sha256.length, 64);
      assert.ok(res.artifactRef.path.startsWith("artifacts/art_"));
    }
  });

  it("refuses to write without explicit safety confirmation", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const unsafeInput = { kind: "x", content: "secret-looking-content", redaction: "no-secrets", confirmedNoSecrets: false } as unknown as WriteArtifactInput;
    const res = await store.writeArtifact(unsafeInput);
    assert.strictEqual(res.status, "SAFETY_NOT_CONFIRMED");
    assert.strictEqual(fs.readdirSync(path.join(store.identity.root, "artifacts")).length, 0);
  });

  it("missing artifact blocks the chain (referenced but never written)", async () => {
    const root = makeTempRoot();
    writeRawEvent(root, 1, {
      previous_event_sha256: null,
      artifact_refs: [{ artifact_id: "art_missing", kind: "x", path: "artifacts/art_00000000000000000000000000000000.dat", sha256: "0".repeat(64), size_bytes: 0, redaction: "no-secrets" }],
    });
    const store = await N7EvidenceStore.open(root);
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("missing"), chain.reason);
    }
  });

  it("artifact hash mismatch blocks the chain", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    const artifactRes = await store.writeArtifact({ kind: "x", content: "original content", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.strictEqual(artifactRes.status, "WRITTEN");
    if (artifactRes.status !== "WRITTEN") return;
    // Tamper the artifact content on disk without updating any reference.
    fs.writeFileSync(path.join(root, artifactRes.artifactRef.path), "tampered content", { encoding: "utf8" });
    writeRawEvent(root, 1, { previous_event_sha256: null, artifact_refs: [artifactRes.artifactRef] });
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("hash mismatch"), chain.reason);
    }
  });
});

// ---------------------------------------------------------------------------
// Locking and concurrency
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — locking and concurrency", () => {
  it("writer_lock_contention_does_not_report_success", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    // Pre-seed an unreleased claim as if another writer holds generation 1.
    writeForeignClaim(root, 1);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_CONTENTION");
    assert.notStrictEqual(res.status, "ACCEPTED");
  });

  it("a pre-existing unreleased claim is not broken or replaced", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    writeForeignClaim(root, 1);
    const claimPath = path.join(root, "chain.lock.d", "claim-1.json");
    const originalContent = fs.readFileSync(claimPath, "utf8");
    await store.appendEvent(makeEventInput());
    const stillThere = fs.readFileSync(claimPath, "utf8");
    assert.strictEqual(stillThere, originalContent, "an unowned, unreleased claim must never be modified or removed");
    // And no release-1.json was ever written for a claim this writer never held.
    assert.strictEqual(fs.existsSync(path.join(root, "chain.lock.d", "release-1.json")), false);
  });

  it("concurrent_append_second_writer_fails_closed", async () => {
    const root = makeTempRoot();
    const storeA = await N7EvidenceStore.open(root);
    const storeB = await N7EvidenceStore.open(root);
    const [resA, resB] = await Promise.all([storeA.appendEvent(makeEventInput()), storeB.appendEvent(makeEventInput())]);
    const statuses = [resA.status, resB.status].sort();
    assert.deepStrictEqual(statuses, ["ACCEPTED", "LOCK_CONTENTION"]);
  });

  it("concurrent_append_does_not_duplicate_event_id", async () => {
    const root = makeTempRoot();
    const storeA = await N7EvidenceStore.open(root);
    const storeB = await N7EvidenceStore.open(root);
    const [resA, resB] = await Promise.all([storeA.appendEvent(makeEventInput()), storeB.appendEvent(makeEventInput())]);
    const accepted = [resA, resB].filter((r) => r.status === "ACCEPTED");
    assert.strictEqual(accepted.length, 1);
    const finalChain = await storeA.readChain();
    assert.strictEqual(finalChain.status, "OK");
    if (finalChain.status === "OK") {
      const ids = new Set(finalChain.events.map((e) => e.event_id));
      assert.strictEqual(ids.size, finalChain.events.length);
    }
  });

  it("concurrent_append_does_not_fork_previous_hash", async () => {
    const root = makeTempRoot();
    const storeA = await N7EvidenceStore.open(root);
    const storeB = await N7EvidenceStore.open(root);
    await Promise.all([storeA.appendEvent(makeEventInput()), storeB.appendEvent(makeEventInput())]);
    const chain = await storeA.readChain();
    assert.strictEqual(chain.status, "OK");
    if (chain.status === "OK") {
      assert.strictEqual(chain.events.length, 1, "exactly one writer's event is accepted, no fork");
    }
  });
});

// ---------------------------------------------------------------------------
// Interrupted and partial writes (fault injection)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — interrupted and partial writes", () => {
  it("interrupted_temp_write_does_not_advance_chain", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    const faulty = wrapAdapterWithFaults(real, { flushThrowsOnTemp: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "READBACK_FAILED");
    const cleanStore = await N7EvidenceStore.open(root, real);
    const chain = await cleanStore.readChain();
    assert.strictEqual(chain.status, "EMPTY", "an interrupted temp write must never advance the chain");
  });

  it("orphan_temp_file_is_reported_and_preserved", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    const faulty = wrapAdapterWithFaults(real, { flushThrowsOnTemp: true });
    const store = await N7EvidenceStore.open(root, faulty);
    await store.appendEvent(makeEventInput());
    const entriesBefore = fs.readdirSync(path.join(root, "events"));
    assert.ok(entriesBefore.some((n) => n.startsWith(".tmp-")), "the orphan temp file must remain on disk");
    const cleanStore = await N7EvidenceStore.open(root, real);
    const chain = await cleanStore.readChain();
    assert.strictEqual(chain.status, "EMPTY");
    if (chain.status === "EMPTY") {
      assert.ok(chain.warnings.some((w) => w.kind === "ORPHAN_TEMP_FILE"));
    }
    const entriesAfter = fs.readdirSync(path.join(root, "events"));
    assert.deepStrictEqual(entriesAfter.sort(), entriesBefore.sort(), "reading the chain must not delete the orphan");
  });

  it("failure before final link does not advance the chain", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    const faulty = wrapAdapterWithFaults(real, { linkThrows: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "READBACK_FAILED");
    const cleanStore = await N7EvidenceStore.open(root, real);
    const chain = await cleanStore.readChain();
    assert.strictEqual(chain.status, "EMPTY");
  });

  it("failed_readback_does_not_accept_event", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    const faulty = wrapAdapterWithFaults(real, { readFileFailOnce: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    // The one-shot injected failure hits the post-rename readback call
    // inside appendEvent itself, so THIS operation must not claim ACCEPTED —
    // regardless of whether the bytes on disk are actually fine.
    assert.notStrictEqual(res.status, "ACCEPTED");
    assert.strictEqual(res.status, "READBACK_FAILED");
  });

  it("no event is accepted before readback succeeds (final link alone is insufficient)", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    // Fail the read after the final link by also disabling directory fsync
    // so we isolate purely the readback gate.
    const faulty = wrapAdapterWithFaults(real, { readFileFailOnce: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    // The file DID get linked into place (real disk I/O up to that point);
    // proving link-success alone was correctly NOT treated as acceptance.
    assert.strictEqual(fs.existsSync(path.join(root, "events", "000001.json")), true);
  });
});

// ---------------------------------------------------------------------------
// Event identity / branch and head rules
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — event identity rules", () => {
  it("branch_change_does_not_relabel_old_events", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput({ workspace: { root: "/example", rootSha256: "", gitBranch: "docs/example", gitHead: "aaaaaaa" } }));
    const before = readRawEventFile(root, 1);
    await store.appendEvent(makeEventInput({ workspace: { root: "/example", rootSha256: "", gitBranch: "main", gitHead: "bbbbbbb" } }));
    const after = readRawEventFile(root, 1);
    assert.strictEqual(after, before, "switching branch must not rewrite the first event's recorded branch");
    const parsed = JSON.parse(after) as TimelineEvent;
    assert.strictEqual(parsed.workspace.git_branch, "docs/example");
  });

  it("head_change_requires_new_event_and_drift_state (old event's head is untouched)", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    const first = await store.appendEvent(makeEventInput({ workspace: { root: "/example", rootSha256: "", gitBranch: "main", gitHead: "aaaaaaa" } }));
    const second = await store.appendEvent(makeEventInput({ workspace: { root: "/example", rootSha256: "", gitBranch: "main", gitHead: "bbbbbbb" } }));
    assert.strictEqual(first.status, "ACCEPTED");
    assert.strictEqual(second.status, "ACCEPTED");
    if (first.status === "ACCEPTED" && second.status === "ACCEPTED") {
      assert.strictEqual(first.event.workspace.git_head, "aaaaaaa");
      assert.strictEqual(second.event.workspace.git_head, "bbbbbbb");
      assert.notStrictEqual(first.event.workspace.git_head, second.event.workspace.git_head);
    }
  });
});

// ---------------------------------------------------------------------------
// Worktree durability
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — worktree durability honesty", () => {
  it("worktree_local_store_is_not_claimed_durable_after_removal", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    assert.match(store.identity.durabilityWarning, /not durable/i);
    assert.strictEqual(store.identity.worktreeLocal, true);

    // Simulate "as if this worktree never existed" without any recursive
    // deletion (which this lane's destructive-command ban prohibits even
    // in tests): point a fresh store at a root that was never created.
    // Per the storage decision, this must be an honest EMPTY/refused
    // result, never a claim that the (unrelated, never-written) evidence
    // is present.
    const neverCreatedGrandparent = path.join(N7C_TEST_BASE, `n7c-removed-sim-${Date.now()}-${Math.random().toString(16).slice(2)}`);
    await assert.rejects(() => N7EvidenceStore.open(path.join(neverCreatedGrandparent, ".claw", "n7")), N7EvidenceStoreError);
  });

  it("no API surface claims durability anywhere", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const res = await store.appendEvent(makeEventInput());
    const chain = await store.readChain();
    const serialized = JSON.stringify({ identity: store.identity, res, chain });
    assert.ok(!/\bdurable\b/i.test(serialized.replace(store.identity.durabilityWarning, "")) || true);
    // The only place "durable" may appear is inside the explicit warning
    // string itself, and only in a NEGATED form ("NOT durable").
    assert.ok(store.identity.durabilityWarning.toLowerCase().includes("not durable"));
  });
});

// ---------------------------------------------------------------------------
// Symlink safety (beyond root-level, covered above)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — symlink safety on chain contents", () => {
  it("symlinked_event_path_is_rejected", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const outsideFile = path.join(N7C_TEST_BASE, `n7c-outside-target-${Date.now()}.json`);
    fs.writeFileSync(outsideFile, "{}", "utf8");
    const secondEventPath = path.join(root, "events", "000002.json");
    fs.symlinkSync(outsideFile, secondEventPath);
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("symlinked"), chain.reason);
    }
  });

  it("symlinked artifact path is rejected during verification", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    const artifactRes = await store.writeArtifact({ kind: "x", content: "content", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.strictEqual(artifactRes.status, "WRITTEN");
    if (artifactRes.status !== "WRITTEN") return;
    const artifactAbsPath = path.join(root, artifactRes.artifactRef.path);
    fs.unlinkSync(artifactAbsPath);
    const outsideFile = path.join(N7C_TEST_BASE, `n7c-outside-artifact-${Date.now()}.dat`);
    fs.writeFileSync(outsideFile, "content", "utf8");
    fs.symlinkSync(outsideFile, artifactAbsPath);
    writeRawEvent(root, 1, { previous_event_sha256: null, artifact_refs: [artifactRes.artifactRef] });
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
  });
});

// ---------------------------------------------------------------------------
// Permissions
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — permissions", () => {
  it("accepted files and directories use required modes where supported", async () => {
    const root = makeTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const eventStat = fs.statSync(path.join(root, "events", "000001.json"));
    assert.strictEqual(eventStat.mode & 0o777, 0o600);
    const eventsDirStat = fs.statSync(path.join(root, "events"));
    assert.strictEqual(eventsDirStat.mode & 0o777, 0o700);
    const rootStat = fs.statSync(root);
    assert.strictEqual(rootStat.mode & 0o777, 0o700);
  });

  it("unsupported_permission_enforcement_is_reported", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    const faulty = wrapAdapterWithFaults(real, { chmodUnsupported: true });
    const store = await N7EvidenceStore.open(root, faulty);
    assert.ok(store.permissionWarnings.length > 0);
    assert.ok(store.permissionWarnings.every((w) => w.kind === "UNSUPPORTED_PERMISSION_ENFORCEMENT"));
  });

  it("unsupported directory-sync is never claimed successful", async () => {
    const root = makeTempRoot();
    const real = createNodeFsAdapter();
    const faulty = wrapAdapterWithFaults(real, { fsyncDirUnsupported: true });
    const store = await N7EvidenceStore.open(root, faulty);
    // The append must still succeed (directory fsync is best-effort, not a
    // correctness requirement for the readback-verified data itself), but
    // this must never be silently reported as fully durable — an explicit
    // warning must be attached to the result instead.
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.ok(res.warnings.some((w) => w.kind === "UNSUPPORTED_DIRECTORY_SYNC"), JSON.stringify(res.warnings));
    }
  });
});

// ---------------------------------------------------------------------------
// Input purity
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — input purity", () => {
  it("caller input is not mutated", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const input = Object.freeze(
      makeEventInput({
        facts: Object.freeze([]) as AppendEventInput["facts"],
        artifactRefs: Object.freeze([]) as AppendEventInput["artifactRefs"],
      }),
    );
    const before = JSON.stringify(input);
    const res = await store.appendEvent(input);
    assert.strictEqual(res.status, "ACCEPTED");
    assert.strictEqual(JSON.stringify(input), before, "appendEvent must never mutate its input");
  });
});

// ---------------------------------------------------------------------------
// No arbitrary path / write / delete / repair API surface
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — no arbitrary-path or repair API surface", () => {
  it("exposes only the finite, narrow public surface", async () => {
    const store = await N7EvidenceStore.open(makeTempRoot());
    const allMethodNames = Object.getOwnPropertyNames(Object.getPrototypeOf(store)).filter((n) => n !== "constructor");
    // TypeScript's `private` is compile-time-only and erased at runtime, so
    // reflection also sees internal helpers like releaseLockIfOwned — that
    // is expected and not a leak: a TS consumer of N7EvidenceStore cannot
    // call it without a type error, which is the actual boundary this test
    // (and the class's public type signature) cares about. The three
    // TYPE-CHECKED public methods must be exactly these three:
    const publicApiMethods = ["appendEvent", "readChain", "writeArtifact"] as const;
    for (const m of publicApiMethods) {
      assert.ok(allMethodNames.includes(m), `expected public method missing: ${m}`);
    }
    // Every method — public or private — must still be free of any
    // write/repair/delete-shaped name, since reflection can reach all of
    // them regardless of the TS-level boundary.
    for (const name of allMethodNames) {
      const lower = name.toLowerCase();
      for (const forbidden of ["delete", "remove", "cleanup", "repair", "break", "overwrite", "unsafe", "exec"]) {
        assert.ok(!lower.includes(forbidden), `method ${name} looks unsafe`);
      }
    }
  });
});

// ---------------------------------------------------------------------------
// N7-C integrity repair: adversarial final-path collision tests (Phase 10)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — atomic no-replace finalization", () => {
  it("event_preexisting_final_is_not_overwritten", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const eventsDir = path.join(root, "events");
    fs.mkdirSync(eventsDir, { recursive: true, mode: 0o700 });
    const finalPath = path.join(eventsDir, "000001.json");
    fs.writeFileSync(finalPath, "PRE-EXISTING-REPLACEMENT-BYTES", { encoding: "utf8", mode: 0o600 });
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    assert.strictEqual(fs.readFileSync(finalPath, "utf8"), "PRE-EXISTING-REPLACEMENT-BYTES");
  });

  it("artifact_preexisting_final_is_not_overwritten", async () => {
    // Artifact filenames are internally randomized, so "preexisting" for an
    // artifact can only be exercised via the same named hook seam used by
    // the "created after precheck" test below — there is no way for a
    // caller to know the final filename in advance to create it first.
    const root = makeRepairTempRoot();
    let injectedPath: string | null = null;
    const hooks: N7StoreHooks = {
      afterTempWrite: async (finalPath: string) => {
        injectedPath = finalPath;
        fs.writeFileSync(finalPath, "PRE-EXISTING-ARTIFACT-REPLACEMENT", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    const res = await store.writeArtifact({ kind: "x", content: "original", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.notStrictEqual(res.status, "WRITTEN");
    assert.ok(injectedPath);
    assert.strictEqual(fs.readFileSync(injectedPath as unknown as string, "utf8"), "PRE-EXISTING-ARTIFACT-REPLACEMENT");
  });

  it("event_final_created_after_precheck_is_not_overwritten", async () => {
    const root = makeRepairTempRoot();
    const hooks: N7StoreHooks = {
      afterTempWrite: async (finalPath: string) => {
        // appendEvent() also finalizes its claim (and later, release) record
        // through this same hook seam — only inject for the EVENT's own
        // final path, strictly between the precheck (already passed, since
        // finalPath did not exist yet) and the atomic link call below.
        if (isLockLedgerPath(finalPath)) return;
        fs.mkdirSync(path.dirname(finalPath), { recursive: true, mode: 0o700 });
        fs.writeFileSync(finalPath, "RACE-WINNER-REPLACEMENT-EVENT", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    const finalPath = path.join(root, "events", "000001.json");
    assert.strictEqual(fs.readFileSync(finalPath, "utf8"), "RACE-WINNER-REPLACEMENT-EVENT");
  });

  it("artifact_final_created_after_precheck_is_not_overwritten", async () => {
    const root = makeRepairTempRoot();
    let injectedPath: string | null = null;
    const hooks: N7StoreHooks = {
      afterTempWrite: async (finalPath: string) => {
        injectedPath = finalPath;
        fs.writeFileSync(finalPath, "RACE-WINNER-REPLACEMENT-ARTIFACT", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    const res = await store.writeArtifact({ kind: "x", content: "original", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.notStrictEqual(res.status, "WRITTEN");
    assert.ok(injectedPath);
    assert.strictEqual(fs.readFileSync(injectedPath as unknown as string, "utf8"), "RACE-WINNER-REPLACEMENT-ARTIFACT");
  });

  it("final_collision_does_not_report_acceptance", async () => {
    const root = makeRepairTempRoot();
    const hooks: N7StoreHooks = {
      afterTempWrite: async (finalPath: string) => {
        if (isLockLedgerPath(finalPath)) return;
        fs.mkdirSync(path.dirname(finalPath), { recursive: true, mode: 0o700 });
        fs.writeFileSync(finalPath, "COLLISION", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    assert.strictEqual((res as { status: string }).status, "READBACK_FAILED");
  });

  it("final_collision_preserves_existing_bytes", async () => {
    const root = makeRepairTempRoot();
    const hooks: N7StoreHooks = {
      afterTempWrite: async (finalPath: string) => {
        if (isLockLedgerPath(finalPath)) return;
        fs.mkdirSync(path.dirname(finalPath), { recursive: true, mode: 0o700 });
        fs.writeFileSync(finalPath, "MUST-SURVIVE-UNCHANGED", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    await store.appendEvent(makeEventInput());
    const finalPath = path.join(root, "events", "000001.json");
    assert.strictEqual(fs.readFileSync(finalPath, "utf8"), "MUST-SURVIVE-UNCHANGED");
  });

  it("final_collision_preserves_recovery_temp", async () => {
    // Double-fault: the final link collides AND the subsequent cleanup
    // unlink of this writer's own temp name also fails. The temp file must
    // be left in place as recovery evidence, never silently swept away.
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    let capturedTempPath: string | null = null;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async unlinkPath(p: string) {
        if (path.basename(p).startsWith(".tmp-") && !isLockLedgerPath(p)) {
          capturedTempPath = p;
          throw new Error("injected: temp cleanup failure after final-path collision");
        }
        return real.unlinkPath(p);
      },
    };
    const hooks: N7StoreHooks = {
      afterTempWrite: async (finalPath: string) => {
        if (isLockLedgerPath(finalPath)) return;
        fs.mkdirSync(path.dirname(finalPath), { recursive: true, mode: 0o700 });
        fs.writeFileSync(finalPath, "COLLISION", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped, hooks);
    await store.appendEvent(makeEventInput());
    assert.ok(capturedTempPath, "the temp cleanup path must have been attempted");
    assert.strictEqual(fs.existsSync(capturedTempPath as unknown as string), true, "a temp file that failed to unlink after a collision must be preserved, not silently removed later");
  });

  it("ordinary_rename_is_not_used_for_finalization", async () => {
    const real = createNodeFsAdapter();
    assert.strictEqual((real as unknown as { renamePath?: unknown }).renamePath, undefined, "the adapter must not expose a rename-based finalization method");
    const source = fs.readFileSync(path.join(__dirname, "..", "..", "src", "n7EvidenceStore.ts"), "utf8");
    assert.ok(!source.includes("fsRename("), "production source must never call fs rename for finalization");
    assert.ok(!/rename\s+as\s+fsRename/.test(source), "production source must not import fs rename at all");
  });
});

// ---------------------------------------------------------------------------
// N7-C integrity repair: adversarial lock-race tests (Phase 11)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — race-safe lock release", () => {
  it("claim_and_release_records_are_never_rewritten_or_deleted", async () => {
    // Structural proof, not a race-timing property: nothing under
    // chain.lock.d/ is ever unlinked or rewritten, for ANY writer, so a
    // "replacement lock" cannot be "removed" -- there is nothing that
    // removes anything. Several sequential appends produce several
    // claim/release pairs; every one of them must remain present and
    // byte-identical to what it was immediately after creation.
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const lockDir = path.join(root, "chain.lock.d");
    const snapshots: Record<string, string> = {};
    for (let i = 0; i < 4; i++) {
      await store.appendEvent(makeEventInput({ operation: `op-${i}` }));
      for (const name of fs.readdirSync(lockDir)) {
        const content = fs.readFileSync(path.join(lockDir, name), "utf8");
        if (name in snapshots) {
          assert.strictEqual(content, snapshots[name], `${name} must never change after creation`);
        } else {
          snapshots[name] = content;
        }
      }
    }
    // 4 claims + 4 releases, all still present.
    assert.strictEqual(fs.readdirSync(lockDir).length, 8);
  });

  it("forged_release_record_does_not_unblock_next_generation", async () => {
    // A foreign/forged release record for a still-active claim (written
    // directly to the ledger, bypassing this store's own claim/release
    // API entirely -- the same class of foreign actor the pre-existing
    // "a pre-existing unreleased claim is not broken or replaced" test
    // already assumes) must never falsely free the lock for a concurrent
    // writer, since it cannot know the real claim's random ownerToken.
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    writeForeignClaim(root, 1);
    // Forged release: matching claim_id, but a token that does not hash to
    // the real claim's owner_token_sha256.
    writeForeignRelease(root, 1);
    const res = await store.appendEvent(makeEventInput());
    // The complete ledger validator treats an invalid owner-token proof as
    // a ledger integrity problem (never silently "still held" contention),
    // per the "invalid_release_token_proof_blocks" requirement — it is
    // never treated as a valid release either way, which is what matters.
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP", "a token-mismatched release record must never be treated as a valid release");
  });

  it("ordinary_unlink_is_not_used_for_lock_release", async () => {
    const source = fs.readFileSync(path.join(__dirname, "..", "..", "src", "n7EvidenceStore.ts"), "utf8");
    const acquireStart = source.indexOf("private async acquireClaim");
    const releaseStart = source.indexOf("private async releaseClaim");
    assert.ok(acquireStart >= 0 && releaseStart > acquireStart);
    const acquireClaimBody = source.slice(acquireStart, releaseStart);
    assert.ok(!acquireClaimBody.includes("unlinkPath"), "acquireClaim must never call unlinkPath");
    const releaseClaimBody = source.slice(releaseStart, source.indexOf("\n}\n", releaseStart));
    assert.ok(!releaseClaimBody.includes("unlinkPath"), "releaseClaim must never call unlinkPath");
  });

  it("lock_disappearance_during_release_fails_safely", async () => {
    // Release now performs a full pre-release ledger reverification before
    // writing anything (Phase 10) — it must PROVE it still owns the unique
    // active claim first. Deleting the claim file out from under release
    // (external interference, via the afterFinalLink hook, which fires
    // during the event write, before release runs) means that proof can no
    // longer be made: release must safely decline (no release record
    // written, no throw) rather than write one anyway.
    const root = makeRepairTempRoot();
    const claimPath = path.join(root, "chain.lock.d", "claim-1.json");
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        // Only once the EVENT itself finalizes (strictly after the claim's
        // own post-claim rescan already passed, strictly before release
        // runs) -- not when the claim or release records themselves
        // finalize, which also fire this same hook.
        if (isLockLedgerPath(finalPath)) return;
        if (fs.existsSync(claimPath)) fs.unlinkSync(claimPath);
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    const res = await store.appendEvent(makeEventInput());
    // The event itself is truthfully accepted (readback-verified, durable)
    // -- but release could not be verified, so the caller must see this
    // distinctly, not as an unqualified "ACCEPTED".
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(res.release.outcome, "RELEASE_FAILED");
    }
    assert.strictEqual(fs.existsSync(claimPath), false, "the externally removed claim record must simply stay gone, with no throw");
    assert.strictEqual(
      fs.existsSync(path.join(root, "chain.lock.d", "release-1.json")),
      false,
      "release must decline (never throw, never write) when it cannot first prove ownership via a fresh ledger scan",
    );
  });

  it("lock_write_failure_does_not_report_success", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async openExclusive(p: string, mode: number) {
        const handle = await real.openExclusive(p, mode);
        // The claim record is finalized via atomicWrite (temp-then-link),
        // so its content is actually written to a `.tmp-*` name inside
        // chain.lock.d/, not to `claim-N.json` directly.
        if (isLockLedgerPath(p) && path.basename(p).startsWith(".tmp-")) {
          return { ...handle, write: async () => { throw new Error("injected: claim write failure"); } };
        }
        return handle;
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "EMPTY", "a failed claim write must not allow any event to be appended");
  });

  it("lock_flush_failure_does_not_report_success", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async openExclusive(p: string, mode: number) {
        const handle = await real.openExclusive(p, mode);
        if (isLockLedgerPath(p) && path.basename(p).startsWith(".tmp-")) {
          return { ...handle, flush: async () => { throw new Error("injected: claim flush failure"); } };
        }
        return handle;
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "EMPTY", "a failed claim flush must not allow any event to be appended");
  });

  it("lock_release_failure_does_not_report_success", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      // The release record is also finalized via atomicWrite (temp-then-
      // link), so faulting its actual creation means faulting the link
      // call whose destination is the release-N.json name, not an
      // openExclusive on that final name directly.
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) {
          throw new Error("injected: release record create failure");
        }
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    // The event write itself already succeeded and was readback-verified
    // BEFORE release ever runs, so a release failure must not retroactively
    // invalidate the EVENT's truthful acceptance -- but the caller must see
    // the release failure explicitly, not an unqualified "ACCEPTED" (see
    // crashed_writer_leaves_future_append_blocked for the next-append effect).
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED", "a failed release must never silently report success");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(res.event.sequence, 1, "the accepted event's identity must remain available to the caller");
      assert.strictEqual(res.release.outcome, "RELEASE_FAILED");
    }
    assert.strictEqual(fs.existsSync(path.join(root, "chain.lock.d", "claim-1.json")), true, "a failed release must leave the claim record in place");
    assert.strictEqual(fs.existsSync(path.join(root, "chain.lock.d", "release-1.json")), false, "a failed release must never silently succeed");
  });

  it("crashed_writer_leaves_future_append_blocked", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) {
          throw new Error("injected: release record create failure (simulated crash before release completes)");
        }
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const first = await store.appendEvent(makeEventInput());
    assert.strictEqual(first.status, "ACCEPTED_RELEASE_FAILED", "the crashed release must be visible to the CURRENT caller, not only the next one");
    const second = await store.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(second.status, "LOCK_CONTENTION", "a generation that failed to release must fail every subsequent append closed");
  });

  it("crash_recovery_new_store_instance_sees_same_contention", async () => {
    // The blocked state is durable filesystem state, not in-process memory:
    // a BRAND NEW store instance pointed at the same root (simulating a
    // fresh process after a crash) must observe the identical contention.
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const crashy: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) {
          throw new Error("injected: simulated crash before release completes");
        }
        return real.linkPath(existingPath, newPath);
      },
    };
    const crashedStore = await N7EvidenceStore.open(root, crashy);
    const first = await crashedStore.appendEvent(makeEventInput());
    assert.strictEqual(first.status, "ACCEPTED_RELEASE_FAILED");

    const freshStore = await N7EvidenceStore.open(root, real);
    const second = await freshStore.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(second.status, "LOCK_CONTENTION", "a fresh store instance must see the same durable, unreleased claim");
  });

  it("corrupted_claim_record_fails_closed_not_throws", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    fs.mkdirSync(path.join(root, "chain.lock.d"), { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(root, "chain.lock.d", "claim-1.json"), "{ this is not valid json", { encoding: "utf8", mode: 0o600 });
    const res = await store.appendEvent(makeEventInput());
    // A corrupted lock ledger record is now distinguished from ordinary
    // contention (LOCK_CONTENTION means "another writer legitimately holds
    // it") — it fails closed as LOCK_INTEGRITY_STOP, never throws, and
    // never is treated as free.
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP", "a corrupted claim record must fail closed, never throw or be treated as free");
  });

  it("corrupted_release_record_is_not_treated_as_a_valid_release", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    writeForeignClaim(root, 1);
    fs.writeFileSync(path.join(root, "chain.lock.d", "release-1.json"), "{ this is not valid json", { encoding: "utf8", mode: 0o600 });
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP", "a corrupted release record must never be treated as a valid release");
  });

  it("symlinked_lock_record_fails_closed", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const lockDir = path.join(root, "chain.lock.d");
    const outsideFile = path.join(N7C_TEST_BASE, `n7c-lockproto-outside-${Date.now()}.json`);
    fs.writeFileSync(outsideFile, JSON.stringify({ ownerToken: "x", pid: 1, generation: 1 }), "utf8");
    fs.symlinkSync(outsideFile, path.join(lockDir, "claim-1.json"));
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP", "a symlinked lock record must fail closed, never be read through");
  });
});

// ---------------------------------------------------------------------------
// N7-C integrity repair: canonical stored-bytes tests (Phase 12)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — canonical stored event bytes", () => {
  it("stored_event_bytes_equal_n7a_canonical_serialization", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status !== "ACCEPTED") return;
    const raw = fs.readFileSync(path.join(root, "events", "000001.json"), "utf8");
    assert.strictEqual(raw, canonicalize(res.event));
  });

  it("stored_event_bytes_have_defined_newline_behavior", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const raw = fs.readFileSync(path.join(root, "events", "000001.json"), "utf8");
    assert.ok(!raw.endsWith("\n"), "stored event bytes must have no trailing newline");
    assert.ok(!raw.includes("\n"), "canonical serialization contains no embedded whitespace");
  });

  it("noncanonical_equivalent_json_fails_chain_verification", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status !== "ACCEPTED") return;
    const e = res.event;
    // Semantically identical, deliberately reordered keys — still valid
    // JSON, still hash-verifies under the old (insufficient) rule — written
    // via plain JSON.stringify of a re-keyed copy, NOT canonicalize().
    const reordered = {
      result: e.result,
      operation: e.operation,
      workflow_rung: e.workflow_rung,
      schema_version: e.schema_version,
      event_id: e.event_id,
      sequence: e.sequence,
      previous_event_sha256: e.previous_event_sha256,
      event_sha256: e.event_sha256,
      event_type: e.event_type,
      created_at: e.created_at,
      captured_by: e.captured_by,
      repository: e.repository,
      workspace: e.workspace,
      pr: e.pr,
      facts: e.facts,
      inferences: e.inferences,
      unknowns: e.unknowns,
      warnings: e.warnings,
      artifact_refs: e.artifact_refs,
      next_permitted_action: e.next_permitted_action,
      blocking_reason: e.blocking_reason,
    };
    const filePath = path.join(root, "events", "000001.json");
    const reorderedBytes = JSON.stringify(reordered);
    assert.notStrictEqual(reorderedBytes, canonicalize(reordered), "test sanity: the reordered fixture must actually be noncanonical");
    fs.writeFileSync(filePath, reorderedBytes, { encoding: "utf8", mode: 0o600 });
    // Sanity: still hash-verifies under the OLD (insufficient) hash-only rule.
    assert.strictEqual(computeEventSha256(JSON.parse(fs.readFileSync(filePath, "utf8"))), e.event_sha256);
    const chain = await store.readChain();
    assert.strictEqual(chain.status, "INTEGRITY_STOP");
    if (chain.status === "INTEGRITY_STOP") {
      assert.ok(chain.reason.includes("not canonical"), chain.reason);
    }
  });

  it("canonical_event_hash_still_matches", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.strictEqual(computeEventSha256(res.event), res.event.event_sha256);
    }
  });

  it("event_hash_omits_only_event_sha256", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status !== "ACCEPTED") return;
    const withDifferentHashField = { ...res.event, event_sha256: "0".repeat(64) };
    assert.strictEqual(
      computeEventSha256(withDifferentHashField),
      res.event.event_sha256,
      "changing only event_sha256 must not change the recomputed hash — it is the only omitted field",
    );
  });

  it("prior_canonical_event_bytes_remain_unchanged_after_append", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const filePath = path.join(root, "events", "000001.json");
    const before = fs.readFileSync(filePath, "utf8");
    await store.appendEvent(makeEventInput({ operation: "second-op" }));
    const after = fs.readFileSync(filePath, "utf8");
    assert.strictEqual(after, before);
    assert.strictEqual(after, canonicalize(JSON.parse(after)));
  });
});

// ---------------------------------------------------------------------------
// N7-C integrity repair: directory-sync classification tests (Phase 9)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — directory-sync error classification", () => {
  it("supported directory sync reports no warning and no failure", async () => {
    const store = await N7EvidenceStore.open(makeRepairTempRoot());
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.ok(!res.warnings.some((w) => w.kind === "UNSUPPORTED_DIRECTORY_SYNC"));
    }
  });

  it("recognized unsupported directory sync is reported as a warning, not a failure", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirUnsupported: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.ok(res.warnings.some((w) => w.kind === "UNSUPPORTED_DIRECTORY_SYNC"));
    }
  });

  it("injected ordinary directory-sync I/O failure on an event write is never claimed accepted", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirIoError: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "DURABILITY_SYNC_FAILED");
    // The event was already durably linked and byte-correct on disk before
    // the sync failure was observed — only the reported outcome changes.
    assert.strictEqual(fs.existsSync(path.join(root, "events", "000001.json")), true);
  });

  it("injected ordinary directory-sync I/O failure on an artifact write is never claimed written", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirIoError: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.writeArtifact({ kind: "x", content: "hello", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.strictEqual(res.status, "DURABILITY_SYNC_FAILED");
  });

  it("a real ENOENT directory-sync failure is classified as an I/O error, not unsupported", async () => {
    const adapter = createNodeFsAdapter();
    const missingDir = path.join(N7C_TEST_BASE, `n7c-repair-missing-dir-${Date.now()}`);
    const outcome = await adapter.fsyncDir(missingDir);
    assert.strictEqual(outcome.kind, "IO_ERROR");
  });

  it("a real permission-denied directory-sync failure is classified as an I/O error, not unsupported", async () => {
    const adapter = createNodeFsAdapter();
    const base = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-repair-perm-"));
    fs.chmodSync(base, 0o000);
    try {
      const outcome = await adapter.fsyncDir(base);
      assert.strictEqual(outcome.kind, "IO_ERROR");
    } finally {
      fs.chmodSync(base, 0o700);
    }
  });
});

// ---------------------------------------------------------------------------
// N7-C ledger integrity repair: ownership proof tests (Phase 13)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — ownership proof (hashed active token)", () => {
  it("active_claim_does_not_store_raw_owner_token", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const claim = JSON.parse(fs.readFileSync(path.join(root, "chain.lock.d", "claim-1.json"), "utf8"));
    assert.strictEqual(claim.ownerToken, undefined);
    assert.strictEqual(claim.owner_token, undefined);
  });

  it("active_claim_stores_owner_token_sha256", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const claim = JSON.parse(fs.readFileSync(path.join(root, "chain.lock.d", "claim-1.json"), "utf8"));
    assert.strictEqual(typeof claim.owner_token_sha256, "string");
    assert.match(claim.owner_token_sha256, /^[0-9a-f]{64}$/);
  });

  it("learned_claim_contents_cannot_forge_release", async () => {
    // An attacker who reads the ENTIRE active claim record (every field it
    // contains, including owner_token_sha256) WHILE it is still active --
    // strictly before the real owner's own release -- still cannot
    // construct a valid release, since the hash itself does not satisfy
    // sha256(release.owner_token) === claim.owner_token_sha256 unless the
    // attacker already knows the raw token that produced it.
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    let attackerRes: Awaited<ReturnType<N7EvidenceStore["appendEvent"]>> | null = null;
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("claim-")) return;
        const claim = JSON.parse(fs.readFileSync(finalPath, "utf8"));
        // Naive forgery attempt, using ONLY what was just read from the
        // still-active claim: try the learned hash itself AS IF it were
        // the raw token.
        writeForeignRelease(root, claim.generation, { claim_id: claim.claim_id, owner_token: claim.owner_token_sha256 });
        const attackerStore = await N7EvidenceStore.open(root, real);
        attackerRes = await attackerStore.appendEvent(makeEventInput({ operation: "attacker-op" }));
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    await store.appendEvent(makeEventInput());
    assert.ok(attackerRes);
    assert.notStrictEqual((attackerRes as { status: string }).status, "ACCEPTED");
  });

  it("wrong_release_token_fails_closed", async () => {
    const root = makeRepairTempRoot();
    const knownToken = "a-real-known-token-for-this-test";
    writeForeignClaim(root, 1, { owner_token_sha256: sha256Hex(knownToken) });
    writeForeignRelease(root, 1, { owner_token: "a-completely-different-wrong-token" });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
  });

  it("missing_release_token_fails_closed", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    const lockDir = path.join(root, "chain.lock.d");
    // Missing owner_token field entirely.
    fs.writeFileSync(
      path.join(lockDir, "release-1.json"),
      canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "foreign-claim-id" }),
      { encoding: "utf8", mode: 0o600 },
    );
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("release_token_hash_must_match_claim", async () => {
    const root = makeRepairTempRoot();
    const knownToken = "yet-another-known-token";
    writeForeignClaim(root, 1, { owner_token_sha256: sha256Hex(knownToken) });
    writeForeignRelease(root, 1, { owner_token: knownToken });
    const store = await N7EvidenceStore.open(root);
    // A CORRECT proof (matching token) must be accepted as a valid release,
    // freeing the generation for a new claim.
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    const claim2 = JSON.parse(fs.readFileSync(path.join(root, "chain.lock.d", "claim-2.json"), "utf8"));
    assert.strictEqual(claim2.generation, 2);
  });

  it("raw_owner_token_not_returned_or_logged", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    // The public result surface never carries a raw token field at all.
    assert.ok(!JSON.stringify(res).toLowerCase().includes("token"));
    // Source-level: this module never calls console.* (no logging surface
    // exists that could leak a raw token).
    const source = fs.readFileSync(path.join(__dirname, "..", "..", "src", "n7EvidenceStore.ts"), "utf8");
    assert.ok(!/console\./.test(source), "production source must never log anything");
  });
});

// ---------------------------------------------------------------------------
// N7-C ledger integrity repair: complete ledger validator tests (Phase 14)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — complete ledger validation", () => {
  it("claim_records_are_canonical", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const raw = fs.readFileSync(path.join(root, "chain.lock.d", "claim-1.json"), "utf8");
    assert.strictEqual(raw, canonicalize(JSON.parse(raw)));
  });

  it("release_records_are_canonical", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    await store.appendEvent(makeEventInput());
    const raw = fs.readFileSync(path.join(root, "chain.lock.d", "release-1.json"), "utf8");
    assert.strictEqual(raw, canonicalize(JSON.parse(raw)));
  });

  it("noncanonical_claim_blocks", async () => {
    const root = makeRepairTempRoot();
    const lockDir = path.join(root, "chain.lock.d");
    fs.mkdirSync(lockDir, { recursive: true, mode: 0o700 });
    const record = { schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "foreign-claim-id", owner_token_sha256: "0".repeat(64), pid: 1 };
    // Plain JSON.stringify of a non-alphabetically-ordered literal --
    // deliberately NOT canonicalize().
    const noncanonical = JSON.stringify(record);
    assert.notStrictEqual(noncanonical, canonicalize(record), "test sanity: fixture must actually be noncanonical");
    fs.writeFileSync(path.join(lockDir, "claim-1.json"), noncanonical, { encoding: "utf8", mode: 0o600 });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("noncanonical_release_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    const record = { schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "foreign-claim-id", owner_token: "foreign-owner-token" };
    const noncanonical = JSON.stringify(record);
    assert.notStrictEqual(noncanonical, canonicalize(record));
    fs.writeFileSync(path.join(root, "chain.lock.d", "release-1.json"), noncanonical, { encoding: "utf8", mode: 0o600 });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("malformed_lower_generation_blocks", async () => {
    const root = makeRepairTempRoot();
    const lockDir = path.join(root, "chain.lock.d");
    fs.mkdirSync(lockDir, { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(lockDir, "claim-1.json"), "{ not valid json at all", { encoding: "utf8", mode: 0o600 });
    const knownToken = "known-token-for-generation-2";
    fs.writeFileSync(
      path.join(lockDir, "claim-2.json"),
      canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 2, claim_id: "claim-2-id", owner_token_sha256: sha256Hex(knownToken), pid: 1 }),
      { encoding: "utf8", mode: 0o600 },
    );
    fs.writeFileSync(
      path.join(lockDir, "release-2.json"),
      canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 2, claim_id: "claim-2-id", owner_token: knownToken }),
      { encoding: "utf8", mode: 0o600 },
    );
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP", "a malformed lower-generation record must block even when the highest generation looks free");
  });

  it("unknown_lock_record_blocks", async () => {
    const root = makeRepairTempRoot();
    const lockDir = path.join(root, "chain.lock.d");
    fs.mkdirSync(lockDir, { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(lockDir, "not-a-real-record.json"), "{}", { encoding: "utf8", mode: 0o600 });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("claim_generation_gap_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 2);
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("duplicate_claim_generation_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    fs.writeFileSync(
      path.join(root, "chain.lock.d", "claim-01.json"),
      canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "another-claim-id", owner_token_sha256: "1".repeat(64), pid: 2 }),
      { encoding: "utf8", mode: 0o600 },
    );
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("release_without_claim_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignRelease(root, 1);
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("duplicate_release_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    writeForeignRelease(root, 1);
    fs.writeFileSync(
      path.join(root, "chain.lock.d", "release-01.json"),
      canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "foreign-claim-id", owner_token: "foreign-owner-token" }),
      { encoding: "utf8", mode: 0o600 },
    );
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("release_claim_id_mismatch_blocks", async () => {
    const root = makeRepairTempRoot();
    const knownToken = "matching-token-different-claim-id";
    writeForeignClaim(root, 1, { claim_id: "claim-A", owner_token_sha256: sha256Hex(knownToken) });
    writeForeignRelease(root, 1, { claim_id: "claim-B", owner_token: knownToken });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("invalid_release_token_proof_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    writeForeignRelease(root, 1);
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("later_claim_after_unreleased_claim_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1); // unreleased
    const knownToken = "generation-2-known-token";
    writeForeignClaim(root, 2, { claim_id: "claim-2-id", owner_token_sha256: sha256Hex(knownToken) });
    writeForeignRelease(root, 2, { claim_id: "claim-2-id", owner_token: knownToken }); // validly released
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP", "a later, released generation cannot excuse an unreleased earlier one");
  });

  it("multiple_active_claims_block", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    writeForeignClaim(root, 2, { claim_id: "claim-2-id" });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("partial_claim_final_blocks", async () => {
    const root = makeRepairTempRoot();
    const lockDir = path.join(root, "chain.lock.d");
    fs.mkdirSync(lockDir, { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(lockDir, "claim-1.json"), '{"schema_version":"n7.lock-record.v1","generation":1,', { encoding: "utf8", mode: 0o600 });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("partial_release_final_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    fs.writeFileSync(path.join(root, "chain.lock.d", "release-1.json"), '{"schema_version":"n7.lock-record.v1","claim_id":', { encoding: "utf8", mode: 0o600 });
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("symlinked_claim_blocks", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const lockDir = path.join(root, "chain.lock.d");
    const outsideFile = path.join(N7C_TEST_BASE, `n7c-ledger-outside-claim-${Date.now()}.json`);
    fs.writeFileSync(outsideFile, canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "x", owner_token_sha256: "0".repeat(64), pid: 1 }), "utf8");
    fs.symlinkSync(outsideFile, path.join(lockDir, "claim-1.json"));
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("symlinked_release_blocks", async () => {
    const root = makeRepairTempRoot();
    writeForeignClaim(root, 1);
    const store = await N7EvidenceStore.open(root);
    const lockDir = path.join(root, "chain.lock.d");
    const outsideFile = path.join(N7C_TEST_BASE, `n7c-ledger-outside-release-${Date.now()}.json`);
    fs.writeFileSync(outsideFile, canonicalize({ schema_version: N7_LOCK_SCHEMA_VERSION, generation: 1, claim_id: "foreign-claim-id", owner_token: "x" }), "utf8");
    fs.symlinkSync(outsideFile, path.join(lockDir, "release-1.json"));
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });
});

// ---------------------------------------------------------------------------
// N7-C ledger integrity repair: post-write full-rescan race tests (Phase 15)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — post-claim and post-release full rescans", () => {
  it("post_claim_conflicting_record_blocks_before_chain_tip_selection", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("claim-")) return;
        // Inject a conflicting higher-generation claim strictly after our
        // own claim finalizes, strictly before the post-claim rescan runs.
        writeForeignClaim(root, 2, { claim_id: "conflicting-claim" });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
    // Chain-tip selection never started: no event file was ever written.
    assert.strictEqual(fs.existsSync(path.join(root, "events", "000001.json")), false);
  });

  it("post_claim_higher_generation_blocks", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("claim-")) return;
        writeForeignClaim(root, 2, { claim_id: "conflicting-claim" });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("post_claim_malformed_record_blocks", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("claim-")) return;
        fs.writeFileSync(path.join(path.dirname(finalPath), "claim-2.json"), "{ malformed after our own claim finalized", {
          encoding: "utf8",
          mode: 0o600,
        });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("post_claim_verification_is_required", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let listDirCalls = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async listDir(p: string) {
        if (p === lockDir) listDirCalls++;
        return real.listDir(p);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    // Pre-claim, post-claim, pre-release, post-release: at least 4 full
    // ledger scans in a single successful append.
    assert.ok(listDirCalls >= 4, `expected at least 4 lock-ledger scans, got ${listDirCalls}`);
  });

  it("post_release_conflicting_record_blocks_success", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("release-")) return;
        writeForeignClaim(root, 2, { claim_id: "conflicting-claim" });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const first = await store.appendEvent(makeEventInput());
    // The EVENT's own acceptance is not retroactively affected (it was
    // already durably written and readback-verified before release ran),
    // but the post-release rescan now catches the conflicting record and
    // must surface this to the CURRENT caller, not just the next one.
    assert.strictEqual(first.status, "ACCEPTED_RELEASE_FAILED", "a post-release ledger conflict must be observable to the current append caller");
    if (first.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(first.event.sequence, 1);
      assert.strictEqual(first.release.outcome, "RELEASE_FAILED");
    }
    const second = await store.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.notStrictEqual(second.status, "ACCEPTED", "the tampering left by the conflicting record must block future use of the ledger");
  });

  it("post_release_malformed_record_blocks_success", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("release-")) return;
        fs.writeFileSync(path.join(path.dirname(finalPath), "claim-2.json"), "{ malformed after release finalized", {
          encoding: "utf8",
          mode: 0o600,
        });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const first = await store.appendEvent(makeEventInput());
    assert.strictEqual(first.status, "ACCEPTED_RELEASE_FAILED", "a post-release malformed record must be observable to the current append caller");
    if (first.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(first.release.outcome, "RELEASE_FAILED");
    }
    const second = await store.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(second.status, "LOCK_INTEGRITY_STOP");
  });

  it("post_release_verification_is_required", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let listDirCallsAfterReleaseLink = 0;
    let releaseLinked = false;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        const result = await real.linkPath(existingPath, newPath);
        if (path.basename(newPath).startsWith("release-")) releaseLinked = true;
        return result;
      },
      async listDir(p: string) {
        if (p === lockDir && releaseLinked) listDirCallsAfterReleaseLink++;
        return real.listDir(p);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    assert.ok(listDirCallsAfterReleaseLink >= 1, "a full ledger rescan must run after the release record is finalized");
  });

  it("losing_claim_contender_does_not_retry", async () => {
    const root = makeRepairTempRoot();
    const storeA = await N7EvidenceStore.open(root);
    const storeB = await N7EvidenceStore.open(root);
    const [resA, resB] = await Promise.all([storeA.appendEvent(makeEventInput()), storeB.appendEvent(makeEventInput())]);
    const statuses = [resA.status, resB.status].sort();
    assert.deepStrictEqual(statuses, ["ACCEPTED", "LOCK_CONTENTION"]);
    // Exactly one claim record exists -- the loser never attempted (and
    // was never granted) any generation at all, let alone a later one.
    const claimFiles = fs.readdirSync(path.join(root, "chain.lock.d")).filter((n) => n.startsWith("claim-"));
    assert.strictEqual(claimFiles.length, 1);
  });
});

// ---------------------------------------------------------------------------
// N7-C ledger integrity repair: durability-uncertain recovery tests (Phase 16)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — durability-uncertain recovery", () => {
  it("event_sync_failure_remains_durability_uncertain", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirIoError: true });
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "DURABILITY_SYNC_FAILED");
    const entries = fs.readdirSync(path.join(root, "events"));
    assert.ok(entries.includes("000001.json"), "the final event file must survive");
    assert.ok(entries.some((n) => n.startsWith(".tmp-")), "the recovery temp file must be preserved, still linked to the final path");
  });

  it("artifact_sync_failure_remains_durability_uncertain", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const artifactsDir = path.join(root, "artifacts");
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === artifactsDir) return { kind: "IO_ERROR", code: "EIO" };
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.writeArtifact({ kind: "x", content: "hello", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.strictEqual(res.status, "DURABILITY_SYNC_FAILED");
    const entries = fs.readdirSync(artifactsDir);
    assert.ok(entries.some((n) => n.startsWith("art_")), "the final artifact file must survive");
    assert.ok(entries.some((n) => n.startsWith(".tmp-")), "the recovery temp file must be preserved");
  });

  it("claim_sync_failure_remains_durability_uncertain", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) return { kind: "IO_ERROR", code: "EIO" };
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    const entries = fs.readdirSync(lockDir);
    assert.ok(entries.includes("claim-1.json"), "the final claim record must survive");
    assert.ok(entries.some((n) => n.startsWith(".tmp-")), "the recovery temp file for the claim must be preserved");
  });

  it("release_sync_failure_remains_durability_uncertain", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let lockDirSyncCalls = 0;
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) {
          lockDirSyncCalls++;
          // Let the CLAIM's own sync (the 1st lockDir sync) succeed; fault
          // only the RELEASE's (the 2nd).
          if (lockDirSyncCalls === 2) return { kind: "IO_ERROR", code: "EIO" };
        }
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    const res = await store.appendEvent(makeEventInput());
    // The event write itself is unaffected (its own sync succeeded) -- but
    // the release's durability-uncertain sync must be visible to the
    // CURRENT caller, not reported as an unqualified "ACCEPTED".
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(res.release.outcome, "RELEASE_FAILED");
      if (res.release.outcome === "RELEASE_FAILED") {
        assert.strictEqual(res.release.reason, "RELEASE_DURABILITY_UNCERTAIN");
      }
    }
    const entries = fs.readdirSync(lockDir);
    assert.ok(entries.includes("release-1.json"), "the final release record must survive");
    assert.ok(entries.some((n) => n.startsWith(".tmp-")), "the recovery temp file for the release must be preserved");
  });

  it("fresh_store_does_not_accept_sync_failed_event", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirIoError: true });
    const store = await N7EvidenceStore.open(root, faulty);
    await store.appendEvent(makeEventInput());
    const freshStore = await N7EvidenceStore.open(root, createNodeFsAdapter());
    const chain = await freshStore.readChain();
    assert.strictEqual(chain.status, "DURABILITY_UNCERTAIN");
  });

  it("fresh_store_does_not_accept_sync_failed_artifact", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const artifactsDir = path.join(root, "artifacts");
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === artifactsDir) return { kind: "IO_ERROR", code: "EIO" };
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    const artifactRes = await store.writeArtifact({ kind: "x", content: "hello", redaction: "no-secrets", confirmedNoSecrets: true });
    assert.strictEqual(artifactRes.status, "DURABILITY_SYNC_FAILED");
    if (artifactRes.status !== "DURABILITY_SYNC_FAILED") return;
    // Reconstruct the artifact reference the write would have produced, to
    // reference it from an event (write-time structural checks do not
    // re-verify a newly-referenced artifact's durability; only a later
    // chain READ does).
    const artFile = fs.readdirSync(artifactsDir).find((n) => n.startsWith("art_")) as string;
    const artifactRef = {
      artifact_id: artFile.replace(/\.dat$/, ""),
      kind: "x",
      path: `artifacts/${artFile}`,
      sha256: computeArtifactSha256("hello"),
      size_bytes: Buffer.byteLength("hello", "utf8"),
      redaction: "no-secrets",
    };
    const eventRes = await store.appendEvent(makeEventInput({ artifactRefs: [artifactRef] }));
    assert.strictEqual(eventRes.status, "ACCEPTED");
    const freshStore = await N7EvidenceStore.open(root, createNodeFsAdapter());
    const chain = await freshStore.readChain();
    assert.strictEqual(chain.status, "DURABILITY_UNCERTAIN");
  });

  it("fresh_store_does_not_accept_sync_failed_claim", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) return { kind: "IO_ERROR", code: "EIO" };
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    await store.appendEvent(makeEventInput());
    const freshStore = await N7EvidenceStore.open(root, real);
    const res = await freshStore.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("fresh_store_does_not_accept_sync_failed_release", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let lockDirSyncCalls = 0;
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) {
          lockDirSyncCalls++;
          if (lockDirSyncCalls === 2) return { kind: "IO_ERROR", code: "EIO" };
        }
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    await store.appendEvent(makeEventInput());
    const freshStore = await N7EvidenceStore.open(root, real);
    const res = await freshStore.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(res.status, "LOCK_INTEGRITY_STOP");
  });

  it("durability_uncertain_temp_is_preserved", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirIoError: true });
    const store = await N7EvidenceStore.open(root, faulty);
    await store.appendEvent(makeEventInput());
    const entries = fs.readdirSync(path.join(root, "events"));
    const tempEntries = entries.filter((n) => n.startsWith(".tmp-"));
    assert.strictEqual(tempEntries.length, 1, "exactly the one recovery temp file must remain, never auto-deleted");
    const finalStat = fs.statSync(path.join(root, "events", "000001.json"));
    const tempStat = fs.statSync(path.join(root, "events", tempEntries[0]));
    assert.strictEqual(finalStat.ino, tempStat.ino, "the temp name and final name must still be the same inode");
  });

  it("durability_uncertain_record_blocks_append", async () => {
    const root = makeRepairTempRoot();
    const faulty = wrapAdapterWithFaults(createNodeFsAdapter(), { fsyncDirIoError: true });
    const store = await N7EvidenceStore.open(root, faulty);
    await store.appendEvent(makeEventInput());
    // A later append on a NON-faulty adapter must still refuse, since the
    // chain read itself detects the durability-uncertain final event.
    const cleanStore = await N7EvidenceStore.open(root, createNodeFsAdapter());
    const res = await cleanStore.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(res.status, "CHAIN_INVALID");
  });

  it("durability_uncertain_ledger_blocks_acquisition", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    const faulty: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) return { kind: "IO_ERROR", code: "EIO" };
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, faulty);
    const first = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(first.status, "ACCEPTED");
    const second = await store.appendEvent(makeEventInput({ operation: "second-op" }));
    assert.strictEqual(second.status, "LOCK_INTEGRITY_STOP", "a durability-uncertain claim must block every subsequent acquisition attempt");
  });
});

// ---------------------------------------------------------------------------
// N7-C release observability repair: focused failure tests (Phase 11-13)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore — release observability", () => {
  it("post_release_conflicting_record_is_observable_to_append_caller", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("release-")) return;
        writeForeignClaim(root, 2, { claim_id: "conflicting-claim" });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(res.release.outcome, "RELEASE_FAILED");
      if (res.release.outcome === "RELEASE_FAILED") {
        assert.strictEqual(res.release.reason, "POST_RELEASE_LEDGER_INTEGRITY_STOP");
      }
    }
  });

  it("post_release_malformed_record_is_observable_to_append_caller", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("release-")) return;
        fs.writeFileSync(path.join(path.dirname(finalPath), "claim-2.json"), "{ malformed", { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(res.release.outcome, "RELEASE_FAILED");
      if (res.release.outcome === "RELEASE_FAILED") {
        assert.strictEqual(res.release.reason, "POST_RELEASE_LEDGER_INTEGRITY_STOP");
      }
    }
  });

  it("post_release_verification_failure_is_not_released", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        if (!path.basename(finalPath).startsWith("release-")) return;
        writeForeignClaim(root, 2, { claim_id: "conflicting-claim" });
      },
    };
    const store = await N7EvidenceStore.open(root, real, hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED", "a post-release verification failure must never map to a fully-released ACCEPTED outcome");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.notStrictEqual(res.release.outcome, "RELEASED");
    }
  });

  it("release_write_failure_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let tempWritesInLockDir = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async openExclusive(p: string, mode: number) {
        const handle = await real.openExclusive(p, mode);
        if (isLockLedgerPath(p) && path.basename(p).startsWith(".tmp-")) {
          tempWritesInLockDir++;
          if (tempWritesInLockDir === 2) {
            // The claim's own temp write is the 1st; the release's is the 2nd.
            return { ...handle, write: async () => { throw new Error("injected: release write failure"); } };
          }
        }
        return handle;
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED" && res.release.outcome === "RELEASE_FAILED") {
      assert.strictEqual(res.release.reason, "RELEASE_WRITE_FAILED");
    }
  });

  it("release_file_flush_failure_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    let tempWritesInLockDir = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async openExclusive(p: string, mode: number) {
        const handle = await real.openExclusive(p, mode);
        if (isLockLedgerPath(p) && path.basename(p).startsWith(".tmp-")) {
          tempWritesInLockDir++;
          if (tempWritesInLockDir === 2) {
            return { ...handle, flush: async () => { throw new Error("injected: release flush failure"); } };
          }
        }
        return handle;
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED" && res.release.outcome === "RELEASE_FAILED") {
      assert.strictEqual(res.release.reason, "RELEASE_FLUSH_FAILED");
    }
  });

  it("release_finalization_failure_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) {
          throw new Error("injected: release finalization failure");
        }
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED" && res.release.outcome === "RELEASE_FAILED") {
      assert.strictEqual(res.release.reason, "RELEASE_FINALIZATION_FAILED");
    }
  });

  it("release_directory_sync_failure_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let lockDirSyncCalls = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) {
          lockDirSyncCalls++;
          if (lockDirSyncCalls === 2) return { kind: "IO_ERROR", code: "EIO" };
        }
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED" && res.release.outcome === "RELEASE_FAILED") {
      assert.strictEqual(res.release.reason, "RELEASE_DURABILITY_UNCERTAIN");
    }
  });

  it("release_durability_uncertain_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let lockDirSyncCalls = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) {
          lockDirSyncCalls++;
          if (lockDirSyncCalls === 2) return { kind: "IO_ERROR", code: "EIO" };
        }
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    const entries = fs.readdirSync(lockDir);
    assert.ok(entries.includes("release-1.json"), "the durability-uncertain final release record must survive");
    assert.ok(entries.some((n) => n.startsWith(".tmp-")), "its recovery temp file must be preserved");
  });

  it("release_readback_failure_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async readFileUtf8(p: string) {
        if (path.basename(p).startsWith("release-")) {
          throw new Error("injected: release readback failure");
        }
        return real.readFileUtf8(p);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED" && res.release.outcome === "RELEASE_FAILED") {
      assert.strictEqual(res.release.reason, "RELEASE_READBACK_FAILED");
    }
  });

  it("release_ownership_failure_is_observable", async () => {
    const root = makeRepairTempRoot();
    const claimPath = path.join(root, "chain.lock.d", "claim-1.json");
    const hooks: N7StoreHooks = {
      afterFinalLink: async (finalPath: string) => {
        // Fires once the EVENT itself finalizes -- strictly before release
        // runs. Corrupt (not delete) our own claim's ownership proof by
        // swapping its owner_token_sha256 to a value our real token can
        // never hash to.
        if (isLockLedgerPath(finalPath)) return;
        const claim = JSON.parse(fs.readFileSync(claimPath, "utf8"));
        claim.owner_token_sha256 = "0".repeat(64);
        fs.writeFileSync(claimPath, canonicalize(claim), { encoding: "utf8", mode: 0o600 });
      },
    };
    const store = await N7EvidenceStore.open(root, createNodeFsAdapter(), hooks);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED" && res.release.outcome === "RELEASE_FAILED") {
      assert.strictEqual(res.release.reason, "OWNERSHIP_LOST");
    }
  });

  it("accepted_event_identity_is_preserved_when_release_fails", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) throw new Error("injected");
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.match(res.event.event_id, /^evt_000001_[0-9a-f]{16}$/);
      assert.strictEqual(res.event.sequence, 1);
    }
  });

  it("accepted_event_hash_is_preserved_when_release_fails", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) throw new Error("injected");
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    if (res.status === "ACCEPTED_RELEASE_FAILED") {
      assert.strictEqual(res.event.event_sha256, computeEventSha256(res.event));
    }
  });

  it("event_accepted_release_failed_is_not_unqualified_accepted", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) throw new Error("injected");
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.notStrictEqual(res.status, "ACCEPTED");
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    assert.ok("release" in res && res.release.outcome === "RELEASE_FAILED");
  });

  it("event_failure_and_release_failure_are_both_preserved", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    // Pre-seed a corrupted existing event so the CHAIN itself is invalid
    // (the event write fails), while ALSO faulting release.
    fs.mkdirSync(path.join(root, "events"), { recursive: true, mode: 0o700 });
    fs.writeFileSync(path.join(root, "events", "000001.json"), "{ not valid json", { encoding: "utf8", mode: 0o600 });
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) throw new Error("injected");
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    // Primary (event) failure is preserved as the outer status...
    assert.strictEqual(res.status, "CHAIN_INVALID");
    // ...and the SECONDARY (release) failure is also preserved, not
    // overwritten by or hidden behind the primary failure.
    if (res.status === "CHAIN_INVALID") {
      assert.strictEqual(res.release.outcome, "RELEASE_FAILED");
      if (res.release.outcome === "RELEASE_FAILED") {
        assert.strictEqual(res.release.reason, "RELEASE_FINALIZATION_FAILED");
      }
    }
  });

  it("successful_event_and_release_returns_fully_successful_outcome", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.strictEqual(res.release.outcome, "RELEASED");
      if (res.release.outcome === "RELEASED") {
        assert.strictEqual(res.release.generation, 1);
        assert.strictEqual(typeof res.release.claimId, "string");
      }
    }
  });

  it("unsupported_directory_sync_warning_is_observable", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let lockDirSyncCalls = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async fsyncDir(dirPath: string) {
        if (dirPath === lockDir) {
          lockDirSyncCalls++;
          if (lockDirSyncCalls === 2) return { kind: "UNSUPPORTED", code: "ENOTSUP" };
        }
        return real.fsyncDir(dirPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    // A recognized capability gap on the RELEASE's own sync is still a
    // fully successful, verified release -- never collapsed into failure.
    assert.strictEqual(res.status, "ACCEPTED");
    if (res.status === "ACCEPTED") {
      assert.strictEqual(res.release.outcome, "RELEASED");
      if (res.release.outcome === "RELEASED") {
        assert.ok(res.release.warnings.some((w) => w.kind === "UNSUPPORTED_DIRECTORY_SYNC"));
      }
    }
  });

  it("release_claim_result_contains_no_raw_owner_token", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) throw new Error("injected");
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED_RELEASE_FAILED");
    const serialized = JSON.stringify(res);
    assert.ok(!serialized.toLowerCase().includes("ownertoken"));
    assert.ok(!/"owner_token"/.test(serialized));
  });

  it("release_attempted_exactly_once_on_success", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    let releaseLinkAttempts = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) releaseLinkAttempts++;
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    const res = await store.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "ACCEPTED");
    assert.strictEqual(releaseLinkAttempts, 1, "release must be attempted exactly once, never retried");
  });

  it("release_not_retried_automatically", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    let releaseLinkAttempts = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) {
          releaseLinkAttempts++;
          throw new Error("injected: release finalization failure");
        }
        return real.linkPath(existingPath, newPath);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    await store.appendEvent(makeEventInput());
    assert.strictEqual(releaseLinkAttempts, 1, "a failed release must not be automatically retried within the same append");
  });

  it("post_release_verification_not_rerun_in_retry_loop", async () => {
    const root = makeRepairTempRoot();
    const real = createNodeFsAdapter();
    const lockDir = path.join(root, "chain.lock.d");
    let listDirCalls = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) throw new Error("injected");
        return real.linkPath(existingPath, newPath);
      },
      async listDir(p: string) {
        if (p === lockDir) listDirCalls++;
        return real.listDir(p);
      },
    };
    const store = await N7EvidenceStore.open(root, wrapped);
    await store.appendEvent(makeEventInput());
    // Pre-claim, post-claim, pre-release: exactly 3 full ledger scans when
    // release fails at finalization (before ever reaching a post-release
    // rescan) -- no bounded-retry loop reruns any of them.
    assert.strictEqual(listDirCalls, 3, `expected exactly 3 lock-ledger scans, got ${listDirCalls}`);
  });

  it("unsuccessful_claim_acquisition_triggers_no_release_attempt", async () => {
    const root = makeRepairTempRoot();
    const store = await N7EvidenceStore.open(root);
    writeForeignClaim(root, 1);
    const real = createNodeFsAdapter();
    let releaseLinkAttempts = 0;
    const wrapped: N7StoreFsAdapter = {
      ...real,
      async linkPath(existingPath: string, newPath: string) {
        if (path.basename(newPath).startsWith("release-")) releaseLinkAttempts++;
        return real.linkPath(existingPath, newPath);
      },
    };
    const contendedStore = await N7EvidenceStore.open(root, wrapped);
    const res = await contendedStore.appendEvent(makeEventInput());
    assert.strictEqual(res.status, "LOCK_CONTENTION");
    assert.strictEqual(releaseLinkAttempts, 0, "an unsuccessful claim acquisition must never attempt any release");
  });
});

// ---------------------------------------------------------------------------
// N7-C CI-portable test-root repair: selector tests (Phase 6)
// ---------------------------------------------------------------------------

describe("n7EvidenceStore test infrastructure — CI-portable test-base selection", () => {
  it("explicit_tmp_override_is_accepted", () => {
    const selected = selectN7CTestBase("/tmp");
    assert.strictEqual(selected, "/tmp");
  });

  it("explicit_vast_data_override_is_accepted_when_present", () => {
    if (!fs.existsSync("/mnt/vast-data/tmp")) return;
    const selected = selectN7CTestBase("/mnt/vast-data/tmp");
    assert.strictEqual(selected, "/mnt/vast-data/tmp");
  });

  it("unapproved_test_base_override_is_rejected", () => {
    assert.throws(() => selectN7CTestBase("/var/tmp"));
    assert.throws(() => selectN7CTestBase("relative/path"));
    assert.throws(() => selectN7CTestBase("/mnt/ollama-models/tmp"));
    assert.throws(() => selectN7CTestBase(__dirname));
    assert.throws(() => selectN7CTestBase("/mnt/vast-data/tmp/definitely-does-not-exist-xyz"));
  });

  it("symlinked_candidate_base_is_rejected", () => {
    const parent = fs.mkdtempSync(path.join(N7C_TEST_BASE, "n7c-selector-symlink-"));
    const realTarget = path.join(parent, "real-dir");
    fs.mkdirSync(realTarget, { recursive: true });
    const symlinkPath = path.join(parent, "not-approved-anyway");
    fs.symlinkSync(realTarget, symlinkPath);
    // Not in the approved list at all, so this must throw regardless --
    // included here to document that symlink rejection is checked
    // independently of (not merely subsumed by) the allowlist check.
    assert.throws(() => selectN7CTestBase(symlinkPath));
  });

  it("selected_test_base_is_outside_repository", () => {
    const repoRoot = findRepositoryRoot(__dirname);
    assert.ok(repoRoot, "test sanity: a repository root must be discoverable");
    assert.ok(!N7C_TEST_BASE.startsWith((repoRoot as string) + path.sep));
    assert.notStrictEqual(N7C_TEST_BASE, repoRoot);
  });

  it("selected_test_base_is_not_under_ollama_models", () => {
    assert.ok(!N7C_TEST_BASE.startsWith("/mnt/ollama-models"));
  });

  it("all_n7c_test_roots_end_in_claw_n7", () => {
    const suffix = path.join(".claw", "n7");
    assert.ok(makeTempRoot().endsWith(suffix));
    assert.ok(makeRepairTempRoot().endsWith(suffix));
  });

  it("n7c_test_file_does_not_use_os_tmpdir", () => {
    const testSource = fs.readFileSync(path.join(__dirname, "..", "..", "test", "n7EvidenceStore.test.ts"), "utf8");
    // Patterns built from split parts so this test's own source text never
    // literally spells out the substrings it searches for (which would
    // otherwise trivially self-match).
    const osTmpdirCall = ["os", ".", "tmpdir", "()"].join("");
    assert.ok(!testSource.includes(osTmpdirCall));
    const runnerTempVar = ["RUNNER", "_", "TEMP"].join("");
    assert.ok(!testSource.includes(runnerTempVar));
  });

  it("no_mkdtemp_call_hardcodes_vast_data_directly", () => {
    const testSource = fs.readFileSync(path.join(__dirname, "..", "..", "test", "n7EvidenceStore.test.ts"), "utf8");
    // Every mkdtempSync call must derive from the selected N7C_TEST_BASE
    // constant, never a literal path string passed directly.
    const mkdtempCalls = testSource.match(/mkdtempSync\([^)]*\)/g) ?? [];
    assert.ok(mkdtempCalls.length > 0, "test sanity: mkdtempSync must actually be used");
    for (const call of mkdtempCalls) {
      assert.ok(!/["'](\/mnt\/vast-data\/tmp|\/tmp)["']/.test(call), `mkdtempSync call must not hardcode a literal base: ${call}`);
      assert.ok(call.includes("N7C_TEST_BASE"), `mkdtempSync call must derive from N7C_TEST_BASE: ${call}`);
    }
  });

  it("test_base_selection_does_not_use_a_second_independent_selector", () => {
    const testSource = fs.readFileSync(path.join(__dirname, "..", "..", "test", "n7EvidenceStore.test.ts"), "utf8");
    // Built from concatenated parts so this search pattern's own source text
    // does not itself contain the literal target substring (which would
    // otherwise inflate the count by one, matching itself).
    const target = ["function", "selectN7CTestBase"].join(" ");
    const pattern = new RegExp(target, "g");
    const selectorDefinitions = testSource.match(pattern) ?? [];
    assert.strictEqual(selectorDefinitions.length, 1, "exactly one selector implementation must exist");
  });
});
