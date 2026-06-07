import * as assert from "assert";
import * as fs from "fs";
import { PACKAGE_JSON_PATH } from "./_paths";

interface Manifest {
  name: string;
  main: string;
  activationEvents: string[];
  contributes: {
    commands: Array<{ command: string; title: string }>;
    configuration?: { properties: Record<string, unknown> };
  };
}

function loadManifest(): Manifest {
  return JSON.parse(fs.readFileSync(PACKAGE_JSON_PATH, "utf8")) as Manifest;
}

describe("manifest — contributes only a read-only open command", () => {
  it("declares exactly one command: a2HarnessPanel.open", () => {
    const m = loadManifest();
    assert.strictEqual(m.contributes.commands.length, 1);
    assert.strictEqual(m.contributes.commands[0].command, "a2HarnessPanel.open");
  });

  it("no contributed command title is a Run-* / approve / apply control", () => {
    const m = loadManifest();
    for (const c of m.contributes.commands) {
      assert.ok(!/run preview|run approval|run apply|approve|^apply\b/i.test(c.title), `dangerous command title: ${c.title}`);
    }
  });

  it("activates only on its own command (no '*', no file-system events)", () => {
    const m = loadManifest();
    assert.deepStrictEqual(m.activationEvents, ["onCommand:a2HarnessPanel.open"]);
    for (const ev of m.activationEvents) {
      assert.ok(!/onDidChange|workspaceContains|\*/.test(ev), `broad activation: ${ev}`);
    }
  });

  it("configures only a helperPath, defaulting to the v0 helper", () => {
    const m = loadManifest();
    const props = m.contributes.configuration?.properties ?? {};
    assert.deepStrictEqual(Object.keys(props), ["a2HarnessPanel.helperPath"]);
  });

  it("main points at the compiled extension entry", () => {
    const m = loadManifest();
    assert.strictEqual(m.main, "./out/extension.js");
  });
});
