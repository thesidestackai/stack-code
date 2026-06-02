import * as path from "path";

// The package root resolves the same way at runtime regardless of whether
// the test file ran from src (unlikely) or from out-test/test (default).
// We walk up from __dirname until we hit a directory containing
// package.json — that is the package root.
import * as fs from "fs";

function findPackageRoot(start: string): string {
  let cur = start;
  for (let i = 0; i < 10; i++) {
    if (fs.existsSync(path.join(cur, "package.json"))) {
      return cur;
    }
    const parent = path.dirname(cur);
    if (parent === cur) break;
    cur = parent;
  }
  throw new Error(`package.json not found above ${start}`);
}

export const PKG_ROOT = findPackageRoot(__dirname);
export const SRC_DIR = path.join(PKG_ROOT, "src");
export const TEST_DIR = path.join(PKG_ROOT, "test");
export const FIXTURES_DIR = path.join(TEST_DIR, "fixtures");
export const PACKAGE_JSON_PATH = path.join(PKG_ROOT, "package.json");
export const GUARDS_SCRIPT = path.join(PKG_ROOT, "scripts", "run-guards.js");
