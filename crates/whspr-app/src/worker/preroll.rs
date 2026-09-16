//! The worker's idle preroll monitor: keeps a `whspr_audio::PrerollMonitor`
//! open exactly when `capture_plan::desired_monitor` says one should be, so
//! the syllable spoken right as the hotkey goes down is not clipped.

use whspr_audio::{PrerollMonitor, DEFAULT_PREROLL_MS};

use super::capture_plan::{monitor_action, MonitorAction, MonitorTarget};

/// Owns at most one running preroll monitor, together with the target it
/// was opened on.
#[derive(Default)]
pub(super) struct PrerollSlot {
    running: Option<(PrerollMonitor, MonitorTarget)>,
}

impl PrerollSlot {
    /// Starts, stops or reopens the monitor so it matches `desired` (see
    /// `capture_plan::monitor_action`). Returns a user-facing message when a
    /// monitor was wanted but the device could not be opened; dictation
    /// itself keeps working, just without preroll.
    pub(super) fn reconcile(&mut self, desired: Option<MonitorTarget>) -> Result<(), String> {
        let running = self.running.as_ref().map(|(_, target)| target);
        let target = match monitor_action(running, desired) {
            MonitorAction::Keep => return Ok(()),
            MonitorAction::Stop => {
                self.stop();
                return Ok(());
            }
            MonitorAction::Start(target) => target,
            MonitorAction::Restart(target) => {
                self.stop();
                target
            }
        };
        let monitor = PrerollMonitor::start(target.device.as_deref(), DEFAULT_PREROLL_MS as u32)
            .map_err(|error| {
                format!(
                    "Pre-roll is off: the microphone could not stay open between \
                     dictations ({error})."
                )
            })?;
        self.running = Some((monitor, target));
        Ok(())
    }

    /// The last `DEFAULT_PREROLL_MS` of audio for a capture about to open
    /// `device`, or nothing when no monitor is running on that same device
    /// (preroll from a different microphone would splice two sources).
    pub(super) fn snapshot_for(&self, device: &Option<String>) -> Vec<f32> {
        match &self.running {
            Some((monitor, target)) if target.device == *device => monitor.snapshot(),
            _ => Vec::new(),
        }
    }

    /// Closes the running monitor, if any, releasing the input device.
    fn stop(&mut self) {
        if let Some((monitor, _)) = self.running.take() {
            monitor.stop();
        }
    }
}
