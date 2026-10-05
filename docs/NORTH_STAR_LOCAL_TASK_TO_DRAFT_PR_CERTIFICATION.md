# North Star: Local Task to Draft PR Certification

## Purpose

This document certifies the end-to-end path by which a bounded, operator-approved local task
travels from a plain-language objective to an open draft pull request, entirely through
Stack-Code's own product surface, with no manual Git or GitHub steps in between.

## Certified Path

```
task -> broker-routed plan -> isolated mutation -> validation
     -> commit -> push -> draft PR -> CI -> review
```

Concretely: an operator-approved task specification is submitted to `claw task run`. The bridge
requests a plan from the broker, validates the planner's output, generates and applies an A2
preview/apply bundle inside an isolated disposable Git worktree, runs the task's declared
validation profile, and reports package-readiness. Packaging itself (`package-plan`,
`package-commit`, `package-push`, `package-pr`) is then driven exclusively through the reviewed
Layer-A orchestrator (`scripts/a2-tier3-write-orchestrator.sh`), producing exactly one commit, a
non-force push, and one draft pull request.

## Safety Boundaries

- Application inference uses `http://127.0.0.1:11435` only.
- Raw `http://127.0.0.1:11434` (or `localhost:11434`) application inference is prohibited; the
  bridge and the shared validator both refuse candidates or task specs that reference it.
- Work occurs in a fresh, isolated Git worktree under the disposable worktree root, never in the
  operator's control checkout.
- The task worktree must start clean and must sit exactly at the currently approved
  `origin/main`; drift during planning or before apply is refused.
- Only the exact operator-declared `allowed_paths`/`target_path` may change; absolute paths,
  traversal, symlink targets, and reserved `.claw` paths are refused before any mutation.
- Exact-file staging and a non-force push are required; the packaging gate independently verifies
  the resulting commit has exactly one parent equal to the approved base before any push or PR is
  created.
- Merge is not part of certification. No PR produced by this path is merged as part of running it.

## Required Evidence

Every certification run must produce a bounded, non-secret receipt chain: task acceptance, the
broker plan (with requested and broker-resolved model recorded as distinct fields), planner
validation, mutation authorization, A2 apply result, changed-file verification, validation,
package-control preflight, package-plan readiness, and task completion. Evidence is classified
by how it was obtained:

- **Verified** — directly observed by running a command and reading its exact output (e.g. a
  receipt file's contents, a `git diff --stat`, a CI job's recorded conclusion).
- **Inferred** — derived from verified evidence via a documented, checkable rule (e.g. "exactly
  one changed file" derived from `git diff --name-only`).
- **Assumed** — accepted from an upstream component's own self-report without independent
  re-derivation (e.g. trusting the broker's reported `resolved_model` string as the model that
  actually produced the plan).
- **Unknown** — explicitly flagged as not established, rather than silently omitted.

## Failure Classification

- **Refused** — the bridge or an underlying gate declined to proceed and left no partial mutation
  outside its own bridge-owned receipt directory (e.g. malformed planner output, unauthorized
  candidate path, dirty starting worktree, broker route violation, secret-shaped candidate text).
- **Blocked** — an external precondition was not met (e.g. broker not admitting new planner
  traffic, a missing reviewed-script binding) and no task attempt was made.
- **Packaging failure** — the task's mutation succeeded and validated, but a Tier-4 packaging rung
  (plan, commit, push, or PR) failed or was unavailable; no manual Git substitution is permitted
  in this case.
- **Certified** — every required receipt is present and internally consistent, exactly one file
  changed, the package rungs produced one commit/one non-force push/one draft PR, and exact-head
  CI on that PR succeeded.

## Non-Goals

- Runtime deployment or service restart of any kind.
- Installing or upgrading dependencies, or modifying lockfiles.
- Manually loading, unloading, or evicting a model, or altering broker scheduling policy.
- Unrelated repository cleanup, refactors, or scope beyond the single certified target file.
- Merging the certification pull request, or any pull request, as part of this certification.
- Proving certification by writing this document alone: a document describing the protocol is
  not itself evidence that a live run through the product surface succeeded.
