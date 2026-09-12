//! The in-flight model-download progress model and its Modernist progress-bar
//! view. Split out of `crate::hf` / `crate::state` so both stay under the
//! AA-06 600-line cap, and so the byte-rate bookkeeping lives next to the
//! widget that renders it. Fed by `Message::HfDownloadProgress` (driven by a
//! `whspr_hf::DownloadProgress` stream bridged into iced -- see
//! [`progress_task`]) and cleared when the download finishes (see
//! `crate::hf`'s `downloaded`).

use std::time::Instant;

use iced::futures::stream;
use iced::Task;
use tokio::sync::mpsc::UnboundedReceiver;
use whspr_hf::{human_size, DownloadProgress};

use crate::state::Message;

/// Live state for the one download currently in flight on the Models screen:
/// what's being fetched, how far along it is, and a smoothed transfer rate so
/// the label can show MB/s. `total` stays 0 until the server's
/// `Content-Length` arrives (hf-hub's `Progress::init`); the view treats that
/// as an indeterminate, percent-less state.
#[derive(Debug, Clone)]
pub struct ActiveDownload {
    /// The model id / filename being downloaded, shown in the label.
    pub label: String,
    /// Bytes fetched so far (monotonic, from `DownloadProgress::downloaded`).
    pub downloaded: u64,
    /// Total bytes to fetch, or 0 while still unknown.
    pub total: u64,
    /// The last (time, bytes) sample the rate is derived from; `None` until
    /// the first update establishes a baseline.
    last_sample: Option<(Instant, u64)>,
    /// The smoothed transfer rate in bytes/sec (EMA), or `None` until enough
    /// samples have accrued to estimate one.
    rate_bps: Option<f64>,
}

impl ActiveDownload {
    /// Starts tracking a fresh download of `label`, before any bytes arrive.
    pub fn new(label: String) -> Self {
        Self {
            label,
            downloaded: 0,
            total: 0,
            last_sample: None,
            rate_bps: None,
        }
    }

    /// Folds in a byte-count update, re-estimating the transfer rate at most a
    /// couple of times a second so a burst of tiny updates doesn't make the
    /// rate jitter.
    pub fn update(&mut self, downloaded: u64, total: u64) {
        const RATE_WINDOW_SECS: f64 = 0.5;
        const SMOOTHING: f64 = 0.3;
        let now = Instant::now();
        self.total = total;
        match self.last_sample {
            Some((then, bytes)) => {
                let dt = now.duration_since(then).as_secs_f64();
                if dt >= RATE_WINDOW_SECS {
                    let instant = downloaded.saturating_sub(bytes) as f64 / dt;
                    self.rate_bps = Some(match self.rate_bps {
                        Some(prev) => prev * (1.0 - SMOOTHING) + instant * SMOOTHING,
                        None => instant,
                    });
                    self.last_sample = Some((now, downloaded));
                }
            }
            None => self.last_sample = Some((now, downloaded)),
        }
        self.downloaded = downloaded;
    }

    /// The completion fraction in `0.0..=1.0`, or `None` while `total` is
    /// still unknown (an indeterminate download).
    pub fn fraction(&self) -> Option<f32> {
        (self.total > 0).then(|| (self.downloaded as f64 / self.total as f64).min(1.0) as f32)
    }

    /// The one-line status label, e.g.
    /// `Downloading large-v3 · 42% · 1.3 GB / 3.1 GB · 3.1 MB/s`. Drops the
    /// percent + total while `total` is unknown, and the rate until one is
    /// estimated.
    fn status_line(&self) -> String {
        let mut line = format!("Downloading {}", self.label);
        match self.fraction() {
            Some(fraction) => {
                let percent = (fraction * 100.0).round() as u32;
                line.push_str(&format!(
                    " · {percent}% · {} / {}",
                    human_size(self.downloaded),
                    human_size(self.total)
                ));
            }
            None => line.push_str(&format!(" · {} so far", human_size(self.downloaded))),
        }
        if let Some(rate) = self.rate_bps {
            if rate >= 1.0 {
                line.push_str(&format!(" · {}/s", human_size(rate as u64)));
            }
        }
        line
    }
}

/// Bridges a `whspr_hf::DownloadProgress` receiver into iced: each byte-count
/// update the download emits comes back to `update` as a
/// `Message::HfDownloadProgress`. The stream ends -- and so does the task --
/// when the download drops its sender, i.e. the moment the transfer finishes,
/// so this runs exactly as long as the download does and never leaks. Runs
/// alongside the completion `Task::perform` (batched by the caller in
/// `crate::hf`). Built on `iced::futures` (iced's own re-export of the
/// `futures` crate), so it needs no extra dependency.
pub fn progress_task(rx: UnboundedReceiver<DownloadProgress>) -> Task<Message> {
    let updates = stream::unfold(rx, |mut rx| async move {
        rx.recv().await.map(|update| (update, rx))
    });
    Task::run(updates, |update| Message::HfDownloadProgress {
        downloaded: update.downloaded,
        total: update.total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fraction_is_none_until_total_known() {
        let download = ActiveDownload::new("large-v3".to_string());
        assert_eq!(download.fraction(), None);
    }

    #[test]
    fn fraction_tracks_downloaded_over_total() {
        let mut download = ActiveDownload::new("large-v3".to_string());
        download.update(500, 1000);
        assert_eq!(download.fraction(), Some(0.5));
    }

    #[test]
    fn fraction_is_clamped_to_one() {
        let mut download = ActiveDownload::new("large-v3".to_string());
        download.update(1200, 1000);
        assert_eq!(download.fraction(), Some(1.0));
    }

    #[test]
    fn status_line_shows_id_and_percent_when_total_known() {
        let mut download = ActiveDownload::new("large-v3".to_string());
        download.update(500, 1000);
        let line = download.status_line();
        assert!(line.contains("large-v3"), "got: {line}");
        assert!(line.contains("50%"), "got: {line}");
    }

    #[test]
    fn status_line_is_percent_less_when_total_unknown() {
        let mut download = ActiveDownload::new("mystery".to_string());
        download.update(1024 * 1024, 0);
        let line = download.status_line();
        assert!(!line.contains('%'), "got: {line}");
        assert!(line.contains("so far"), "got: {line}");
    }
}
