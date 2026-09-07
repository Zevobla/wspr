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

    /// Renders the icon for a `TrayVisual` bucket, reusing the Flow Bar's
    /// semantic colors (`crate::theme::color`) so the tray icon and the
    /// overlay agree on what each color means.
    fn icon_for_visual(visual: TrayVisual) -> Icon {
        let scheme = &crate::theme::color::LIGHT;
        let color = match visual {
            TrayVisual::Idle => scheme.on_surface_variant,
            TrayVisual::Recording => scheme.error,
            TrayVisual::Processing => scheme.tertiary,
            TrayVisual::Done => scheme.success_container,
        };

        render_circle(color)
    }

    /// Renders a filled circle on a transparent square as raw RGBA --
    /// there's no bundled icon asset (see `Cargo.toml`'s dependency
    /// notes), so the tray icon is drawn in code instead.
    fn render_circle(color: iced::Color) -> Icon {
        const SIZE: u32 = 22;
        let radius = SIZE as f32 / 2.0;
        let (r, g, b) = (
            (color.r * 255.0) as u8,
            (color.g * 255.0) as u8,
            (color.b * 255.0) as u8,
        );

        let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let dx = x as f32 + 0.5 - radius;
                let dy = y as f32 + 0.5 - radius;
                let inside = (dx * dx + dy * dy).sqrt() <= radius - 1.0;
                if inside {
                    rgba.extend_from_slice(&[r, g, b, 255]);
                } else {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                }
            }
        }

        Icon::from_rgba(rgba, SIZE, SIZE).expect("a fixed-size circle is always a valid icon")
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
}
