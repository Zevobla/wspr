//! System tray icon (B-11): reflects the pipeline's coarse state (idle /
//! recording / thinking) and offers a "Show Hub" / "Quit" menu.
//!
//! ## Threading
//! `Handle::create` must run on the same thread as iced's winit event
//! loop, *after* that loop is genuinely pumping -- `tray-icon`'s own docs
//! call out both requirements (creating one before the loop is truly
//! running is called out as risky on macOS in particular). iced's
//! `update`/`view` run synchronously on that thread (they're polled
//! directly from inside the winit `ApplicationHandler` callbacks, not
//! moved onto the executor like `Task` futures are), so `crate::app`
//! calls `Handle::create` from `update`'s `HubOpened` arm -- never from
//! inside a `Task::perform`/`Task::future` body, which *would* run on the
//! tokio executor instead. By the time `HubOpened` arrives, a real OS
//! window has already been created, so the loop is unambiguously running.
//!
//! `tray-icon`'s own event channels (`MenuEvent::receiver()`) are plain
//! `Receiver`s, not a `Stream` iced can subscribe to directly, so
//! `crate::app` polls `Handle::poll_action` from a ticking `Subscription`
//! instead (see `tray_poll_subscription`) -- a small, deliberate latency
//! trade (up to the poll interval) for not needing a winit
//! `EventLoopProxy` hook into iced's internals.
//!
//! ## Visual states
//! `PipelineState` (from `whspr-core`) has no "just finished" variant --
//! the pipeline reports `Injecting` then immediately `Idle`, with nothing
//! in between for a user to actually glance at. So the tray's icon is
//! driven by `TrayVisual`, a small app-local enum `visual_for` maps
//! `PipelineState` onto, with one extra bucket (`Done`) `PipelineState`
//! itself can't represent. `crate::app` drives that extra bucket directly
//! via `Handle::set_visual`, timing a lingering "Done" display off
//! `WorkerEvent::Completed` rather than off any single `PipelineState`
//! transition. `Injecting` itself maps to `Done` here too, matching
//! `crate::flow_bar`'s existing precedent (`base_colors_for`), which
//! already treats `Injecting` as "Done" -- both agree on what that
//! instant means, it's just the tray that additionally makes it linger.
//!
//! ## Platform support
//! Implemented for macOS and Windows only, both of which integrate with
//! the very event loop iced already pumps. Linux's tray-icon backend
//! (`libappindicator`/`libayatana-appindicator` over D-Bus) needs its own
//! live GLib main loop to deliver menu-click events; winit's Linux
//! backend talks to X11/Wayland directly and does not pump GLib's, so
//! menu clicks would simply never arrive. Bridging that means running a
//! second toolkit's event loop on its own thread and proxying its events
//! back into iced's `Subscription` system -- a separate integration
//! project, not a wire-up, so it's deliberately not attempted here (see
//! the final report for the write-up). The platform-gated module below
//! compiles to an inert stub on Linux: callers get `None`/no-ops instead
//! of a broken tray.

use whspr_core::PipelineState;

/// The tray icon's visual identity -- decoupled from `PipelineState` (see
/// the module doc comment) so "Done" can be represented without a core
/// change. Four buckets, each rendered as a distinct shape *and* color
/// (see `platform::icon_for_visual`), not just a color swap on the same
/// dot, so idle/recording/processing/done stay distinguishable even to a
/// glance that can't tell the color hues apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayVisual {
    Idle,
    Recording,
    Processing,
    Done,
}

