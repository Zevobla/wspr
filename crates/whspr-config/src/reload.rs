//! Config file re-reading (B-05) and API-key lookup helpers, split out of
//! `lib.rs` to keep that module focused on the `Config` type itself.

use std::path::Path;

use crate::Config;

/// Re-reads the config file from disk (B-05). Useful when the user has
/// edited the config file and needs to pick up changes without a full
/// restart.
pub fn config_reload(path: &Path) -> whspr_core::Result<Config> {
    let config_path = path.join("config.toml");

    if config_path.exists() {
        let contents = std::fs::read_to_string(&config_path).map_err(|e| {
            whspr_core::WhsprError::Config(format!("failed to read config file: {e}"))
        })?;

        toml::from_str::<Config>(&contents).map_err(|e| {
            whspr_core::WhsprError::Config(format!("failed to parse config TOML: {e}"))
        })
    } else {
        Err(whspr_core::WhsprError::Config(
            "config file not found".to_string(),
        ))
    }
}

/// Looks up an API key for a cloud backend by id (e.g. "openai") from the
/// given config's `api_keys` table. Reads only `config`; never an
/// environment variable.
pub fn api_key_for(config: &Config, choice_id: &str) -> Option<String> {
    config.api_keys.get(choice_id).cloned()
}
