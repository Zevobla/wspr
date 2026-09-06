//! System tray icon (B-11): reflects the pipeline's coarse state (idle /
//! recording / thinking) and offers a "Show Hub" / "Quit" menu.
//!
//! ## Threading (macOS/Windows)
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
//! instead (see `tray_poll_subscription` in `crate::app`) -- a small,
//! deliberate latency trade (up to the poll interval) for not needing a
//! winit `EventLoopProxy` hook into iced's internals. `MenuEvent::
//! receiver()` is thread-agnostic (just a channel tray-icon's platform
//! backend pushes into), so this same polling approach works regardless
//! of which thread actually owns the tray icon -- including Linux's,
//! below.
//!
//! ## Platform support
//! macOS and Windows integrate with the very event loop iced already
//! pumps (see above). Linux's tray-icon backend
//! (`libappindicator`/`libayatana-appindicator` over D-Bus) needs its own
//! live GLib main loop to actually deliver menu-click events; winit's
//! Linux backend talks to X11/Wayland directly and never pumps GLib's on
//! its own. So Linux gets a dedicated OS thread that calls `gtk::init()`
//! and `gtk::main()` for the app's lifetime, hosting the tray icon there
//! -- `Handle::create` still returns quickly (a bounded handshake, not an
//! indefinite block), and `set_state` forwards updates into that thread
//! over a channel rather than touching the (thread-affine, `!Send`)
//! `TrayIcon` directly. Any other platform gets an inert stub: callers
//! get `None`/no-ops instead of a broken tray.

use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};
use whspr_core::PipelineState;

/// Fixed ids for the two menu items -- shared by every platform's
/// creation code and by `poll_action`'s click resolution, so there's no
/// need to read them back off the `MenuItem`s after creation.
const SHOW_HUB_ID: &str = "show-hub";
const QUIT_ID: &str = "quit";

/// What a menu click resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    ShowHub,
    Quit,
}

/// Drains every pending menu click, returning the last one (if any).
/// `MenuEvent::receiver()` is a thread-agnostic channel (see the module
/// doc comment), so this one function covers every platform.
fn poll_action() -> Option<Action> {
    let mut action = None;
    while let Ok(event) = MenuEvent::receiver().try_recv() {
        if event.id().0 == SHOW_HUB_ID {
            action = Some(Action::ShowHub);
        } else if event.id().0 == QUIT_ID {
            action = Some(Action::Quit);
        }
    }
    action
}

/// Builds the menu + tray icon for `state`. Shared by every real
/// platform; what differs between them is which thread calls this and
/// how the result is held (see the platform modules below).
fn build_tray(state: PipelineState) -> Option<TrayIcon> {
    let menu = Menu::new();
    let show_hub = MenuItem::with_id(SHOW_HUB_ID, "Show Hub", true, None);
    let quit = MenuItem::with_id(QUIT_ID, "Quit", true, None);
    menu.append(&show_hub).ok()?;
    menu.append(&quit).ok()?;

    TrayIconBuilder::new()
        .with_icon(icon_for(state))
        .with_tooltip("whspr")
        .with_menu(Box::new(menu))
        .build()
        .ok()
}

/// A small flat-colored circle, tinted per pipeline state -- reuses the
/// Flow Bar's semantic colors (`crate::theme::color`) so the tray icon
/// and the overlay agree on what each color means.
fn icon_for(state: PipelineState) -> Icon {
    let scheme = &crate::theme::color::LIGHT;
    let color = match state {
        PipelineState::Idle => scheme.on_surface_variant,
        PipelineState::Recording | PipelineState::Error => scheme.error,
        PipelineState::Transcribing | PipelineState::Refining => scheme.tertiary,
        PipelineState::Injecting => scheme.success_container,
    };

    render_circle(color)
}

/// Renders a filled circle on a transparent square as raw RGBA -- there's
/// no bundled icon asset (see `Cargo.toml`'s dependency notes), so the
/// tray icon is drawn in code instead.
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

#[cfg(any(target_os = "macos", target_os = "windows"))]
mod platform {
    use tray_icon::TrayIcon;
    use whspr_core::PipelineState;

    use super::{build_tray, icon_for, poll_action, Action};

    /// A live tray icon. Not `Debug` (`TrayIcon` isn't), so `State`'s
    /// `Option<Handle>` field relies on this hand-written, deliberately
    /// opaque impl.
    pub struct Handle {
        icon: TrayIcon,
    }

