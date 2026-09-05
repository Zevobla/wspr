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
/// bytes) and whether it exited zero.
pub struct CmdOutput {
    pub success: bool,
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
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    })
}
