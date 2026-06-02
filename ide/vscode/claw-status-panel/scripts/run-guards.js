#!/usr/bin/env node
// Static-grep guards for the claw-status-panel package.
//
// Source of record:
// - docs/a2-l3-ide-adapter-implementation-scope-card.md §13 (refresh-boundary)
// - docs/a2-l3-ide-adapter-implementation-scope-card.md §14 (clipboard)
// - docs/a2-l3-ide-adapter-implementation-scope-card.md §15 (no-write)
// - docs/a2-l3-ide-adapter-implementation-scope-card.md §16 (no-network)
// - docs/a2-l3-ide-adapter-implementation-scope-card.md §17 (no-.claw read)
// - docs/a2-l3-ide-adapter-implementation-scope-card.md §18 (secrets)
//
// Invocation: `node scripts/run-guards.js` from the package root.
// Exits non-zero on any violation. Tests in test/guards.test.ts exercise
// the same logic so the lane fails identically under `npm test`.

const path = require("path");
const fs = require("fs");

const ROOT = path.resolve(__dirname, "..");
const SRC = path.join(ROOT, "src");

function listSrcFiles() {
  const out = [];
  const walk = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, entry.name);
      if (entry.isDirectory()) {
        walk(p);
      } else if (entry.isFile() && p.endsWith(".ts")) {
        out.push(p);
      }
    }
  };
  walk(SRC);
  return out;
}

function stripCommentsAndStrings(source) {
  // Crude but adequate for our guards: remove // line comments and
  // /* */ block comments, then remove single/double/backtick string
  // literals. Anything that survives is "live" code we apply guards to.
  let out = "";
  let i = 0;
  const n = source.length;
  while (i < n) {
    const two = source.slice(i, i + 2);
    if (two === "//") {
      while (i < n && source[i] !== "\n") i++;
      continue;
    }
    if (two === "/*") {
      i += 2;
      while (i < n && source.slice(i, i + 2) !== "*/") i++;
      i += 2;
      continue;
    }
    const ch = source[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      const quote = ch;
      i++;
      while (i < n && source[i] !== quote) {
        if (source[i] === "\\") {
          i += 2;
          continue;
        }
        i++;
      }
      i++;
      out += " "; // keep tokenization separation
      continue;
    }
    out += ch;
    i++;
  }
  return out;
}

