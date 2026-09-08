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
//! transition. `Injecting` itself maps to `Done` here too: the pipeline
//! reports `Injecting` at the instant a turn's text is handed off, which is
//! exactly the "done" moment worth a glance -- the tray just additionally
//! makes it linger.
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
/// instant is the "done" moment worth surfacing.
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

    /// Renders the icon for a `TrayVisual` bucket. Modernist: each state is
    /// a distinct *shape* (hollow square / microphone / half-filled square /
    /// inverted check), not just a color, so it stays legible in a
    /// monochrome menu bar. Colors come from `crate::theme::color` so the
    /// tray's state colors match the rest of the app.
    fn icon_for_visual(visual: TrayVisual) -> Icon {
        let scheme = &crate::theme::color::LIGHT;
        match visual {
            TrayVisual::Idle => render_square_outline(scheme.on_surface_variant),
            TrayVisual::Recording => render_microphone(scheme.error),
            TrayVisual::Processing => render_square_half(scheme.tertiary),
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

    /// Idle: a hollow 2px-outlined square -- a clean empty frame, a distinct
    /// silhouette from the other three states rather than just a dimmer color.
    fn render_square_outline(color: iced::Color) -> Icon {
        const THICKNESS: f32 = 3.0;
        render_icon(move |dx, dy| {
            (in_square(dx, dy, RADIUS) && !in_square(dx, dy, RADIUS - THICKNESS)).then_some(color)
        })
    }

    /// Processing (Transcribing|Refining): an outlined square with its left
    /// half filled -- a "half done" silhouette distinct from full/empty.
    fn render_square_half(color: iced::Color) -> Icon {
        const THICKNESS: f32 = 3.0;
        render_icon(move |dx, dy| {
            if !in_square(dx, dy, RADIUS) {
                return None;
            }
            let border = !in_square(dx, dy, RADIUS - THICKNESS);
            (border || dx <= 0.0).then_some(color)
        })
    }

    /// Done: a solid (inverted) square `fill` with a `mark`-colored
    /// checkmark stroke drawn over it.
    fn render_check(fill: iced::Color, mark: iced::Color) -> Icon {
        const HALF_WIDTH: f32 = 1.6;
        // Checkmark path, in center-relative coordinates: a short leg
        // down-right, then a longer leg up-right, sized to sit inside the
        // square with margin.
        let p0 = (-5.0, 0.5);
        let p1 = (-1.5, 4.5);
        let p2 = (5.5, -4.5);
        render_icon(move |dx, dy| {
            if !in_square(dx, dy, RADIUS) {
                return None;
            }
            let on_stroke = dist_to_segment((dx, dy), p0, p1) <= HALF_WIDTH
                || dist_to_segment((dx, dy), p1, p2) <= HALF_WIDTH;
            Some(if on_stroke { mark } else { fill })
        })
    }

    /// Recording: a bold microphone silhouette painted in `color` (the
    /// scheme's red), so "recording" reads as an unmistakable mic at a
    /// glance rather than as one more colored box.
    fn render_microphone(color: iced::Color) -> Icon {
        render_icon(move |dx, dy| in_microphone(dx, dy).then_some(color))
    }

    /// Whether `(dx, dy)` (relative to the icon's center) falls inside a
    /// square of half-side `half` -- the one silhouette Modernist uses.
    fn in_square(dx: f32, dy: f32, half: f32) -> bool {
        dx.abs() <= half && dy.abs() <= half
    }

    /// Whether `(dx, dy)` (center-relative) falls inside an axis-aligned
    /// rectangle spanning `[-half_w, half_w]` horizontally and `[y0, y1]`
    /// vertically. Backs the microphone's stem and base bar.
    fn in_rect(dx: f32, dy: f32, half_w: f32, y0: f32, y1: f32) -> bool {
        dx.abs() <= half_w && dy >= y0 && dy <= y1
    }

    /// Whether `(dx, dy)` (center-relative) falls inside the disc of radius
    /// `r` centered at `c`. Used as an annulus (outer minus inner) for the
    /// microphone's cradle.
    fn in_disc(dx: f32, dy: f32, c: (f32, f32), r: f32) -> bool {
        let (ex, ey) = (dx - c.0, dy - c.1);
        ex * ex + ey * ey <= r * r
    }

    /// Whether `(dx, dy)` (center-relative) falls within the microphone
    /// silhouette: a vertical capsule "head", a U-shaped cradle hugging its
    /// lower half, a narrow stem, and a wider base bar. Pure so `shape_tests`
    /// can assert the shape without rasterizing an `Icon`.
    fn in_microphone(dx: f32, dy: f32) -> bool {
        // Head: a vertical stadium -- a rectangle capped by two half-discs,
        // expressed as everything within `HEAD_HALF_WIDTH` of a vertical
        // segment (`dist_to_segment`) -- sitting in the upper center.
        const HEAD_HALF_WIDTH: f32 = 3.5;
        let head = dist_to_segment((dx, dy), (0.0, -5.5), (0.0, -1.5)) <= HEAD_HALF_WIDTH;
        // Cradle: the lower arc of a ring around the head, with short arms
        // rising up its sides -- the detail that reads as a mic rather than
        // a lollipop.
        let cradle_center = (0.0, -3.0);
        let cradle = in_disc(dx, dy, cradle_center, 6.0)
            && !in_disc(dx, dy, cradle_center, 4.0)
            && dy >= -4.5;
        // Stem down to, and the foot bar it stands on.
        let stem = in_rect(dx, dy, 1.2, 3.0, 6.0);
        let base = in_rect(dx, dy, 5.0, 6.0, 8.0);
        head || cradle || stem || base
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
        fn in_square_includes_center_and_corner_but_excludes_beyond_half() {
            assert!(in_square(0.0, 0.0, 5.0));
            // A square includes its corners (unlike a disc of the same
            // "radius"): (5, 5) is in a half-5 square.
            assert!(in_square(5.0, 5.0, 5.0));
            assert!(!in_square(6.0, 0.0, 5.0));
        }

        #[test]
        fn in_square_outline_excludes_the_center_hole() {
            // The 3px-thick outline: inside the hole is excluded, the
            // border band is included, outside the square is excluded.
            let border = |dx: f32, dy: f32| in_square(dx, dy, 10.0) && !in_square(dx, dy, 7.0);
            assert!(!border(0.0, 0.0));
            assert!(border(9.0, 0.0));
            assert!(!border(11.0, 0.0));
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

        #[test]
        fn in_rect_is_inclusive_within_bounds_and_empty_outside() {
            assert!(in_rect(0.0, 5.0, 2.0, 4.0, 6.0));
            // Beyond the half-width, or past the vertical span, is empty.
            assert!(!in_rect(3.0, 5.0, 2.0, 4.0, 6.0));
            assert!(!in_rect(0.0, 7.0, 2.0, 4.0, 6.0));
        }

        #[test]
        fn in_disc_includes_center_but_excludes_a_far_point() {
            assert!(in_disc(0.0, 0.0, (0.0, 0.0), 3.0));
            assert!(in_disc(2.0, 0.0, (0.0, 0.0), 3.0));
            assert!(!in_disc(4.0, 0.0, (0.0, 0.0), 3.0));
        }

        #[test]
        fn microphone_head_paints_but_a_far_corner_is_transparent() {
            // A point on the head's vertical centerline is inside the mic...
            assert!(in_microphone(0.0, -3.5));
            // ...while a top corner of the canvas is clearly outside it.
            assert!(!in_microphone(-9.0, -9.0));
        }

        #[test]
        fn microphone_base_bar_is_wider_than_its_stem() {
            // Out near the base's edge is filled (the foot bar)...
            assert!(in_microphone(4.5, 7.0));
            // ...but the same column higher up, level with the narrow stem,
            // is empty.
            assert!(!in_microphone(4.5, 4.5));
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
        assert_eq!(visual_for(PipelineState::Refining), TrayVisual::Processing);
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