/// Maps a raw pipeline state onto its tray visual bucket. Pure, so it's
/// unit-testable without a live tray icon (see the `tests` module below).
/// `Injecting` maps to `Done` -- see the module doc comment for why that
/// mirrors `crate::flow_bar`'s existing treatment of that state.
fn visual_for(state: PipelineState) -> TrayVisual {
    match state {
        PipelineState::Idle => TrayVisual::Idle,
        PipelineState::Recording | PipelineState::Error => TrayVisual::Recording,
        PipelineState::Transcribing | PipelineState::Refining => TrayVisual::Processing,
        PipelineState::Injecting => TrayVisual::Done,
    }
}

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform {
    use tray_icon::menu::{Menu, MenuEvent, MenuItem};
    use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

    use whspr_core::PipelineState;

    use super::TrayVisual;

    /// A live tray icon plus the menu item ids needed to tell which one
    /// was clicked. Not `Debug` (`TrayIcon` isn't), so `State`'s `Option<
    /// Handle>` field relies on this hand-written, deliberately opaque
    /// impl.
    pub struct Handle {
        icon: TrayIcon,
        show_hub_id: String,
        quit_id: String,
    }

    impl std::fmt::Debug for Handle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("tray::Handle").finish_non_exhaustive()
        }
    }

    /// What a menu click resolves to.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        ShowHub,
        Quit,
    }

    impl Handle {
        /// Creates the tray icon and its menu. See the module doc comment
        /// for the threading/timing requirements this relies on the
        /// caller to satisfy.
        pub fn create(state: PipelineState) -> Option<Self> {
            let menu = Menu::new();
            let show_hub = MenuItem::with_id("show-hub", "Show Hub", true, None);
            let quit = MenuItem::with_id("quit", "Quit", true, None);
            menu.append(&show_hub).ok()?;
            menu.append(&quit).ok()?;

            let icon = TrayIconBuilder::new()
                .with_icon(icon_for(state))
                .with_tooltip("whspr")
                .with_menu(Box::new(menu))
                .build()
                .ok()?;

            Some(Self {
                icon,
                show_hub_id: show_hub.id().0.clone(),
                quit_id: quit.id().0.clone(),
            })
        }

        /// Updates the icon to reflect a new pipeline state (via
        /// `super::visual_for`). Guarded: a failure here is cosmetic,
        /// never worth surfacing.
        pub fn set_state(&self, state: PipelineState) {
            self.set_visual(super::visual_for(state));
        }

        /// Updates the icon directly to `visual`, bypassing the
        /// `PipelineState` mapping -- used by `crate::app` for the
        /// lingering "Done" display after `WorkerEvent::Completed`, a
        /// moment `PipelineState` alone can't represent (see the module
        /// doc comment). Guarded like `set_state`.
        pub fn set_visual(&self, visual: TrayVisual) {
            let _ = self.icon.set_icon(Some(icon_for_visual(visual)));
        }

        /// Drains every pending menu click, returning the last one (if
        /// any) as an `Action`. Polled rather than pushed -- see the
        /// module doc comment.
        pub fn poll_action(&self) -> Option<Action> {
            let mut action = None;
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                if event.id().0 == self.show_hub_id {
                    action = Some(Action::ShowHub);
                } else if event.id().0 == self.quit_id {
                    action = Some(Action::Quit);
                }
            }
            action
        }
    }

    fn icon_for(state: PipelineState) -> Icon {
        icon_for_visual(super::visual_for(state))
    }

    /// Renders the icon for a `TrayVisual` bucket -- a distinct shape
    /// *and* color per bucket (see `TrayVisual`'s doc comment), reusing
    /// the Flow Bar's semantic colors (`crate::theme::color`) so the tray
    /// icon and the overlay agree on what each color means.
    fn icon_for_visual(visual: TrayVisual) -> Icon {
        let scheme = &crate::theme::color::LIGHT;
        match visual {
            TrayVisual::Idle => render_ring(scheme.on_surface_variant),
            TrayVisual::Recording => render_dot(scheme.error),
            TrayVisual::Processing => render_diamond(scheme.tertiary),
            TrayVisual::Done => render_check(scheme.success_container, scheme.on_success_container),
        }
    }

    /// Side length of the square icon canvas, in pixels.
    const SIZE: u32 = 22;
    /// Outer radius every shape below is inscribed within, inset 1px from
    /// the canvas edge so nothing gets clipped.
    const RADIUS: f32 = SIZE as f32 / 2.0 - 1.0;

    /// Rasterizes a `SIZE`x`SIZE` tray icon by evaluating `paint` at the
    /// center of every pixel, in coordinates relative to the icon's
    /// center (so the shape math in each `render_*` helper below reads
    /// naturally) -- shared so the RGBA pixel-buffer bookkeeping lives in
    /// one place instead of once per shape.
    fn render_icon(paint: impl Fn(f32, f32) -> Option<iced::Color>) -> Icon {
        let center = SIZE as f32 / 2.0;
        let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = x as f32 + 0.5 - center;
                let dy = y as f32 + 0.5 - center;
                match paint(dx, dy) {
                    Some(color) => rgba.extend_from_slice(&to_rgba8(color)),
                    None => rgba.extend_from_slice(&[0, 0, 0, 0]),
                }
            }
        }
        Icon::from_rgba(rgba, SIZE, SIZE).expect("a fixed-size icon is always valid")
    }

    fn to_rgba8(color: iced::Color) -> [u8; 4] {
        [
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
            255,
        ]
    }

    /// Recording: a solid filled dot.
    fn render_dot(color: iced::Color) -> Icon {
        render_icon(move |dx, dy| in_disc(dx, dy, RADIUS).then_some(color))
    }

    /// Idle: a hollow ring -- visually the opposite of Recording's filled
    /// dot (empty vs. full) rather than just a dimmer color.
    fn render_ring(color: iced::Color) -> Icon {
        const THICKNESS: f32 = 4.0;
        render_icon(move |dx, dy| in_ring(dx, dy, RADIUS, RADIUS - THICKNESS).then_some(color))
    }

    /// Processing (Transcribing|Refining): a filled diamond -- a
    /// corner-having silhouette a dot/ring never produces, so it reads as
    /// a different shape even at tray-icon size, not just a different
    /// color.
    fn render_diamond(color: iced::Color) -> Icon {
        render_icon(move |dx, dy| in_diamond(dx, dy, RADIUS).then_some(color))
    }

    /// Done: a filled circle (`fill`) with a checkmark stroke (`mark`)
    /// drawn over it.
    fn render_check(fill: iced::Color, mark: iced::Color) -> Icon {
        const HALF_WIDTH: f32 = 1.6;
        // Checkmark path, in the same center-relative coordinates as
        // `paint` below: a short leg down-right, then a longer leg
        // up-right, sized to sit inside the circle with margin.
        let p0 = (-5.0, 0.5);
        let p1 = (-1.5, 4.5);
        let p2 = (5.5, -4.5);
        render_icon(move |dx, dy| {
            if !in_disc(dx, dy, RADIUS) {
                return None;
            }
            let on_stroke = dist_to_segment((dx, dy), p0, p1) <= HALF_WIDTH
                || dist_to_segment((dx, dy), p1, p2) <= HALF_WIDTH;
            Some(if on_stroke { mark } else { fill })
        })
    }

    /// Whether `(dx, dy)` (relative to the icon's center) falls inside a
    /// disc of `radius`.
    fn in_disc(dx: f32, dy: f32, radius: f32) -> bool {
        (dx * dx + dy * dy).sqrt() <= radius
    }

    /// Whether `(dx, dy)` falls in the annulus between `inner` and
    /// `outer` -- `in_disc` with a hole cut out of the middle.
    fn in_ring(dx: f32, dy: f32, outer: f32, inner: f32) -> bool {
        let dist = (dx * dx + dy * dy).sqrt();
        dist <= outer && dist >= inner
    }

    /// Whether `(dx, dy)` falls inside a diamond (a square rotated 45
    /// degrees) of `radius` -- taxicab distance instead of Euclidean, so
    /// its silhouette has corners a disc never has.
    fn in_diamond(dx: f32, dy: f32, radius: f32) -> bool {
        dx.abs() + dy.abs() <= radius
    }

    /// Euclidean distance from point `p` to the segment `a`-`b`, clamping
    /// the projection onto the segment (rather than the infinite line) so
    /// a point past either endpoint measures to that endpoint. Used by
    /// `render_check` to rasterize the checkmark's two strokes without a
    /// vector-graphics dependency.
    fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
        let ab = (b.0 - a.0, b.1 - a.1);
        let len_sq = ab.0 * ab.0 + ab.1 * ab.1;
        let t = if len_sq > 0.0 {
            (((p.0 - a.0) * ab.0 + (p.1 - a.1) * ab.1) / len_sq).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let closest = (a.0 + t * ab.0, a.1 + t * ab.1);
        let d = (p.0 - closest.0, p.1 - closest.1);
        (d.0 * d.0 + d.1 * d.1).sqrt()
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;

        #[test]
        fn in_disc_includes_center_and_excludes_beyond_radius() {
            assert!(in_disc(0.0, 0.0, 5.0));
            assert!(!in_disc(6.0, 0.0, 5.0));
        }

        #[test]
        fn in_ring_excludes_the_center_hole() {
            assert!(!in_ring(0.0, 0.0, 10.0, 6.0));
            assert!(in_ring(8.0, 0.0, 10.0, 6.0));
            assert!(!in_ring(11.0, 0.0, 10.0, 6.0));
        }

        /// The whole reason Processing reads as a different shape than a
        /// plain filled circle: at radius 10, (7, 7) is inside the disc
        /// (distance ~9.9) but outside the diamond (|7| + |7| = 14 > 10).
        #[test]
        fn in_diamond_excludes_a_disc_corner_point_the_disc_would_include() {
            assert!(in_disc(7.0, 7.0, 10.0));
            assert!(!in_diamond(7.0, 7.0, 10.0));
        }

        #[test]
        fn dist_to_segment_is_zero_on_the_segment() {
            let d = dist_to_segment((0.0, 0.0), (-2.0, 0.0), (2.0, 0.0));
            assert!(d < 1e-6);
        }

        #[test]
        fn dist_to_segment_matches_perpendicular_distance_at_the_midpoint() {
            let d = dist_to_segment((0.0, 3.0), (-2.0, 0.0), (2.0, 0.0));
            assert!((d - 3.0).abs() < 1e-6);
        }

        #[test]
        fn dist_to_segment_clamps_to_the_nearest_endpoint_beyond_the_segment() {
            let d = dist_to_segment((5.0, 0.0), (-2.0, 0.0), (2.0, 0.0));
            assert!((d - 3.0).abs() < 1e-6);
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
mod platform {
    use whspr_core::PipelineState;

    use super::TrayVisual;

    /// Not implemented on this platform -- see the module doc comment.
    #[derive(Debug)]
    pub struct Handle;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        ShowHub,
        Quit,
    }

    impl Handle {
        pub fn create(_state: PipelineState) -> Option<Self> {
            None
        }

        pub fn set_state(&self, _state: PipelineState) {}

        pub fn set_visual(&self, _visual: TrayVisual) {}

        pub fn poll_action(&self) -> Option<Action> {
            None
        }
    }
}

pub use platform::{Action, Handle};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_enum_has_expected_variants() {
        let show_hub = Action::ShowHub;
        let quit = Action::Quit;
        assert_eq!(show_hub, Action::ShowHub);
        assert_eq!(quit, Action::Quit);
        assert_ne!(show_hub, quit);
    }

    #[test]
    fn action_is_copy() {
        let action = Action::ShowHub;
        // `Action` is `Copy`, so this assignment copies rather than moves --
        // `action` stays usable on the next line.
        let copied = action;
        assert_eq!(action, copied);
    }

    #[test]
    fn action_implements_eq() {
        assert!(Action::ShowHub == Action::ShowHub);
        assert!(Action::Quit == Action::Quit);
        assert!(Action::ShowHub != Action::Quit);
    }

    #[test]
    fn idle_maps_to_idle_visual() {
        assert_eq!(visual_for(PipelineState::Idle), TrayVisual::Idle);
    }

    #[test]
    fn recording_and_error_share_the_recording_visual() {
        assert_eq!(visual_for(PipelineState::Recording), TrayVisual::Recording);
        assert_eq!(visual_for(PipelineState::Error), TrayVisual::Recording);
    }

    #[test]
    fn transcribing_and_refining_share_the_processing_visual() {
        assert_eq!(
            visual_for(PipelineState::Transcribing),
            TrayVisual::Processing
        );
        assert_eq!(
            visual_for(PipelineState::Refining),
            TrayVisual::Processing
        );
    }

    #[test]
    fn injecting_maps_to_the_done_visual() {
        assert_eq!(visual_for(PipelineState::Injecting), TrayVisual::Done);
    }

    #[test]
    fn every_visual_bucket_is_distinct() {
        let idle = visual_for(PipelineState::Idle);
        let recording = visual_for(PipelineState::Recording);
        let processing = visual_for(PipelineState::Transcribing);
        let done = visual_for(PipelineState::Injecting);

        let buckets = [idle, recording, processing, done];
        for (i, a) in buckets.iter().enumerate() {
            for (j, b) in buckets.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "buckets {i} and {j} must be visually distinct");
                }
            }
        }
    }
}
