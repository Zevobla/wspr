//! The actual criterion verifications, one module per theme. Each function
//! here returns zero or more `CheckResult`s; `run_all` in this module
//! collects every family into the single ordered `Vec` `main` hands to
//! `report::print`.

pub mod architecture;
pub mod build;
pub mod cli;
pub mod config;
pub mod docs;
pub mod git_growth;
pub mod license;
pub mod privacy;
pub mod slop;
pub mod tests_isolation;

use crate::report::CheckResult;
use std::path::Path;

/// Runs every implemented check family against the repo at `root` and
/// returns all results, in catalog order (the report printer re-sorts by
/// group anyway, but keeping this in catalog order makes diffs of the raw
/// output stable).
pub fn run_all(root: &Path) -> Vec<CheckResult> {
    let mut results = Vec::new();
    results.extend(build::check_build_and_lock(root));
    results.extend(build::check_clippy(root));
    results.push(build::check_fmt(root));

    let test_results = tests_isolation::check_test_suite(root);
    let tests_passed = test_results
        .iter()
        .all(|r| r.verdict == crate::report::Verdict::Pass);
    results.extend(test_results);
    results.push(tests_isolation::check_mic_model_independence(
        root,
        tests_passed,
    ));
    results.push(tests_isolation::check_ci_configured(root));
    results.extend(tests_isolation::check_test_suite_runtime_and_determinism(
        root,
    ));

    results.push(license::check_license_file_present(root));
    results.extend(license::check_declared_license(root));
    results.push(license::check_readme_names_license(root));
    results.push(license::check_model_weights_ignored_and_absent(root));
    results.push(license::check_no_secrets_in_history(root));
    results.push(license::check_dependency_license_inventory(root));
    results.push(license::check_no_copyleft_dependencies(root));

    results.push(docs::check_readme_architecture(root));
    results.push(docs::check_readme_settings_table(root));
    results.push(docs::check_readme_swap_docs(root));
    results.push(docs::check_readme_dependencies_documented(root));
    results.push(docs::check_readme_build_steps_documented(root));
    results.push(docs::check_readme_honesty(root));

    results.push(git_growth::check_commit_count(root));
    results.extend(git_growth::check_commit_timing(root));
    results.push(git_growth::check_single_authorship(root));
    results.extend(git_growth::check_rs_commit_shape(root));
    results.push(git_growth::check_commit_distribution(root));

    match crate::repo::ensure_binary_built(root, "whspr-cli", "whspr") {
        Ok(bin) => {
            results.extend(cli::run_cli_checks(&bin, root));
            results.push(privacy::check_transcribe_offline(&bin, root));
        }
        Err(e) => {
            results.extend(cli::build_failure_results(&e.to_string()));
            results.push(CheckResult::fail(
                "P-01",
                format!("could not build whspr-cli: {e}"),
            ));
        }
    }
    results.push(privacy::check_default_backends_are_local());

    results.push(config::check_config_created_on_first_run());
    results.push(config::check_config_format_is_toml());
    results.push(config::check_config_in_platform_dir(root));

    results.push(slop::check_stub_function_count(root));
    results.push(slop::check_unused_deps(root));

    results.push(architecture::check_file_size_sanity(root));
    results.push(architecture::check_logging_single_interface(root));
    results.push(architecture::check_core_free_of_heavy_deps(root));
    results.push(architecture::check_no_circular_deps(root));

    results
}