const NETWORK_PATTERNS = [
  /\baxios\b/,
  /\bnode-fetch\b/,
  /\bgot\b/,
  /\bsuperagent\b/,
  /\bky\b/,
  /\bundici\b/,
  /\bfetch\s*\(/,
  /\bXMLHttpRequest\b/,
  /\bWebSocket\b/,
  /\bnet\.createConnection\b/,
  /\bdgram\.createSocket\b/,
  /\bhttp\.request\b/,
  /\bhttps\.request\b/,
  /\bhttp\.get\b/,
  /\bhttps\.get\b/,
  /https?:\/\/\S+/,
  /\bollama\b/i,
  /\bbroker[_-]?url\b/i,
  /\btelemetry\b/i,
  /\banalytics\b/i,
  /\bopenExternal\b/,
];

const WATCHER_PATTERNS = [
  /\bchokidar\b/,
  /\bwatchman\b/,
  /\bcreateFileSystemWatcher\b/,
  /\bonDidChangeFiles\b/,
  /\bonDidCreateFiles\b/,
  /\bonDidDeleteFiles\b/,
  /\bonDidRenameFiles\b/,
  /\bfs\.watch\b/,
  /\bfs\.watchFile\b/,
  /\bonDidSaveTextDocument\b/,
  /\bonDidOpenTextDocument\b/,
  /\bonDidChangeTextDocument\b/,
  /\bonDidChangeWindowState\b/,
  /\bonDidChangeWorkspaceFolders\b/,
];

const POLLING_PATTERNS = [
  /\bsetInterval\s*\(/,
  /\bsetTimeout\s*\(/,
  /\bsetImmediate\s*\(/,
];

const WRITE_PATTERNS = [
  /\bfs\.writeFile\b/,
  /\bfs\.writeFileSync\b/,
  /\bfs\.appendFile\b/,
  /\bfs\.appendFileSync\b/,
  /\bfs\.createWriteStream\b/,
  /\bfs\.rename\b/,
  /\bfs\.renameSync\b/,
  /\bfs\.unlink\b/,
  /\bfs\.unlinkSync\b/,
  /\bfs\.rm\b/,
  /\bfs\.rmSync\b/,
  /\bfs\.rmdir\b/,
  /\bfs\.rmdirSync\b/,
  /\bfs\.mkdir\b/,
  /\bfs\.mkdirSync\b/,
  /\bworkspace\.fs\.writeFile\b/,
  /\bworkspace\.fs\.delete\b/,
  /\bworkspace\.fs\.rename\b/,
  /\bworkspace\.fs\.createDirectory\b/,
];

const CHAIN_WRITE_PATTERNS = [
  /\bclaw\s+plan\s+run\b/,
  /\bclaw\s+plan\s+approve\b/,
  /\bclaw\s+plan\s+apply-bundle\b/,
  /\bclaw\s+plan\s+apply\b/,
];

const CLAW_DIRECT_PATTERNS = [
  /\.claw\b/,
  /\bl2b-runs\b/,
  /\bl2b-preview-bundles\b/,
  /\bl2b-checkpoints\b/,
  /\bl2b-payloads\b/,
  /\brun-manifest\.json\b/,
  /\bpreview-bundle\.json\b/,
  /\bapply-bundle\.json\b/,
  /\bafter\.sha256\b/,
];

const SECRET_PATTERNS = [
  /\bSecretStorage\b/,
  /\bcontext\.secrets\b/,
  /\bPasswordSafe\b/,
];

const CLIPBOARD_COMPOSITE_PATTERNS = [
  // approval-line composition: `apply ${step_id} ${preview_sha256}`
  /apply\s+\$\{[^}]*step[^}]*\}\s+\$\{[^}]*preview[^}]*\}/,
  /['"]apply ['"]\s*\+\s*\w+\s*\+\s*['"] ['"]\s*\+\s*\w+/,
];

const HARNESS_DEPENDENCY_PATTERNS = [
  /\ba2-harness-adapter\b/,
  /\brust\/crates\/a2-harness-adapter\b/,
];

const violations = [];

function record(file, label, match) {
  violations.push(`${path.relative(ROOT, file)}: ${label}: ${match}`);
}

function check(file, source, label, patterns) {
  for (const re of patterns) {
    const m = source.match(re);
    if (m) {
      record(file, label, m[0]);
    }
  }
}

for (const file of listSrcFiles()) {
  const raw = fs.readFileSync(file, { encoding: "utf8" });
  const live = stripCommentsAndStrings(raw);
  check(file, live, "FORBIDDEN-NETWORK", NETWORK_PATTERNS);
  check(file, live, "FORBIDDEN-WATCHER", WATCHER_PATTERNS);
  check(file, live, "FORBIDDEN-POLLING", POLLING_PATTERNS);
  check(file, live, "FORBIDDEN-WRITE", WRITE_PATTERNS);
  check(file, live, "FORBIDDEN-CHAIN-WRITE", CHAIN_WRITE_PATTERNS);
  check(file, live, "FORBIDDEN-CLAW-DIRECT", CLAW_DIRECT_PATTERNS);
  check(file, live, "FORBIDDEN-SECRET-API", SECRET_PATTERNS);
  check(file, live, "FORBIDDEN-CLIPBOARD-COMPOSITE", CLIPBOARD_COMPOSITE_PATTERNS);
  check(file, live, "FORBIDDEN-HARNESS-DEPENDENCY", HARNESS_DEPENDENCY_PATTERNS);
}

if (violations.length > 0) {
  console.error("claw-status-panel guards FAILED:");
  for (const v of violations) {
    console.error(`  - ${v}`);
  }
  process.exit(1);
}

console.log("claw-status-panel guards PASS (" + listSrcFiles().length + " src files audited)");
