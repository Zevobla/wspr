//! The Settings tab's `update` arms. Every Settings control does the same
//! shape of work -- mutate one `state.config` field then `persist_config`
//! -- and touches nothing else in the app, so the arms live here instead of
//! bloating `crate::app::update` past this project's 600-line-per-file
//! guideline (AA-06). `crate::app::update` forwards any message it doesn't
//! handle itself to `update` below.

use iced::Task;

use crate::app::persist_config;
use crate::config_ui;
use crate::devices::NoticeUpdate;
use crate::secret_store::{remove_secret, save_secret, SecretSlot};
use crate::state::{Message, State};

/// Handles a Settings-tab control message: mutates the matching
/// `state.config` field, persists, and reports no follow-up task. Called
/// from `crate::app::update`'s catch-all, so in practice `message` is
/// always one of the Settings variants; the `_` arm keeps the match total
/// for anything the caller might forward in the future (a harmless no-op).
pub(crate) fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::NoiseSuppressionToggled(enabled) => {
            state.config.capture.noise_suppression = enabled;
            persist_config(state);
        }
        Message::InputGainChanged(value) => {
            state.config.capture.input_gain = value.clamp(0.0, 3.0);
            persist_config(state);
        }
        Message::VadThresholdChanged(value) => {
            state.config.capture.vad_threshold = value.clamp(0.0, 1.0);
            persist_config(state);
        }
        Message::TranslateToggled(enabled) => {
            state.config.capture.translate = enabled;
            persist_config(state);
        }
        Message::ShortenToggled(enabled) => {
            state.config.capture.shorten = enabled;
            persist_config(state);
        }
        Message::AutoSendToggled(enabled) => {
            state.config.capture.auto_send = enabled;
            persist_config(state);
        }
        Message::InputFieldDetectionToggled(enabled) => {
            state.config.capture.input_field_detection = enabled;
            persist_config(state);
        }
        Message::RefineTimeoutMsChanged(text) => {
            state.refine_timeout_draft = text.clone();
            if let Ok(value) = text.trim().parse::<u64>() {
                state.config.capture.refine_timeout_ms = value.clamp(0, 600_000);
                persist_config(state);
            }
        }
        Message::PrePasteDelayMsChanged(text) => {
            state.pre_paste_delay_draft = text.clone();
            if let Ok(value) = text.trim().parse::<u64>() {
                state.config.injection.pre_paste_delay_ms = value.clamp(0, 10_000);
                persist_config(state);
            }
        }
        Message::MicPrivacyToggled(enabled) => {
            state.config.privacy.mic_privacy = enabled;
            persist_config(state);
        }
        Message::HistoryEncryptionToggled(enabled) => {
            if crate::history_encryption::set_history_encryption(state, enabled) {
                persist_config(state);
            }
        }
        Message::CookieBrowserChanged(browser) => {
            state.config.privacy.cookies_browser = browser;
            persist_config(state);
        }
        Message::DeviceHotplugToggled(enabled) => {
            state.config.device.device_hotplug = enabled;
            persist_config(state);
        }
        Message::ActiveWindowToggled(enabled) => {
            state.config.device.active_window = enabled;
            persist_config(state);
        }
        Message::BluetoothSourceToggled(enabled) => {
            state.config.device.bluetooth_source = enabled;
            persist_config(state);
        }
        Message::VirtualSourceToggled(enabled) => {
            state.config.device.virtual_source = enabled;
            persist_config(state);
        }
        Message::InputDevicesChanged(change) => {
            state.input_devices =
                crate::devices::apply_device_change(&state.input_devices, &change);
            let configured = state.config.device.input_device.as_deref();
            match crate::devices::hotplug_notice(configured, &change, state.notice.as_deref()) {
                NoticeUpdate::Show(notice) => state.notice = Some(notice),
                NoticeUpdate::Clear => state.notice = None,
                NoticeUpdate::Keep => {}
            }
        }
        Message::TrayStaticToggled(enabled) => {
            state.config.device.tray_static = enabled;
            if let Some(tray) = &state.tray {
                tray.set_state(state.pipeline_state, enabled);
            }
            persist_config(state);
        }
        Message::NormalizeNumbersToggled(enabled) => {
            state.config.normalize.numbers = enabled;
            persist_config(state);
        }
        Message::NormalizeDatesToggled(enabled) => {
            state.config.normalize.dates = enabled;
            persist_config(state);
        }
        Message::NormalizeTimesToggled(enabled) => {
            state.config.normalize.times = enabled;
            persist_config(state);
        }
        Message::NumberFormatSelected(label) => {
            state.config.normalize.numbers_format = config_ui::number_format_from_label(label);
            persist_config(state);
        }
        Message::ParagraphBreakToggled(enabled) => {
            state.config.normalize.paragraph_break = enabled;
            persist_config(state);
        }
        Message::PunctuationToggleToggled(enabled) => {
            state.config.normalize.punctuation_toggle = enabled;
            persist_config(state);
        }
        Message::ApiKeyDraftChanged(id, draft) => {
            state.api_key_drafts.insert(id, draft);
        }
        Message::ApiKeySaved(id) => {
            let draft = state.api_key_drafts.remove(id).unwrap_or_default();
            let key = draft.trim();
            if !key.is_empty() {
                match save_secret(state, SecretSlot::ApiKey(id), key) {
                    Ok(()) => persist_config(state),
                    Err(error) => {
                        state.last_error = Some(format!("API key not saved: {error}"));
                        state.api_key_drafts.insert(id, draft);
                    }
                }
            }
        }
        Message::ApiKeyRemoved(id) => {
            if let Err(error) = remove_secret(state, SecretSlot::ApiKey(id)) {
                state.last_error = Some(format!("API key not fully removed: {error}"));
            }
            persist_config(state);
        }
        // UI-only state (no config write): the Settings sub-nav selection
        // and the History search box. Handled here because this is the sink
        // `crate::app::update` forwards every unrecognized message to.
        Message::SettingsSectionSelected(section) => {
            state.settings_section = section;
        }
        Message::HistorySearchChanged(query) => {
            state.history_search = query;
        }
        _ => {}
    }
    Task::none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unplugging_the_configured_device_updates_the_list_and_warns() {
        let mut config = whspr_config::Config::default();
        config.device.input_device = Some("USB Mic".to_string());
        let mut state = State::new(config);
        state.input_devices = vec!["Built-in Microphone".to_string(), "USB Mic".to_string()];

        let _ = update(
            &mut state,
            Message::InputDevicesChanged(whspr_audio::DeviceChange {
                added: vec![],
                removed: vec!["USB Mic".to_string()],
            }),
        );

        assert_eq!(state.input_devices, vec!["Built-in Microphone".to_string()]);
        assert_eq!(
            state.notice,
            Some(crate::devices::fallback_notice("USB Mic"))
        );
    }

    #[test]
    fn typing_an_api_key_only_updates_the_draft() {
        let mut state = State::new(whspr_config::Config::default());

        let _ = update(
            &mut state,
            Message::ApiKeyDraftChanged("openai", "sk-draft".to_string()),
        );

        assert_eq!(
            state.api_key_drafts.get("openai").map(String::as_str),
            Some("sk-draft")
        );
        assert!(state.config.api_keys.is_empty());
    }
}
