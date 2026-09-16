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

/// Resolves [`Delivery`] for a dictation that just finished, querying the
/// focused element only when detection is enabled. The Accessibility query
/// is cross-process, so it runs on a blocking thread.
pub(super) async fn resolve_delivery(detection_enabled: bool) -> Delivery {
    let field_editable = if detection_enabled {
        tokio::task::spawn_blocking(whspr_inject::focused_field_is_editable)
            .await
            .ok()
            .flatten()
    } else {
        None
    };
    delivery_for(detection_enabled, field_editable)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_definite_non_text_focus_with_detection_on_copies_to_clipboard() {
        let cases = [
            ((true, Some(false)), Delivery::Clipboard),
            ((true, Some(true)), Delivery::Inject),
            ((true, None), Delivery::Inject),
            ((false, Some(false)), Delivery::Inject),
            ((false, None), Delivery::Inject),
        ];
        for ((enabled, editable), expected) in cases {
            assert_eq!(
                delivery_for(enabled, editable),
                expected,
                "detection={enabled} editable={editable:?}"
            );
        }
    }

    #[tokio::test]
    async fn detection_off_injects_without_querying_focus() {
        assert_eq!(resolve_delivery(false).await, Delivery::Inject);
    }
}
