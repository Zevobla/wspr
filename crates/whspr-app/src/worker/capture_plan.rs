//! Pure decisions behind a live capture: which input device to open (and
//! the fallback when the configured one is gone), the `CaptureOptions` the
//! user's `[capture]` settings translate to, how a finished clip is
//! trimmed, and whether an idle preroll monitor should be listening. Kept
//! free of cpal so every branch is unit-testable without a microphone.

/// The input device a capture should open, after checking the configured
/// name against the devices actually connected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DeviceResolution {
    /// Device name to open; `None` opens the OS default input device.
    pub device: Option<String>,
    /// The configured device name when it is not connected right now --
    /// capture then falls back to the default device and the worker tells
    /// the user so (see `crate::devices::fallback_notice`).
    pub missing: Option<String>,
}

/// Resolves `[device].input_device` (`configured`) against `available` (as
/// from `whspr_audio::input_device_names`). An empty `available` list means
/// enumeration itself failed (no microphone permission yet, a host error)
/// rather than every device vanishing, so the configured name is kept and
/// `whspr_audio`'s own name resolution decides.
pub(super) fn resolve_device(configured: Option<&str>, available: &[String]) -> DeviceResolution {
    match configured {
        Some(name) if available.is_empty() || available.iter().any(|a| a == name) => {
            DeviceResolution {
                device: Some(name.to_string()),
                missing: None,
            }
        }
        Some(name) => DeviceResolution {
            device: None,
            missing: Some(name.to_string()),
        },
        None => DeviceResolution {
            device: None,
            missing: None,
        },
    }
}
