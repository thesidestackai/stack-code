#!/usr/bin/env node
// Static-grep guards for the a2-harness-panel package.
//
// Source of record: docs/a2-l4-ide-extension-panel-scope.md §7, §14, §16.
//
// Invocation: `node scripts/run-guards.js` from the package root.
// Exits non-zero on any violation. test/guards.test.ts exercises the same
// logic so the lane fails identically under `npm test`.
//
// What these guards enforce (panel-source level):
//   - no network / telemetry / broker / ollama / :11434 egress
//   - no filesystem-watcher / polling / background refresh
//   - no `fs` use at all (the panel reads/writes no file; the helper does the
//     read-only .claw inspection) -> enforces no-write + no-.claw-direct-read
//   - no secret-storage API
//   - no chain-write command literal in live code
//   - no approval-line composition
//   - ONLY src/helperRunner.ts may spawn a process; it may spawn (no exec/eval,
//     no shell:true). Every other module is process-spawn-free.

const path = require("path");
const fs = require("fs");

const ROOT = path.resolve(__dirname, "..");
const SRC = path.join(ROOT, "src");
const HELPER_RUNNER = path.join(SRC, "helperRunner.ts");

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
  // Remove // line comments, /* */ block comments, then single/double/backtick
  // string literals. Anything surviving is "live" code we apply guards to.
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
      out += " ";
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
  /\b11434\b/,
];

const WATCHER_PATTERNS = [
  /\bchokidar\b/,
  /\bwatchman\b/,
  /\bcreateFileSystemWatcher\b/,
  /\bonDidChangeFiles\b/,
  /\bonDidCreateFiles\b/,
  /\bonDidDeleteFiles\b/,
  /\bonDidRenameFiles\b/,
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

// The panel uses no `fs` at all. Forbidding the module enforces no-write and
// no-direct-.claw-read structurally: only the helper inspects artifacts.
const FS_PATTERNS = [
  /\bfs\./,
  /\brequire\(\s*fs\s*\)/, // (string stripped, but keep as belt-and-braces)
  /\bworkspace\.fs\b/,
  /\breadFileSync\b/,
  /\bwriteFileSync\b/,
  /\bcreateReadStream\b/,
  /\bcreateWriteStream\b/,
];

const SECRET_PATTERNS = [
  /\bSecretStorage\b/,
  /\bcontext\.secrets\b/,
  /\bPasswordSafe\b/,
];

// Chain-write command literals must not appear in LIVE code. They legitimately
// appear inside the helper's stdout at runtime (rendered as text) and inside
// string literals / comments (which are stripped before this check).
const CHAIN_WRITE_PATTERNS = [
  /\bclaw\s+plan\s+run\b/,
  /\bclaw\s+plan\s+approve\b/,
  /\bclaw\s+plan\s+apply-bundle\b/,
  /\bclaw\s+plan\s+apply\b/,
];

// Approval-line composition (apply <step_id> <preview_sha256>) must never be
// built in panel source.
const APPROVAL_COMPOSE_PATTERNS = [
  /apply\s+\$\{[^}]*step[^}]*\}\s+\$\{[^}]*preview[^}]*\}/i,
  /['"]apply ['"]\s*\+\s*\w+\s*\+\s*['"] ['"]\s*\+\s*\w+/i,
];

