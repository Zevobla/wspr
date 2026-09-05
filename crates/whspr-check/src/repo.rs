//! Repo-location and subprocess helpers shared by every check.
//!
//! `whspr-check` is meant to be invoked from inside the whspr repo (e.g.
//! `cargo run -p whspr-check` from the repo root, or a subdirectory of it —
//! `cargo` itself doesn't require you to be at the workspace root). Rather
//! than baking in a compile-time path (which would break the moment the
//! binary is packaged/copied, e.g. under `nix build`), we walk up from the
//! current working directory at *runtime* looking for the repo's
//! fingerprint: a root `Cargo.toml` next to a `crates/` directory and a
//! `flake.nix`.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Finds the whspr repo root by walking up from the current directory.
pub fn find_repo_root() -> anyhow::Result<PathBuf> {
    let start = std::env::current_dir()?;
    let mut dir = start.as_path();
    loop {
        if dir.join("flake.nix").is_file()
            && dir.join("Cargo.toml").is_file()
            && dir.join("crates").is_dir()
        {
            return Ok(dir.to_path_buf());
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => {
                anyhow::bail!(
                    "could not locate the whspr repo root (looked for flake.nix + Cargo.toml \
                     + crates/ walking up from {}); run whspr-check from inside the whspr repo",
                    start.display()
                );
            }
        }
    }
}

/// The result of running a subprocess to completion: captured stdout/stderr
/// (lossily decoded, since we only ever grep/compare these, never round-trip
/// bytes), whether it exited zero, and the raw exit code for callers that
/// need to distinguish exit codes more finely (e.g. `git grep` uses exit 1
/// for "ran fine, no matches" rather than an actual error).
pub struct CmdOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Runs `program args...` with cwd `root`, waiting for it to finish. Never
/// panics on a nonzero exit or missing binary — callers decide what a
/// failure means for their criterion (`CmdOutput::success` / a `Result::Err`
/// only for "couldn't even spawn the process").
pub fn run(root: &Path, program: &str, args: &[&str]) -> anyhow::Result<CmdOutput> {
    run_env(root, program, args, &[])
}

/// Same as [`run`], additionally setting/overriding the given environment
/// variables for the child process (used e.g. to poison network access with
/// bogus proxy vars for the offline-isolation checks).
pub fn run_env(
    root: &Path,
    program: &str,
    args: &[&str],
    envs: &[(&str, &str)],
) -> anyhow::Result<CmdOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{program} {}`: {e}", args.join(" ")))?;
    Ok(CmdOutput {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Same as [`run`], but removing the given environment variables from the
/// child's environment first (used to prove a code path doesn't implicitly
/// depend on, e.g., a display server being present).
pub fn run_without_envs(
    root: &Path,
    program: &str,
    args: &[&str],
    remove: &[&str],
) -> anyhow::Result<CmdOutput> {
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(root);
    for key in remove {
        cmd.env_remove(key);
    }
    let output = cmd
        .output()
        .map_err(|e| anyhow::anyhow!("failed to spawn `{program} {}`: {e}", args.join(" ")))?;
    Ok(CmdOutput {
        success: output.status.success(),
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}

/// Builds `package` and returns the path to its `bin_name` binary under
/// the default `target/debug/` layout. Assumes no custom
/// `CARGO_TARGET_DIR`, which matches every documented dev workflow in
/// this repo (README, CLAUDE.md, flake devShell).
pub fn ensure_binary_built(root: &Path, package: &str, bin_name: &str) -> anyhow::Result<PathBuf> {
    let build = run(root, "cargo", &["build", "-p", package, "--quiet"])?;
    if !build.success {
        anyhow::bail!("`cargo build -p {package}` failed: {}", build.stderr);
    }
    let path = root.join("target/debug").join(bin_name);
    if !path.is_file() {
        anyhow::bail!(
            "expected a binary at {} after building {package}, but it's not there (custom \
             CARGO_TARGET_DIR?)",
            path.display()
        );
    }
    Ok(path)
}

/// Runs `cargo metadata --format-version 1` and parses its JSON. Shared by
/// every check that needs the resolved dependency graph or per-package
/// manifest fields (declared deps, license, ...) rather than just reading
/// Cargo.toml files directly.
pub fn cargo_metadata(root: &Path) -> anyhow::Result<serde_json::Value> {
    let output = run(root, "cargo", &["metadata", "--format-version", "1"])?;
    if !output.success {
        anyhow::bail!("`cargo metadata` failed: {}", output.stderr);
    }
    serde_json::from_str(&output.stdout)
        .map_err(|e| anyhow::anyhow!("could not parse cargo metadata JSON: {e}"))
}

/// Reads README.md from the repo root. Shared by every doc-content check
/// (Z-04, W-06/07/08, AH-03/04, AC-03) so they don't each independently
/// read-and-error-handle the same file.
pub fn read_readme(root: &Path) -> anyhow::Result<String> {
    let path = root.join("README.md");
    std::fs::read_to_string(&path)
        .map_err(|e| anyhow::anyhow!("could not read {}: {e}", path.display()))
}

/// Lists every git-tracked file path (relative to `root`). Several checks
/// care specifically about what's *committed*, not what happens to be
/// lying around in the working tree (stray local files, build output,
/// etc.), so this is the primary way checks enumerate "the repo's content".
pub fn git_ls_files(root: &Path) -> anyhow::Result<Vec<String>> {
    let output = run(root, "git", &["ls-files"])?;
    if !output.success {
        anyhow::bail!("`git ls-files` failed: {}", output.stderr);
    }
    Ok(output.stdout.lines().map(str::to_string).collect())
}

/// Runs `git grep -n <extra_args> <pattern>` against the tracked working
/// tree, restricted to `pathspecs` (an empty slice means "the whole tree"),
/// and returns matching lines (`path:lineno:content`). Git uses exit code 1
/// for "ran fine, found nothing" - that's mapped to an empty `Vec` here,
/// not an error; only a real git failure (any other nonzero code) becomes
/// an `Err`.
///
/// Always excludes `crates/whspr-check` itself: several checks grep the
/// tree for the very string literals (`todo!()`, `start_capture(`, ...)
/// that this checker's own source necessarily contains in order to search
/// for them, which would otherwise make a check flag itself.
pub fn git_grep(
    root: &Path,
    extra_args: &[&str],
    pattern: &str,
    pathspecs: &[&str],
) -> anyhow::Result<Vec<String>> {
    let mut args = vec!["grep", "-n"];
    args.extend_from_slice(extra_args);
    args.push(pattern);
    args.push("--");
    if pathspecs.is_empty() {
        args.push(".");
    } else {
        args.extend_from_slice(pathspecs);
    }
    args.push(":!crates/whspr-check");
    let output = run(root, "git", &args)?;
    match output.code {
        Some(0) => Ok(output.stdout.lines().map(str::to_string).collect()),
        Some(1) => Ok(Vec::new()),
        _ => anyhow::bail!(
            "`git grep -n {} {pattern}` failed (exit {:?}): {}",
            extra_args.join(" "),
            output.code,
            output.stderr
        ),
    }
}
