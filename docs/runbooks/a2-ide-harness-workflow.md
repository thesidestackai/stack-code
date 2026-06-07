# A2 IDE Harness Workflow — Operator Runbook (v0)

> v0 is an **IDE-adjacent** harness: VS Code / Cursor tasks plus a print/validate-only
> helper script. It does **not** run any A2 command and it does **not** weaken any safety
> gate. You still run preview / approval / apply yourself, with approval at a **real terminal**.

Scope source of truth: [a2-l4-ide-harness-workflow-scope.md](../a2-l4-ide-harness-workflow-scope.md).

---

## What this gives you

A guided, visual way to drive the proven A2-L2b chain without memorizing long commands:

```text
1. choose workspace root + plan.yaml
2. validate paths (read-only; refuses absolute after_file)
3. print the PREVIEW command            (you run it; writes no target)
4. find .claw artifacts + view hashes   (read-only)
5. print the APPROVAL command           (you run it at a REAL terminal; writes no target)
6. print the APPLY-BUNDLE command       (you run it; GENERATOR, writes no target)
7. print the APPLY command              (you run it ONCE; the only target write)
8. verify the final target hash         (read-only)
```

The helper script **prints** each command for you to run manually. Nothing in this
harness executes `claw plan run/approve/apply-bundle/apply` on your behalf.

---

## The proven chain (what you actually run)

```text
1. PREVIEW   claw plan run <plan.yaml> --workspace-root <ws> --workspace-write-preview
2. APPROVE   claw plan approve <preview-bundle.json> --approval-result-output <out.json>
             (REAL terminal; at the prompt type:  apply <step-id> <preview_sha256>)
3. BUNDLE    claw plan apply-bundle <preview-generator-result.json> <approval-result.json>
4. APPLY     claw plan apply <apply-bundle.json>
```

Roles you must keep straight:

```text
apply-bundle  = GENERATOR. Assembles apply-bundle.json. Writes NO target.
plan apply    = EXECUTOR.  The ONLY command that writes the target. Run it once.
```

---

## Using it in VS Code / Cursor

1. Open the repo in VS Code or Cursor.
2. Open the Command Palette → **Tasks: Run Task**.
3. Run the **A2:** tasks in order. Each task prompts for the paths it needs.

| Task | What it does | Runs an A2 command? |
| --- | --- | --- |
| A2: Help | Shows the chain and safety rules | No |
| A2: Validate Input | Read-only checks on workspace + plan.yaml | No |
| A2: Print Preview Command | Prints STEP 1 | No (prints only) |
| A2: Find Artifacts | Lists `.claw` artifacts + hashes | No |
| A2: Print Approval Command | Prints STEP 2 (real-terminal approval) | No (prints only) |
| A2: Print Apply-Bundle Command | Prints STEP 3 (generator) | No (prints only) |
| A2: Print Apply Command | Prints STEP 4 (executor) | No (prints only) |
| A2: Verify Final Target | Read-only hash check | No |

You can also call the helper directly:

```bash
scripts/a2-ide-harness.sh help
scripts/a2-ide-harness.sh validate-input --workspace <ws> --plan <plan.yaml>
scripts/a2-ide-harness.sh print-preview --workspace <ws> --plan <plan.yaml>
scripts/a2-ide-harness.sh find-artifacts --workspace <ws>
scripts/a2-ide-harness.sh print-approval --workspace <ws> --preview-bundle <pb.json> --approval-output <out.json>
scripts/a2-ide-harness.sh print-apply-bundle --preview-generator-result <gen.json> --approval-result <appr.json>
scripts/a2-ide-harness.sh print-apply --apply-bundle <ab.json>
scripts/a2-ide-harness.sh verify-final --workspace <ws> --target <target> --after-sha <sha>
```

Set `A2_CLAW=/path/to/claw` to point the printed commands at a specific `claw` binary.
The default is the dated build artifact noted in the merged scope.

---

## The approval step (read this)

Approval **must** happen at a real interactive terminal. Run the printed
`claw plan approve … --approval-result-output …` command in a normal terminal and type
the exact approval line:

```text
apply <step-id> <preview_sha256>
```

- Do **not** pipe input, use `--yes`, batch, expect, or any fake-TTY trick.
- Running approval inside a non-interactive command runner (including a Claude/Codex task
  runner) will **fail-closed** (exit 7). That is the TTY guard working, not a bug — move to
  a real terminal.

---

## Safety rules this workflow preserves

```text
- Preview does not write the target.
- Approval does not write the target.
- apply-bundle generation does not write the target.
- Only `claw plan apply` writes the target, and only once per approved preview.
- No auto-approval. No hidden apply. No apply without a validated approval-result.
- No apply if the target hash differs from before_sha.
- No model / broker / runtime calls from this harness. No raw :11434 inference.
```

If anything is ambiguous — missing preview bundle, missing approval-result, hash mismatch,
target drift, a prior apply marker, an unreviewed or absolute after_file, or an unsafe target
path — **stop** and resolve it before continuing. The helper flags these read-only; it never
forces past them.

---

## What is NOT in v0

This is intentionally not a full extension yet. A future v1 (see the scope card) can add a
proper VS Code / Cursor panel with a diff viewer, an approval-phrase input bound to a vetted
non-TTY entry point, and an apply button disabled until the approval-result validates. v0
stays print/validate-only until the command contracts are exercised and stable.
