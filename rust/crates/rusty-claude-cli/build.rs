use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Run `git` with `args`, returning trimmed stdout only on success.
fn git_output(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

/// Emit a Cargo watcher for `path`, but only once it is known to exist.
///
/// Cargo resolves relative `rerun-if-changed` entries against the package
/// root -- `rust/crates/rusty-claude-cli`, three levels below the repository
/// -- so a bare `.git/...` entry never names anything real. Absolutise first.
fn watch_if_present(path: &str) {
    let raw = PathBuf::from(path);
    let resolved = std::fs::canonicalize(&raw).unwrap_or(raw);
    if resolved.exists() {
        println!("cargo:rerun-if-changed={}", resolved.display());
    }
}

fn main() {
    // Get the FULL git SHA (40 lowercase hex characters).
    //
    // The canonical installation safety contract compares this value against
    // the source HEAD / locally known origin/main for EXACT equality. An
    // abbreviated hash is not a sufficient provenance proof, so this must stay
    // `rev-parse HEAD` and must never be shortened again.
    let git_sha = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|output| {
            if output.status.success() {
                String::from_utf8(output.stdout).ok()
            } else {
                None
            }
        })
        .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string());

    println!("cargo:rustc-env=GIT_SHA={git_sha}");

    // TARGET is always set by Cargo during build
    let target = env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());
    println!("cargo:rustc-env=TARGET={target}");

    // Build date from SOURCE_DATE_EPOCH (reproducible builds) or current UTC date.
    // Intentionally ignoring time component to keep output deterministic within a day.
    let build_date = std::env::var("SOURCE_DATE_EPOCH")
        .ok()
        .and_then(|epoch| epoch.parse::<i64>().ok())
        .map(|_ts| {
            // Use SOURCE_DATE_EPOCH to derive date via chrono if available;
            // for simplicity we just use the env var as a signal and fall back
            // to build-time env. In practice CI sets this via workflow.
            std::env::var("BUILD_DATE").unwrap_or_else(|_| "unknown".to_string())
        })
        .or_else(|| std::env::var("BUILD_DATE").ok())
        .unwrap_or_else(|| {
            // Fall back to current date via `date` command
            Command::new("date")
                .args(["+%Y-%m-%d"])
                .output()
                .ok()
                .and_then(|o| {
                    if o.status.success() {
                        String::from_utf8(o.stdout).ok()
                    } else {
                        None
                    }
                })
                .map_or_else(|| "unknown".to_string(), |s| s.trim().to_string())
        });
    println!("cargo:rustc-env=BUILD_DATE={build_date}");

    // Rerun if git state changes.
    //
    // The provenance SHA above must not outlive a HEAD change in a warm Cargo
    // target, so these watchers have to name real Git metadata. In a linked
    // worktree `.git` is a pointer FILE into
    // `<main-repo>/.git/worktrees/<name>/`, so crate-relative `.git/HEAD` and
    // `.git/refs` resolve to nothing at all. Ask Git for the true locations
    // rather than assuming any particular on-disk layout.
    if let Some(head) = git_output(&["rev-parse", "--git-path", "HEAD"]) {
        // Worktree-local HEAD. On a detached HEAD this file holds the SHA
        // itself, which is sufficient to invalidate on checkout.
        watch_if_present(&head);
    }
    if let Some(head_ref) = git_output(&["symbolic-ref", "-q", "HEAD"]) {
        // On a branch, HEAD only names the ref; the SHA lives in the ref file,
        // which for a linked worktree usually sits in the common Git dir.
        if let Some(ref_path) = git_output(&["rev-parse", "--git-path", &head_ref]) {
            watch_if_present(&ref_path);
        }
    }
    if let Some(packed) = git_output(&["rev-parse", "--git-path", "packed-refs"]) {
        // Fallback for a branch whose loose ref has been packed away.
        watch_if_present(&packed);
    }
}
