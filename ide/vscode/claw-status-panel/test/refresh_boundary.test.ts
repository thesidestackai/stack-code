import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import { PKG_ROOT, SRC_DIR } from "./_paths";

const SRC = SRC_DIR;

function readSrcFiles(): Array<{ name: string; source: string }> {
  const out: Array<{ name: string; source: string }> = [];
  const walk = (dir: string) => {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const p = path.join(dir, e.name);
      if (e.isDirectory()) {
        walk(p);
      } else if (e.isFile() && p.endsWith(".ts")) {
        out.push({ name: path.relative(PKG_ROOT, p), source: fs.readFileSync(p, "utf8") });
      }
    }
  };
  walk(SRC);
  return out;
}

function stripCommentsAndStrings(src: string): string {
  let out = "";
  let i = 0;
  const n = src.length;
  while (i < n) {
    const two = src.slice(i, i + 2);
    if (two === "//") {
      while (i < n && src[i] !== "\n") i++;
      continue;
    }
    if (two === "/*") {
      i += 2;
      while (i < n && src.slice(i, i + 2) !== "*/") i++;
      i += 2;
      continue;
    }
    const ch = src[i];
    if (ch === '"' || ch === "'" || ch === "`") {
      const q = ch;
      i++;
      while (i < n && src[i] !== q) {
        if (src[i] === "\\") {
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

describe("refresh boundary — no implicit refresh triggers in source", () => {
  const files = readSrcFiles();

  it("source contains NO setInterval / setTimeout / setImmediate", () => {
    for (const f of files) {
      const live = stripCommentsAndStrings(f.source);
      assert.ok(!/\bsetInterval\s*\(/.test(live), `${f.name} uses setInterval`);
      assert.ok(!/\bsetTimeout\s*\(/.test(live), `${f.name} uses setTimeout`);
      assert.ok(!/\bsetImmediate\s*\(/.test(live), `${f.name} uses setImmediate`);
    }
  });

  it("source contains NO filesystem watchers (chokidar / fs.watch / VS Code watcher API)", () => {
    for (const f of files) {
      const live = stripCommentsAndStrings(f.source);
      assert.ok(!/\bchokidar\b/.test(live), `${f.name} references chokidar`);
      assert.ok(!/\bfs\.watch\b/.test(live), `${f.name} uses fs.watch`);
      assert.ok(!/\bfs\.watchFile\b/.test(live), `${f.name} uses fs.watchFile`);
      assert.ok(
        !/\bcreateFileSystemWatcher\b/.test(live),
        `${f.name} uses vscode.workspace.createFileSystemWatcher`,
      );
    }
  });

  it("source contains NO VS Code file-change event subscriptions", () => {
    const forbidden = [
      "onDidChangeFiles",
      "onDidCreateFiles",
      "onDidDeleteFiles",
      "onDidRenameFiles",
      "onDidSaveTextDocument",
      "onDidOpenTextDocument",
      "onDidChangeTextDocument",
      "onDidChangeWindowState",
      "onDidChangeWorkspaceFolders",
    ];
    for (const f of files) {
      const live = stripCommentsAndStrings(f.source);
      for (const ev of forbidden) {
        assert.ok(
          !new RegExp(`\\b${ev}\\b`).test(live),
          `${f.name} subscribes to ${ev}`,
        );
      }
    }
  });

  it("the package activationEvents include only operator-gesture-driven events", () => {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(PKG_ROOT, "package.json"), "utf8"),
    );
    const events: string[] = manifest.activationEvents ?? [];
    for (const ev of events) {
      assert.ok(
        ev.startsWith("onCommand:"),
        `activation event must be onCommand:*; got ${ev}`,
      );
    }
  });

  it("the extension exposes only one refresh command and no auto-refresh wiring", () => {
    const manifest = JSON.parse(
      fs.readFileSync(path.join(PKG_ROOT, "package.json"), "utf8"),
    );
    const cmds: Array<{ command: string }> = manifest.contributes?.commands ?? [];
    assert.strictEqual(cmds.length, 1);
    assert.strictEqual(cmds[0].command, "clawStatus.refresh");
    const settings = manifest.contributes?.configuration?.properties ?? {};
    for (const k of Object.keys(settings)) {
      assert.ok(!/auto[-_]?refresh/i.test(k));
      assert.ok(!/poll/i.test(k));
      assert.ok(!/refreshOn/i.test(k));
    }
  });
});

describe("refresh boundary — no Git event stream / daemon / IPC subscriptions", () => {
  const files = readSrcFiles();

  it("source has no simple-git, nodegit, or vscode-git extension subscriptions", () => {
    for (const f of files) {
      const live = stripCommentsAndStrings(f.source);
      assert.ok(!/\bsimple-git\b/.test(live));
      assert.ok(!/\bnodegit\b/.test(live));
      assert.ok(!/\bvscode\.git\b/.test(live));
      assert.ok(!/\bgetExtension\b/.test(live));
    }
  });

  it("source has no WebSocket / net.createConnection / dgram", () => {
    for (const f of files) {
      const live = stripCommentsAndStrings(f.source);
      assert.ok(!/\bWebSocket\b/.test(live));
      assert.ok(!/\bnet\.createConnection\b/.test(live));
      assert.ok(!/\bdgram\.createSocket\b/.test(live));
    }
  });
});
