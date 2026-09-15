//! The installer's state machine: which screen is showing, the install
//! options the user has toggled, and the simulated install progress.
//!
//! The flow is `Install -> Installing -> Done`, with `Failure` reachable via
//! the `WHSPR_SETUP_SCREEN` env path (and, once the real install lands, from
//! a genuine write error). See `crate::update` for the transitions.

use std::path::PathBuf;

use iced::window;

use crate::install::Job;

/// One of the installer's screens, carrying the state that screen needs.
#[derive(Debug, Clone)]
pub enum Screen {
    /// The landing screen: a headline, the pitch, and the Install/Options
    /// buttons. `expanded` reveals the three option toggle rows below the
    /// buttons; the three bools are those options' values.
    Install {
        expanded: bool,
        autostart: bool,
        start_menu: bool,
        desktop: bool,
    },
    /// The progress screen: a bar walking 0->100 while the step label cycles.
    /// `progress` is a percentage, driven by the real install's steps (see
    /// `crate::install`).
    Installing { progress: u8 },
    /// The success screen.
    Done,
    /// The error screen (a step couldn't complete). `detail` carries the real
    /// error when reached from a genuine install failure; `None` renders the
    /// canned design (the `WHSPR_SETUP_SCREEN=failure` self-validation path).
    /// `at` is the bar value the install stopped at, for the header status.
    Failure { detail: Option<String>, at: u8 },
}

impl Screen {
    /// The default landing screen: options collapsed, launch-at-login and
    /// the start-menu shortcut on, the desktop shortcut off.
    pub fn install_default() -> Self {
        Screen::Install {
            expanded: false,
            autostart: true,
            start_menu: true,
            desktop: false,
        }
    }
}

/// The four steps the simulated install walks, in order. The step shown is
/// derived from `Installing`'s progress (0/25/50/75 thresholds) so the label
/// and the bar can never drift out of sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    CopyingFiles,
    CreatingShortcuts,
    RegisteringHotkey,
    FinishingUp,
}

impl Step {
    /// The step in progress at `progress` percent.
    pub fn at(progress: u8) -> Self {
        match progress {
            0..=24 => Step::CopyingFiles,
            25..=49 => Step::CreatingShortcuts,
            50..=74 => Step::RegisteringHotkey,
            _ => Step::FinishingUp,
        }
    }

    /// The step's uppercase label.
    pub fn label(self) -> &'static str {
        match self {
            Step::CopyingFiles => "COPYING FILES",
            Step::CreatingShortcuts => "CREATING SHORTCUTS",
            Step::RegisteringHotkey => "REGISTERING HOTKEY",
            Step::FinishingUp => "FINISHING UP",
        }
    }
}

/// Everything the installer draws from.
#[derive(Debug)]
pub struct State {
    pub screen: Screen,
    /// The installer window's id, learned once it opens; needed to drive
    /// window drag and the headless screenshot.
    pub window: Option<window::Id>,
    /// The headless-screenshot output path, from `WHSPR_SETUP_SCREENSHOT`.
    /// `None` in a normal run.
    pub screenshot_path: Option<PathBuf>,
    /// Whether the one screenshot has already been taken (so the timer stops
    /// firing after it).
    pub screenshot_taken: bool,
}

impl State {
    /// Boots into the screen `WHSPR_SETUP_SCREEN` asks for (defaulting to the
    /// landing screen) and reads the screenshot output path from the env.
    pub fn from_env() -> Self {
        State {
            screen: screen_from_env(),
            window: None,
            screenshot_path: crate::screenshot::path_from_env(),
            screenshot_taken: false,
        }
    }
}

/// The screen `WHSPR_SETUP_SCREEN` selects. Used both by the headless
/// screenshot harness and to launch any screen live for a look. Defaults to
/// the landing screen when unset or unrecognized. `installing` seeds a
/// mid-run 45% so the bar and the "Creating shortcuts" step are both visible.
pub fn screen_from_env() -> Screen {
    match std::env::var("WHSPR_SETUP_SCREEN")
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "install-expanded" | "expanded" | "options" => Screen::Install {
            expanded: true,
            autostart: true,
            start_menu: true,
            desktop: false,
        },
        "installing" => Screen::Installing { progress: 45 },
        "done" => Screen::Done,
        "failure" | "error" => Screen::Failure {
            detail: None,
            at: 62,
        },
        _ => Screen::install_default(),
    }
}

/// Every message the installer's `update` handles.
#[derive(Debug, Clone)]
pub enum Message {
    /// The window finished opening; carries its id.
    WindowOpened(window::Id),
    /// The header band was pressed -- start dragging the window.
    Drag,
    /// The close mark was pressed -- exit.
    Close,
    /// The Options button was pressed -- expand/collapse the option rows.
    ToggleOptions,
    /// A "launch at sign-in" toggle press.
    ToggleAutostart,
    /// A "start-menu shortcut" toggle press.
    ToggleStartMenu,
    /// A "desktop shortcut" toggle press.
    ToggleDesktop,
    /// The Install button was pressed -- begin the real install.
    StartInstall,
    /// A real install step finished off the UI thread. `jobs` is the plan and
    /// `index` the job that just ran; `result` is its outcome. On `Ok` the bar
    /// advances and the next job dispatches (or `InstallSucceeded` fires); on
    /// `Err` the installer routes to the Failure screen.
    InstallStepped {
        jobs: Vec<Job>,
        index: usize,
        result: Result<(), String>,
    },
    /// Every job completed -- switch to the Done screen.
    InstallSucceeded,
    /// The "Open whspr" button was pressed.
    OpenApp,
    /// The "Retry" button was pressed -- back to the landing screen.
    Retry,
    /// The screenshot timer fired -- capture the window.
    TakeScreenshot,
    /// The capture completed; carries the RGBA to encode.
    ScreenshotTaken(window::Screenshot),
}
