//! How a finished dictation reaches the user: typed into the focused app, or
//! -- when `[capture].input_field_detection` finds nothing there that takes
//! text -- put on the clipboard with a notice saying so.

/// Where a completed dictation goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// Type it into the focused app.
    Inject,
    /// Nothing that takes text has focus: copy it to the clipboard instead
    /// and tell the user ([`CLIPBOARD_NOTICE`]).
    Clipboard,
}

/// The notice shown when a dictation went to the clipboard instead of being
/// typed.
pub(crate) const CLIPBOARD_NOTICE: &str = "No text field focused \u{2014} copied to clipboard";

/// Decides [`Delivery`] from whether field detection is on and what it
/// found (`whspr_inject::focused_field_is_editable`). Only a definite "the
/// focused element is not a text field" diverts to the clipboard; an
/// undeterminable answer (`None`) keeps today's typing behaviour.
pub(super) fn delivery_for(detection_enabled: bool, field_editable: Option<bool>) -> Delivery {
    if detection_enabled && field_editable == Some(false) {
        Delivery::Clipboard
    } else {
        Delivery::Inject
    }
}
