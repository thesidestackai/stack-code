//! A2 Tier 3 read-only evidence collector (binary).
//!
//! Read-only: emits one `a2-tier3-evidence-snapshot.v0` JSON object to stdout for
//! a control checkout and an optional named worktree. Runs no claw / orchestrator,
//! writes nothing, and calls no model / broker / runtime / network / Vault.
//!
//! Usage:
//!   a2-evidence-collector <control-checkout> [--worktree <path>] [--captured-base <sha>]
//!
//! Exit codes:
//!   0  the requested subjects were observable and a snapshot was emitted
//!   2  usage error
//!   3  the control checkout could not be observed read-only

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use a2_evidence_collector::{
    build_snapshot, gather_git_state, list_smoke_worktrees, scan_worktree, WorktreeEvidence,
};

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut control_checkout: Option<String> = None;
    let mut named_worktree: Option<String> = None;
    let mut captured_base: Option<String> = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--worktree" => {
                let Some(v) = args.next() else {
                    eprintln!("a2-evidence-collector: --worktree requires a path");
                    return ExitCode::from(2);
                };
                named_worktree = Some(v);
            }
            "--captured-base" => {
                let Some(v) = args.next() else {
                    eprintln!("a2-evidence-collector: --captured-base requires a value");
                    return ExitCode::from(2);
                };
                captured_base = Some(v);
            }
            other if other.starts_with("--") => {
                eprintln!("a2-evidence-collector: unknown flag {other}");
                return ExitCode::from(2);
            }
            other => {
                if control_checkout.is_some() {
                    eprintln!("a2-evidence-collector: unexpected positional argument {other}");
                    return ExitCode::from(2);
                }
                control_checkout = Some(other.to_string());
            }
        }
    }

    let control_checkout = control_checkout.unwrap_or_else(|| ".".to_string());
    let checkout_path = Path::new(&control_checkout);

    let mut git = gather_git_state(checkout_path);
    if git.dirty.is_none() && git.current_origin_main.is_none() {
        eprintln!(
            "a2-evidence-collector: control checkout not observable read-only: {control_checkout}"
        );
        return ExitCode::from(3);
    }
    if captured_base.is_some() {
        git.captured_base = captured_base;
    }

    // Read-only smoke-worktree completeness for partial_smoke_count.
    let smoke = list_smoke_worktrees(checkout_path);
    git.smoke_worktree_completeness = smoke
        .iter()
        .map(|p| scan_worktree(p).is_complete_success())
        .collect();

    let ev: Option<WorktreeEvidence> = named_worktree
        .as_deref()
        .map(|p| scan_worktree(&PathBuf::from(p)));

    let snapshot = build_snapshot(
        &control_checkout,
        named_worktree.as_deref(),
        &git,
        ev.as_ref(),
        None,
    );

    match serde_json::to_string(&snapshot) {
        Ok(json) => {
            println!("{json}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("a2-evidence-collector: failed to serialize snapshot: {e}");
            ExitCode::from(3)
        }
    }
}
