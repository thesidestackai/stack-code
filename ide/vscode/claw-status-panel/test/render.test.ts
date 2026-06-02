import * as assert from "assert";
import { parseStatus } from "../src/parser";
import { buildRenderModel, renderHtml } from "../src/render";
import { classifyAll } from "../src/evidence_path";
import {
  baselineSuccessEnvelope,
  baselineRefusalEnvelope,
  jsonOf,
  NEXT_OPERATOR_COMMAND_STOP_LITERAL,
} from "./fixtures/_helpers";
import { EXIT_STATUS_REFUSED } from "../src/envelope";

function buildHtml(stdout: string, exit: number, workspace = "/disposable/wks"): string {
  const parsed = parseStatus(stdout, exit);
  const evidence = classifyAll(
    parsed.envelope?.evidence_paths ?? [],
    workspace,
  );
  const model = buildRenderModel(parsed, evidence);
  return renderHtml(model);
}

describe("render — STOP rendering", () => {
  it("STOP banner contains literal stop_condition value verbatim (no friendly text)", () => {
    const env = baselineSuccessEnvelope({
      stop_condition: "payload-sha-mismatch",
      next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
    });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(html.includes("payload-sha-mismatch"));
    assert.ok(/data-testid="stop-banner"/.test(html));
    assert.ok(!html.includes("Mismatch detected"));
    assert.ok(!html.includes("Action required"));
    assert.ok(/data-stop="true"/.test(html));
  });

  it("STOP rendering uses the stop class, not a 'warning' or 'info' downgrade", () => {
    const env = baselineSuccessEnvelope({
      stop_condition: "live-target-sha-changed",
      next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
    });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(/class="[^"]*stop[^"]*"/.test(html));
    assert.ok(!/class="[^"]*warning-banner[^"]*"/.test(html));
    assert.ok(!/class="[^"]*info-banner[^"]*"/.test(html));
    assert.ok(!html.includes("data-testid=\"snooze\""));
    assert.ok(!html.includes("data-testid=\"dismiss\""));
    assert.ok(!html.includes("data-testid=\"mute\""));
  });

  it("STOP banner has a heading at least as prominent as the OK banner heading (both <h2>)", () => {
    const stop = buildHtml(jsonOf(baselineSuccessEnvelope({ stop_condition: "live-target-missing", next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL })), 0);
    const ok = buildHtml(jsonOf(baselineSuccessEnvelope()), 0);
    assert.ok(stop.includes("<h2>STOP — escalate</h2>"));
    assert.ok(ok.includes("<h2>Status: OK</h2>"));
  });

  it("STOP rendering surfaces evidence_paths without a collapsed disclosure (visible on load)", () => {
    const env = baselineSuccessEnvelope({
      stop_condition: "approval-sha-mismatch",
      next_operator_command: NEXT_OPERATOR_COMMAND_STOP_LITERAL,
      evidence_paths: [
        "/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json",
      ],
    });
    const html = buildHtml(jsonOf(env), 0);
    const evidenceIdx = html.indexOf("data-testid=\"evidence\"");
    assert.ok(evidenceIdx >= 0, "evidence section must exist");
    const evidenceSlice = html.slice(evidenceIdx, evidenceIdx + 800);
    assert.ok(!/<details/.test(evidenceSlice), "evidence section must not be a <details> disclosure");
    assert.ok(evidenceSlice.includes("/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json"));
  });

  it("refusal envelope (exit 12) renders STOP, refused-marker visible, stop_condition visible", () => {
    const env = baselineRefusalEnvelope();
    const html = buildHtml(jsonOf(env), EXIT_STATUS_REFUSED);
    assert.ok(/data-stop="true"/.test(html));
    assert.ok(html.includes("a2-l2d-status-refused"));
    assert.ok(html.includes("workspace-root-invalid"));
  });
});

describe("render — next_operator_command as copyable text", () => {
  it("rendered as text with data-copy-target, NOT as a button or executable affordance", () => {
    const env = baselineSuccessEnvelope();
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(/data-copy-target="next_operator_command"/.test(html));
    assert.ok(!/data-action="execute_next_operator_command"/.test(html));
    assert.ok(!/onclick="exec/.test(html));
    assert.ok(!html.includes("run-in-terminal"));
    assert.ok(!html.includes("send-to-terminal"));
  });

  it("rendered string is verbatim — no decoration, no terminal-prefix, no shell-quoting", () => {
    const literal =
      "claw plan approve /disposable/wks/.claw/l2b-preview-bundles/run-0001/step-001/preview-bundle.json";
    const env = baselineSuccessEnvelope({ next_operator_command: literal });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(html.includes(literal));
    assert.ok(!html.includes("$ " + literal));
    assert.ok(!html.includes("> " + literal));
    assert.ok(!html.includes('"' + literal + '"'));
  });
});

describe("render — evidence path rendering", () => {
  it("in-workspace evidence path renders as link, NOT eagerly opened, no 'open all' button", () => {
    const env = baselineSuccessEnvelope({
      evidence_paths: ["/disposable/wks/.claw/l2b-runs/run-0001/run-manifest.json"],
    });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(/data-evidence-location="in-workspace"/.test(html));
    assert.ok(/class="evidence-link"/.test(html));
    assert.ok(!/data-action="open-all-evidence"/.test(html));
    assert.ok(!html.includes("Open all"));
  });

  it("out-of-workspace evidence path renders with explicit warning surface", () => {
    const env = baselineSuccessEnvelope({
      evidence_paths: ["/somewhere/else/approval-result.json"],
    });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(/data-evidence-location="out-of-workspace"/.test(html));
    assert.ok(/data-testid="out-of-workspace-warning"/.test(html));
  });

  it("evidence path text is verbatim — no canonicalization", () => {
    const verbatim = "/disposable/wks/./.claw/l2b-runs/run-0001/run-manifest.json";
    const env = baselineSuccessEnvelope({ evidence_paths: [verbatim] });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(html.includes(verbatim));
  });
});

describe("render — manifest read_only_invariant rendering", () => {
  it("rendered verbatim, visible on every envelope", () => {
    const html = buildHtml(jsonOf(baselineSuccessEnvelope()), 0);
    assert.ok(html.includes("this command does not mutate state"));
    assert.ok(/data-field="read_only_invariant"/.test(html));
  });

  it("substituted invariant renders the substituted string verbatim (no coercion)", () => {
    const env = baselineSuccessEnvelope({ read_only_invariant: "trust me bro" });
    const html = buildHtml(jsonOf(env), 0);
    assert.ok(html.includes("trust me bro"));
    assert.ok(!html.includes("this command does not mutate state"));
  });
});

describe("render — refresh affordance", () => {
  it("has exactly one refresh button per render", () => {
    const html = buildHtml(jsonOf(baselineSuccessEnvelope()), 0);
    const matches = html.match(/data-action="refresh"/g) ?? [];
    assert.strictEqual(matches.length, 1);
  });

  it("does NOT expose approve / apply / run / apply-bundle action buttons", () => {
    const html = buildHtml(jsonOf(baselineSuccessEnvelope()), 0);
    for (const forbidden of [
      "data-action=\"approve\"",
      "data-action=\"apply\"",
      "data-action=\"apply-bundle\"",
      "data-action=\"run\"",
      "data-action=\"approve-and-apply\"",
    ]) {
      assert.ok(!html.includes(forbidden), `forbidden affordance present: ${forbidden}`);
    }
  });
});