    impl std::fmt::Debug for Handle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("tray::Handle").finish_non_exhaustive()
        }
    }

    impl Handle {
        /// Creates the tray icon and its menu. See the module doc comment
        /// for the threading/timing requirements this relies on the
        /// caller to satisfy.
        pub fn create(state: PipelineState) -> Option<Self> {
            build_tray(state).map(|icon| Self { icon })
        }

        /// Updates the icon's color to reflect a new pipeline state.
        /// Guarded: a failure here is cosmetic, never worth surfacing.
        pub fn set_state(&self, state: PipelineState) {
            let _ = self.icon.set_icon(Some(icon_for(state)));
        }

        pub fn poll_action(&self) -> Option<Action> {
            poll_action()
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use whspr_core::PipelineState;

    use super::{build_tray, icon_for, poll_action, Action};

    /// How long `create` waits for the GTK thread to report whether it
    /// managed to stand up the tray icon, before giving up and returning
    /// `None`. Bounded so a stuck/slow GTK/D-Bus environment can never
    /// hang the winit thread that calls this.
    const CREATE_TIMEOUT: Duration = Duration::from_secs(2);
    /// How often the GTK thread checks for a pending state update.
    const STATE_POLL_INTERVAL: Duration = Duration::from_millis(100);

    /// A channel into the dedicated GTK thread that actually owns the
    /// `TrayIcon` -- `tray_icon::TrayIcon` wraps an `Rc<RefCell<..>>`
    /// internally (`!Send`), so it can never leave the thread it was
    /// created on. `set_state` forwards a request instead of touching it
    /// directly.
    pub struct Handle {
        state_tx: mpsc::Sender<PipelineState>,
    }

    impl std::fmt::Debug for Handle {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("tray::Handle").finish_non_exhaustive()
        }
    }

    impl Handle {
        /// Spawns the dedicated GTK-main-loop thread and waits (with a
        /// bound) for it to report whether the tray icon actually came
        /// up. See the module doc comment for why Linux needs its own
        /// thread at all.
        pub fn create(state: PipelineState) -> Option<Self> {
            let (ready_tx, ready_rx) = mpsc::channel::<bool>();
            let (state_tx, state_rx) = mpsc::channel::<PipelineState>();

            thread::Builder::new()
                .name("whspr-tray-gtk".to_string())
                .spawn(move || run_gtk_thread(state, ready_tx, state_rx))
                .ok()?;

            match ready_rx.recv_timeout(CREATE_TIMEOUT) {
                Ok(true) => Some(Self { state_tx }),
                _ => None,
            }
        }

        pub fn set_state(&self, state: PipelineState) {
            let _ = self.state_tx.send(state);
        }

        pub fn poll_action(&self) -> Option<Action> {
            poll_action()
        }
    }

    /// Runs on its own thread for the app's lifetime: initializes GTK,
    /// builds the tray icon, then pumps `gtk::main()` forever so
    /// libappindicator's D-Bus-delivered menu clicks actually get
    /// dispatched (`crate::tray`'s top doc comment has the full
    /// reasoning for why this thread needs to exist at all).
    fn run_gtk_thread(
        initial_state: PipelineState,
        ready_tx: mpsc::Sender<bool>,
        state_rx: mpsc::Receiver<PipelineState>,
    ) {
        if gtk::init().is_err() {
            let _ = ready_tx.send(false);
            return;
        }

        let Some(tray) = build_tray(initial_state) else {
            let _ = ready_tx.send(false);
            return;
        };

        let _ = ready_tx.send(true);

        // Polls the plain `std::sync::mpsc` receiver from inside GTK's
        // own loop. `timeout_add_local` (unlike `timeout_add`) never
        // requires its closure to be `Send`, since it's guaranteed to
        // only ever run on the thread that registered it -- exactly this
        // one -- so moving `tray` (`!Send`) into it is fine.
        glib::source::timeout_add_local(STATE_POLL_INTERVAL, move || {
            while let Ok(state) = state_rx.try_recv() {
                let _ = tray.set_icon(Some(icon_for(state)));
            }
            glib::ControlFlow::Continue
        });

        gtk::main();
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    use whspr_core::PipelineState;

    use super::Action;

    /// Not implemented on this platform.
    #[derive(Debug)]
    pub struct Handle;

    impl Handle {
        pub fn create(_state: PipelineState) -> Option<Self> {
            None
        }

        pub fn set_state(&self, _state: PipelineState) {}

        pub fn poll_action(&self) -> Option<Action> {
            None
        }
    }
}

pub use platform::Handle;
