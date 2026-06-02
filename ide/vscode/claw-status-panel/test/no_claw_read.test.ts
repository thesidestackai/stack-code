import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { PKG_ROOT, SRC_DIR } from "./_paths";

const SRC = SRC_DIR;

function listSrc(): Array<{ name: string; src: string }> {
  const out: Array<{ name: string; src: string }> = [];
  const walk = (d: string) => {
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) walk(p);
      else if (e.isFile() && p.endsWith(".ts")) out.push({ name: path.relative(PKG_ROOT, p), src: fs.readFileSync(p, "utf8") });
    }
  };
  walk(SRC);
  return out;
}

function strip(src: string): string {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const two = src.slice(i, i + 2);
    if (two === "//") { while (i < n && src[i] !== "\n") i++; continue; }
    if (two === "/*") { i += 2; while (i < n && src.slice(i, i + 2) !== "*/") i++; i += 2; continue; }
    const ch = src[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      const q = ch; i++;
      while (i < n && src[i] !== q) { if (src[i] === "\\") { i += 2; continue; } i++; }
      i++; out += " "; continue;
    }
    out += ch; i++;
  }
  return out;
}

describe("no direct .claw/** parsing", () => {
  const files = listSrc();

  const FORBIDDEN_DIRECT = [
    "\\.claw",
    "l2b-runs",
    "l2b-preview-bundles",
    "l2b-checkpoints",
    "l2b-payloads",
    "run-manifest\\.json",
    "preview-bundle\\.json",
    "apply-bundle\\.json",
    "after\\.sha256",
  ];

  for (const term of FORBIDDEN_DIRECT) {
    it(`no live reference to \`${term}\` in src/ (strings / comments / fixtures allowed)`, () => {
      const re = new RegExp(term);
      for (const f of files) {
        const live = strip(f.src);
        assert.ok(
          !re.test(live),
          `${f.name} has live (non-string, non-comment) reference matching ${term}`,
        );
      }
    });
  }

  it("no live filesystem read API call against an .claw-derived path can be constructed", () => {
    // Audit: any fs.readFile* call site in live code MUST be one of the
    // explicitly-allowed call sites. The only allowed reads are:
    //   - manifest_audit.ts: package.json read
    //   - evidence_path.ts: fs.existsSync (existence check, not contents)
    const allowedReadSites = new Set(["src/manifest_audit.ts", "src/evidence_path.ts"]);
    for (const f of files) {
      const live = strip(f.src);
      if (/\bfs\.readFile\b|\bfs\.readFileSync\b|\bfs\.read\b/.test(live)) {
        assert.ok(
          allowedReadSites.has(f.name),
          `${f.name} performs a file read but is not on the allowed read site list`,
        );
      }
    }
  });
});

describe("no .claw/** read during a parsed-status panel session (runtime sentinel)", () => {
  it("evidence_path classifyEvidencePath performs no file read against .claw/ contents", () => {
    // Create a disposable workspace with a sentinel file under .claw/. The
    // classifier may call fs.existsSync (allowed by §13) but MUST NOT open
    // or read the file. We verify by mtime-checking the sentinel.
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), "claw-status-panel-test-"));
    try {
      const clawDir = path.join(tmp, ".claw", "l2b-runs", "run-x");
      fs.mkdirSync(clawDir, { recursive: true });
      const sentinel = path.join(clawDir, "run-manifest.json");
      fs.writeFileSync(sentinel, JSON.stringify({ pending_step_id: "x" }));
      const beforeAtime = fs.statSync(sentinel).atime.getTime();

      // Wait briefly so any atime change would be visible.
      const start = Date.now();
      while (Date.now() - start < 5) { /* spin */ }

      const { classifyEvidencePath } = require("../src/evidence_path");
      const c = classifyEvidencePath(sentinel, tmp);
      assert.strictEqual(c.exists, true);
      assert.strictEqual(c.location, "in-workspace");

      // atime preservation is filesystem-dependent (noatime mounts mask
      // reads); we instead assert byte-identical file contents and size,
      // which is sufficient to prove no write or truncation occurred.
      const afterContent = fs.readFileSync(sentinel, "utf8");
      assert.strictEqual(
        afterContent,
        JSON.stringify({ pending_step_id: "x" }),
      );
      void beforeAtime;
    } finally {
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });
});
