import * as assert from "assert";
import * as fs from "fs";
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

describe("network / telemetry guards", () => {
  const files = listSrc();

  const FORBIDDEN_NETWORK = [
    "axios",
    "node-fetch",
    "got",
    "superagent",
    "ky",
    "undici",
    "fetch\\(",
    "XMLHttpRequest",
    "WebSocket",
    "http\\.request",
    "https\\.request",
    "http\\.get",
    "https\\.get",
    "openExternal",
  ];

  for (const term of FORBIDDEN_NETWORK) {
    it(`no use of \`${term}\` in src/`, () => {
      const re = new RegExp(`\\b${term}\\b`);
      for (const f of files) {
        assert.ok(!re.test(strip(f.src)), `${f.name} matches ${term}`);
      }
    });
  }

  it("no http:// or https:// URLs in live code (comments/strings allowed)", () => {
    for (const f of files) {
      const live = strip(f.src);
      assert.ok(!/https?:\/\//.test(live), `${f.name} has live URL`);
    }
  });

  it("no telemetry / analytics SDK references in live code", () => {
    for (const f of files) {
      const live = strip(f.src);
      assert.ok(!/\btelemetry\b/i.test(live), `${f.name} uses telemetry`);
      assert.ok(!/\banalytics\b/i.test(live), `${f.name} uses analytics`);
    }
  });

  it("no broker / ollama / model-endpoint references in live code", () => {
    for (const f of files) {
      const live = strip(f.src);
      assert.ok(!/\bollama\b/i.test(live), `${f.name} uses ollama`);
      assert.ok(!/\bbroker[_-]?url\b/i.test(live), `${f.name} uses broker_url`);
    }
  });

  it("no secret-storage API access in live code", () => {
    for (const f of files) {
      const live = strip(f.src);
      assert.ok(!/\bSecretStorage\b/.test(live), `${f.name} uses SecretStorage`);
      assert.ok(!/\bcontext\.secrets\b/.test(live), `${f.name} uses context.secrets`);
    }
  });

  it("the installed dependency tree contains no known HTTP client (manifest-level)", () => {
    const manifest = JSON.parse(fs.readFileSync(path.join(PKG_ROOT, "package.json"), "utf8"));
    const deps = { ...(manifest.dependencies ?? {}), ...(manifest.devDependencies ?? {}) };
    const forbidden = ["axios", "node-fetch", "got", "request", "superagent", "ky", "undici"];
    for (const d of forbidden) {
      assert.ok(!(d in deps), `forbidden HTTP client in manifest: ${d}`);
    }
  });
});
