//! Live preroll capture: keeps a `PrerollBuffer` continuously topped up
//! from an open input stream while the app is idle (before the user has
//! pressed the hotkey to start a real capture), so the first word of an
//! utterance can be recovered even though the real recording technically
//! started a fraction of a second late (criterion E-10).
//!
//! # Contract
//!
//! A `PrerollMonitor` should only be constructed when *both*:
//! - the app actually wants preroll (an explicit opt-in — it means an
//!   input stream is open essentially all the time the app is idle,
//!   rather than only while the user is actively dictating), **and**
//! - `whspr-config`'s `[privacy] mic_privacy` is **off** — when
//!   `mic_privacy` is on, the whole point is to release the mic outside
//!   active capture, which a standing preroll stream would directly
//!   defeat.
//!
//! This module doesn't read `Config` itself (this crate doesn't depend on
//! `whspr-config`) — the caller (the app's worker) is responsible for
//! checking both conditions before calling `start`.

use std::sync::{Arc, Mutex};

use cpal::traits::DeviceTrait;
use cpal::traits::HostTrait;
use cpal::traits::StreamTrait;
use cpal::StreamConfig;

use whspr_core::{AudioBuffer, Result};

use crate::capture::{mic_access_error, no_input_device_error};
use crate::device;
use crate::preroll::PrerollBuffer;

/// A live, continuously-refreshed preroll ring buffer backed by an open
/// input stream.
///
/// Construct with `start`, read the current ring contents at any time
/// with `snapshot` (non-destructive — unlike `PrerollBuffer::drain_preroll`,
/// repeated calls don't reset it, since the monitor keeps listening), and
/// release the input device with `stop`.
pub struct PrerollMonitor {
    stream: cpal::Stream,
    buffer: Arc<Mutex<PrerollBuffer>>,
    sample_rate: u32,
}

impl PrerollMonitor {
    /// Opens `device` by name (or the OS default input device if `None`,
    /// same resolution rules as `start_capture_on_device`) and starts
    /// continuously feeding a `PrerollBuffer` sized for `preroll_ms`
    /// milliseconds at the device's native sample rate.
    ///
    /// See the module doc for when this should (and shouldn't) be called.
    pub fn start(device: Option<&str>, preroll_ms: u32) -> Result<PrerollMonitor> {
        let host = cpal::default_host();
        let device = match device {
            Some(name) => device::resolve_input_device(&host, name)?,
            None => host
                .default_input_device()
                .ok_or_else(no_input_device_error)?,
        };

        let config = device
            .default_input_config()
            .map_err(|e| mic_access_error("get default input config", e))?;
        let sample_rate = config.sample_rate();
        let stream_config: StreamConfig = config.into();

        let ring = PrerollBuffer::from_ms(preroll_ms as usize, sample_rate);
        let buffer = Arc::new(Mutex::new(ring));
        let buffer_clone = Arc::clone(&buffer);

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => device
                .build_input_stream(
                    stream_config,
                    move |data: &[f32], _: &_| {
                        if let Ok(mut buf) = buffer_clone.lock() {
                            buf.push_slice(data);
                        }
                    },
                    |err| tracing::error!("cpal preroll stream error: {}", err),
                    None,
                )
                .map_err(|e| mic_access_error("build F32 preroll stream", e))?,
            cpal::SampleFormat::I16 => device
                .build_input_stream(
                    stream_config,
                    move |data: &[i16], _: &_| {
                        if let Ok(mut buf) = buffer_clone.lock() {
                            for sample in data {
                                buf.push(*sample as f32 / 32768.0);
                            }
                        }
                    },
                    |err| tracing::error!("cpal preroll stream error: {}", err),
                    None,
                )
                .map_err(|e| mic_access_error("build I16 preroll stream", e))?,
            cpal::SampleFormat::U16 => device
                .build_input_stream(
                    stream_config,
                    move |data: &[u16], _: &_| {
                        if let Ok(mut buf) = buffer_clone.lock() {
                            for sample in data {
                                let s = *sample as f32 - 32768.0;
                                buf.push(s / 32768.0);
                            }
                        }
                    },
                    |err| tracing::error!("cpal preroll stream error: {}", err),
                    None,
                )
                .map_err(|e| mic_access_error("build U16 preroll stream", e))?,
            other => {
                return Err(whspr_core::WhsprError::Audio(format!(
                    "unsupported sample format: {other:?}"
                )));
            }
        };

        stream
            .play()
            .map_err(|e| mic_access_error("start preroll stream", e))?;

        Ok(PrerollMonitor {
            stream,
            buffer,
            sample_rate,
        })
    }

    /// Returns the ring's current contents, resampled to 16kHz mono — the
    /// same shape `CaptureHandle::stop` produces, so the result can be
    /// handed straight to `CaptureOptions::preroll`.
    ///
    /// Non-destructive: the underlying ring keeps accumulating, so a later
    /// `snapshot` call sees more (or different) data, not an emptied
    /// buffer. A monitor that hasn't received any audio yet (or whose
    /// resample fails, which in practice only happens for a malformed
    /// sample rate) returns an empty `Vec`.
    pub fn snapshot(&self) -> Vec<f32> {
        let Ok(buf) = self.buffer.lock() else {
            return Vec::new();
        };
        let samples = buf.contents();
        drop(buf);

        if samples.is_empty() {
            return Vec::new();
        }

        let native = AudioBuffer::new(samples, self.sample_rate);
        crate::resample_to_16k_mono(&native)
            .map(|resampled| resampled.samples)
            .unwrap_or_default()
    }

    /// Closes the input stream, releasing the device. Any buffered
    /// preroll samples are dropped along with it — call `snapshot` first
    /// if the caller still needs them (typically right before starting a
    /// real `CaptureHandle`).
    pub fn stop(self) {
        drop(self.stream);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preroll::DEFAULT_PREROLL_MS;

    #[test]
    fn start_without_device_does_not_panic() {
        // Mirrors device::tests::default_input_device_name_does_not_panic:
        // in a sandboxed/headless environment (no input device, or one
        // present but access-denied) this returns Err; on a machine with a
        // real, accessible default input device it returns Ok. Either is
        // fine - only a panic would be a bug. If it does succeed, release
        // the stream again immediately.
        if let Ok(monitor) = PrerollMonitor::start(None, DEFAULT_PREROLL_MS as u32) {
            monitor.stop();
        }
    }

    #[test]
    fn snapshot_on_fresh_monitor_is_empty() {
        // Same environment caveat as above: only assert when construction
        // actually succeeds. Immediately after `start`, the callback
        // hasn't necessarily delivered any audio yet, so the ring should
        // still read as empty.
        if let Ok(monitor) = PrerollMonitor::start(None, DEFAULT_PREROLL_MS as u32) {
            assert!(
                monitor.snapshot().is_empty(),
                "a monitor that hasn't had time to receive any audio yet should snapshot empty"
            );
            monitor.stop();
        }
    }
}
