//! The Settings tab's `update` arms. Every Settings control does the same
//! shape of work -- mutate one `state.config` field then `persist_config`
//! -- and touches nothing else in the app, so the arms live here instead of
//! bloating `crate::app::update` past this project's 600-line-per-file
//! guideline (AA-06). `crate::app::update` forwards any message it doesn't
//! handle itself to `update` below.

use iced::Task;

use crate::app::persist_config;
use crate::config_ui;
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
            state.config.privacy.history_encryption = enabled;
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
        Message::TrayStaticToggled(enabled) => {
            state.config.device.tray_static = enabled;
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
        Message::ApiKeyChanged(id, value) => {
            state.config.api_keys.insert(id.to_string(), value);
            persist_config(state);
        }
        _ => {}
    }
    Task::none()
}
