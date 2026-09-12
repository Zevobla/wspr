//! Detecting which browsers are actually installed, so the import sign-in
//! picker offers only real cookie sources instead of a fixed list.
//!
//! A browser "counts" only when a real cookie **database** exists — not merely
//! its app-support folder, which can linger as an empty leftover after an
//! uninstall (yt-dlp reports exactly that: "could not find <id> cookies
//! database in <path>"). So we probe the actual store: `<profile>/Cookies`
//! (or `<profile>/Network/Cookies`) for Chromium-family browsers,
//! `Profiles/<p>/cookies.sqlite` for Firefox, the `.binarycookies` file for
//! Safari.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

/// An installed browser whose logged-in cookies `yt-dlp` can borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieBrowser {
    /// The `--cookies-from-browser` id (`firefox`, `chrome`, `safari`, …).
    pub id: &'static str,
    /// A display label for the picker (`Firefox`, `Chrome`, …).
    pub label: &'static str,
}

/// Where a browser keeps its cookie store, and how to tell it's really there.
enum Store {
    /// A Chromium user-data dir; the DB is `<profile>/Cookies` or
    /// `<profile>/Network/Cookies` for some profile subdir (Default, Profile N).
    Chromium(PathBuf),
    /// A Firefox base dir; the DB is `cookies.sqlite` under a profile, whether
    /// nested in `Profiles/` (macOS/Windows) or a direct child (Linux).
    Firefox(PathBuf),
    /// One or more concrete cookie files; present if any exists (Safari).
    Files(Vec<PathBuf>),
}

impl Store {
    fn present(&self) -> bool {
        match self {
            Store::Files(paths) => paths.iter().any(|p| p.exists()),
            Store::Chromium(dir) => profiles_in(dir).iter().any(|p| {
                p.join("Cookies").exists() || p.join("Network").join("Cookies").exists()
            }),
            Store::Firefox(dir) => {
                let mut roots = profiles_in(&dir.join("Profiles"));
                roots.extend(profiles_in(dir));
                roots.iter().any(|p| p.join("cookies.sqlite").exists())
            }
        }
    }
}

/// The immediate subdirectories (profile candidates) of `dir`; empty if `dir`
/// can't be read. Owned paths so no directory handle outlives the call.
fn profiles_in(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect()
}

/// The browsers installed on this machine (a real cookie DB exists), in a
/// stable preference order (Firefox/Chrome first — they carry a signed-in
/// session that defeats YouTube's bot wall; Safari last). Empty if the home
/// directory can't be resolved. Filesystem probes only, no subprocess.
pub fn installed_cookie_browsers() -> Vec<CookieBrowser> {
    let Some(base) = BaseDirs::new() else {
        return Vec::new();
    };
    candidates(&base)
        .into_iter()
        .filter(|(_, _, store)| store.present())
        .map(|(id, label, _)| CookieBrowser { id, label })
        .collect()
}

/// Per-platform `(id, label, cookie store)` candidates, in preference order.
fn candidates(base: &BaseDirs) -> Vec<(&'static str, &'static str, Store)> {
    #[cfg(target_os = "macos")]
    {
        let home = base.home_dir();
        let app = home.join("Library/Application Support");
        vec![
            ("firefox", "Firefox", Store::Firefox(app.join("Firefox"))),
            ("chrome", "Chrome", Store::Chromium(app.join("Google/Chrome"))),
            (
                "brave",
                "Brave",
                Store::Chromium(app.join("BraveSoftware/Brave-Browser")),
            ),
            ("edge", "Edge", Store::Chromium(app.join("Microsoft Edge"))),
            ("chromium", "Chromium", Store::Chromium(app.join("Chromium"))),
            ("vivaldi", "Vivaldi", Store::Chromium(app.join("Vivaldi"))),
            (
                "opera",
                "Opera",
                Store::Chromium(app.join("com.operasoftware.Opera")),
            ),
            (
                "safari",
                "Safari",
                Store::Files(vec![
                    home.join(
                        "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
                    ),
                    home.join("Library/Cookies/Cookies.binarycookies"),
                ]),
            ),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let local = base.data_local_dir();
        let roaming = base.data_dir();
        vec![
            (
                "firefox",
                "Firefox",
                Store::Firefox(roaming.join("Mozilla/Firefox")),
            ),
            (
                "chrome",
                "Chrome",
                Store::Chromium(local.join("Google/Chrome/User Data")),
            ),
            (
                "brave",
                "Brave",
                Store::Chromium(local.join("BraveSoftware/Brave-Browser/User Data")),
            ),
            (
                "edge",
                "Edge",
                Store::Chromium(local.join("Microsoft/Edge/User Data")),
            ),
            (
                "chromium",
                "Chromium",
                Store::Chromium(local.join("Chromium/User Data")),
            ),
            (
                "vivaldi",
                "Vivaldi",
                Store::Chromium(local.join("Vivaldi/User Data")),
            ),
        ]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let home = base.home_dir();
        let config = base.config_dir();
        vec![
            (
                "firefox",
                "Firefox",
                Store::Firefox(home.join(".mozilla/firefox")),
            ),
            (
                "chrome",
                "Chrome",
                Store::Chromium(config.join("google-chrome")),
            ),
            (
                "brave",
                "Brave",
                Store::Chromium(config.join("BraveSoftware/Brave-Browser")),
            ),
            ("edge", "Edge", Store::Chromium(config.join("microsoft-edge"))),
            ("chromium", "Chromium", Store::Chromium(config.join("chromium"))),
            ("vivaldi", "Vivaldi", Store::Chromium(config.join("vivaldi"))),
            ("opera", "Opera", Store::Chromium(config.join("opera"))),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_returns_only_known_unique_ids_without_panicking() {
        // Filesystem-dependent, so assert the contract rather than an exact
        // set: every detected browser is a known yt-dlp id, with no dupes.
        let known = [
            "firefox", "chrome", "brave", "edge", "chromium", "vivaldi", "opera", "safari",
        ];
        let found = installed_cookie_browsers();
        for b in &found {
            assert!(known.contains(&b.id), "unexpected browser id: {}", b.id);
        }
        let mut ids: Vec<&str> = found.iter().map(|b| b.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "detected browsers should be unique");
    }

    #[test]
    fn empty_leftover_dir_is_not_detected() {
        // A Chromium user-data dir with no Cookies DB (an uninstall leftover)
        // must not count as installed.
        let tmp = std::env::temp_dir().join(format!("whspr-browsers-{}", std::process::id()));
        let _ = std::fs::create_dir_all(tmp.join("Default"));
        assert!(!Store::Chromium(tmp.clone()).present());
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
