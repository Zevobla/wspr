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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_reload_reads_edited_file_from_disk() {
        let dir = std::env::temp_dir().join(format!("whspr-reload-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "language = \"en\"\n").unwrap();
        let first = config_reload(&dir).expect("first reload");
        assert_eq!(first.language.as_deref(), Some("en"));

        // Editing the file and reloading picks up the change without a restart.
        std::fs::write(&path, "language = \"ru\"\n").unwrap();
        let second = config_reload(&dir).expect("second reload");
        assert_eq!(second.language.as_deref(), Some("ru"));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn config_reload_errors_when_file_missing() {
        let dir = std::env::temp_dir().join("whspr-reload-nonexistent-xyz");
        assert!(config_reload(&dir).is_err());
    }

    #[test]
    fn api_key_for_returns_configured_key() {
        let mut config = Config::default();
        config
            .api_keys
            .insert("openai".to_string(), "k".to_string());
        assert_eq!(api_key_for(&config, "openai"), Some("k".to_string()));
        assert_eq!(api_key_for(&config, "missing"), None);
    }
}