// Process spawning is allowed ONLY in helperRunner.ts. Everywhere else these
// are forbidden.
const PROCESS_PATTERNS = [
  /\bchild_process\b/,
  /\bspawn\s*\(/,
  /\bspawnSync\s*\(/,
  /\bexecFile\s*\(/,
  /\bexecFileSync\s*\(/,
  /\bexec\s*\(/,
  /\bexecSync\s*\(/,
  /\beval\s*\(/,
];

// Even in helperRunner.ts, these are forbidden: no exec/eval, no sync spawns,
// no shell.
const HELPER_RUNNER_FORBIDDEN = [
  /\bexec\s*\(/,
  /\bexecSync\s*\(/,
  /\bexecFile\s*\(/,
  /\bexecFileSync\s*\(/,
  /\bspawnSync\s*\(/,
  /\beval\s*\(/,
  /\bshell\s*:\s*true\b/,
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

const files = listSrcFiles();
for (const file of files) {
  const raw = fs.readFileSync(file, { encoding: "utf8" });
  const live = stripCommentsAndStrings(raw);
  check(file, live, "FORBIDDEN-NETWORK", NETWORK_PATTERNS);
  check(file, live, "FORBIDDEN-WATCHER", WATCHER_PATTERNS);
  check(file, live, "FORBIDDEN-POLLING", POLLING_PATTERNS);
  check(file, live, "FORBIDDEN-FS", FS_PATTERNS);
  check(file, live, "FORBIDDEN-SECRET-API", SECRET_PATTERNS);
  check(file, live, "FORBIDDEN-CHAIN-WRITE", CHAIN_WRITE_PATTERNS);
  check(file, live, "FORBIDDEN-APPROVAL-COMPOSE", APPROVAL_COMPOSE_PATTERNS);

  if (path.resolve(file) === path.resolve(HELPER_RUNNER)) {
    check(file, live, "FORBIDDEN-HELPER-RUNNER-API", HELPER_RUNNER_FORBIDDEN);
  } else {
    check(file, live, "FORBIDDEN-PROCESS-SPAWN", PROCESS_PATTERNS);
  }
}

// Structural assertion: helperRunner.ts must exist (the single spawn boundary).
if (!fs.existsSync(HELPER_RUNNER)) {
  violations.push("src/helperRunner.ts: MISSING single-spawn-boundary module");
}

// N6A structural assertions: ALLOWED_FLAGS for execution subcommands must not
// contain force-family flags, PR mark-ready/merge flags, or commit-amend flags.
// Read the raw source (not stripped) and pattern-match the specific array literals.
const helperSource = fs.readFileSync(HELPER_RUNNER, { encoding: "utf8" });

const pushFlagsM = helperSource.match(/"package-push"\s*:\s*\[([^\]]*)\]/);
if (pushFlagsM) {
  if (/['"]force['"]/i.test(pushFlagsM[1])) {
    violations.push("src/helperRunner.ts: FORBIDDEN-FORCE-PUSH-IN-HELPER: force-family flag in ALLOWED_FLAGS[\"package-push\"]");
  }
} else {
  violations.push("src/helperRunner.ts: N6A-MISSING: ALLOWED_FLAGS[\"package-push\"] entry not found");
}

const prFlagsM = helperSource.match(/"package-pr"\s*:\s*\[([^\]]*)\]/);
if (prFlagsM) {
  if (/['"](?:ready|approve|merge)['"]/i.test(prFlagsM[1])) {
    violations.push("src/helperRunner.ts: FORBIDDEN-PR-MARK-READY-IN-HELPER: ready/approve/merge flag in ALLOWED_FLAGS[\"package-pr\"]");
  }
} else {
  violations.push("src/helperRunner.ts: N6A-MISSING: ALLOWED_FLAGS[\"package-pr\"] entry not found");
}

const commitFlagsM = helperSource.match(/"package-commit"\s*:\s*\[([^\]]*)\]/);
if (commitFlagsM) {
  if (/['"](?:amend|all)['"]/i.test(commitFlagsM[1])) {
    violations.push("src/helperRunner.ts: FORBIDDEN-COMMIT-AMEND-IN-HELPER: amend/all flag in ALLOWED_FLAGS[\"package-commit\"]");
  }
} else {
  violations.push("src/helperRunner.ts: N6A-MISSING: ALLOWED_FLAGS[\"package-commit\"] entry not found");
}

// D7 structural assertion (N6): every N6 execution run button (identified by
// the literal CSS class "n6-run-btn" in the render.ts source) MUST carry
// data-n6-token-required="true" in the same source region (within 300 chars).
//
// Why "n6-run-btn" and not "n6Run*": render.ts builds the button HTML in a
// template literal where the ui-action is a runtime expression (escapeHtml
// call), so the literal "n6Run" never appears in source. The CSS class name
// "n6-run-btn" IS a compile-time literal inside the template and is the
// correct source-level marker. The runtime action IDs are validated at the
// HTML level by n6Render.test.ts (the C half of D7=C).
//
// D7=C: this source-level check pairs with the n6Render.test.ts HTML-level check.
const RENDER_TS = path.join(SRC, "render.ts");
if (fs.existsSync(RENDER_TS)) {
  const renderSource = fs.readFileSync(RENDER_TS, { encoding: "utf8" });
  // Find all N6 run button template-literal occurrences by CSS class.
  const n6BtnPattern = /n6-run-btn/g;
  let n6BtnMatch;
  let n6BtnCount = 0;
  while ((n6BtnMatch = n6BtnPattern.exec(renderSource)) !== null) {
    n6BtnCount++;
    const start = Math.max(0, n6BtnMatch.index - 30);
    const end = Math.min(renderSource.length, n6BtnMatch.index + 300);
    const context = renderSource.slice(start, end);
    if (!context.includes('data-n6-token-required="true"')) {
      violations.push(
        `src/render.ts: D7-VIOLATION: n6-run-btn occurrence at offset ${n6BtnMatch.index} missing data-n6-token-required="true" in surrounding 300 chars`,
      );
    }
  }
  // Enforcement: N6 run buttons MUST exist (guards that N6 render block is
  // present and wired — prevents silently omitting the execution boundary).
  if (n6BtnCount === 0) {
    violations.push(
      "src/render.ts: D7-MISSING: no n6-run-btn class found — N6 execution boundary render block must be present",
    );
  }
} else {
  violations.push("src/render.ts: MISSING render module");
}

if (violations.length > 0) {
  console.error("a2-harness-panel guards FAILED:");
  for (const v of violations) {
    console.error(`  - ${v}`);
  }
  process.exit(1);
}

console.log("a2-harness-panel guards PASS (" + files.length + " src files audited)");
