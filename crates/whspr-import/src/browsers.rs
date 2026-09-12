//! Detecting the browsers actually installed on this machine — by their real
//! names — so the import sign-in picker reflects reality instead of a fixed
//! engine list.
//!
//! `yt-dlp --cookies-from-browser` only takes a fixed set of engine ids
//! (chrome, firefox, brave, …). To support Firefox forks it has no id for
//! (Zen, IceCat, LibreWolf, camoufox, …) we pass `firefox:<profile-path>`,
//! which reads that fork's `cookies.sqlite` directly (verified equivalent to
//! plain `firefox`). So detection splits by engine family:
//!
//! - **Firefox family — discovered generically.** Any app-data subdir holding
//!   a `Profiles/<p>/cookies.sqlite` is a Firefox-format browser; the folder
//!   name is its real name and the spec is `firefox:<path>` (plain `firefox`
//!   for the stock single-profile install). No hardcoded fork list — new forks
//!   appear automatically.
//! - **Chromium family — a known-id table.** Dozens of Electron apps ship the
//!   same `Cookies` DB, so a generic scan would be meaningless; instead probe
//!   the yt-dlp-supported browsers by their real dirs, one entry per real
//!   profile (`chrome`, `chrome:Profile 1`, …).
//! - **Safari** — its `.binarycookies` file.

use std::path::{Path, PathBuf};

use directories::BaseDirs;

/// An installed browser whose logged-in cookies `yt-dlp` can borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieBrowser {
    /// Real display name — a Firefox fork's folder name, or the browser name
    /// for Chromium/Safari — with a profile suffix when there's more than one.
    pub label: String,
    /// The `yt-dlp --cookies-from-browser` argument (`firefox`,
    /// `firefox:/path/to/profile`, `chrome`, `chrome:Profile 1`, `safari`).
    pub spec: String,
}

/// Every installed browser with a real cookie store, Firefox family first
/// (they defeat YouTube's bot wall), then Chromium browsers, then Safari.
/// Filesystem probes only, no subprocess.
pub fn installed_cookie_browsers() -> Vec<CookieBrowser> {
    let Some(base) = BaseDirs::new() else {
        return Vec::new();
    };
    let mut out = firefox_family(&base);
    out.extend(chromium_family(&base));
    out.extend(safari(&base));
    out
}

/// The immediate children of `dir`, sorted for stable order; empty if `dir`
/// can't be read (owned paths, so no directory handle outlives the call).
fn read_children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    v.sort();
    v
}

fn dir_name(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

/// Emits a [`CookieBrowser`] per Firefox-format profile found under `app_root`
/// — every `<name>/Profiles/<p>/cookies.sqlite`. `name` is the browser's real
/// folder name; the stock single-profile Firefox uses the bare `firefox` spec,
/// everything else (forks, extra profiles) uses `firefox:<path>`.
fn firefox_from_root(app_root: &Path, out: &mut Vec<CookieBrowser>) {
    for child in read_children(app_root) {
        let name = dir_name(&child);
        let profiles: Vec<PathBuf> = read_children(&child.join("Profiles"))
            .into_iter()
            .filter(|p| p.join("cookies.sqlite").exists())
            .collect();
        let stock = name.eq_ignore_ascii_case("Firefox");
        let single = profiles.len() == 1;
        for prof in &profiles {
            let spec = if stock && single {
                "firefox".to_string()
            } else {
                format!("firefox:{}", prof.display())
            };
            let label = if single {
                name.clone()
            } else {
                format!("{name} ({})", dir_name(prof))
            };
            out.push(CookieBrowser { label, spec });
        }
    }
}

#[cfg(target_os = "macos")]
fn firefox_family(base: &BaseDirs) -> Vec<CookieBrowser> {
    let mut out = Vec::new();
    firefox_from_root(&base.home_dir().join("Library/Application Support"), &mut out);
    out
}

#[cfg(not(target_os = "macos"))]
fn firefox_family(base: &BaseDirs) -> Vec<CookieBrowser> {
    // Windows/Linux keep Firefox-family under a browser-specific root rather
    // than one shared dir, so the generic scan doesn't apply cleanly; detect
    // the stock Firefox profile (fork discovery here is a later refinement).
    #[cfg(target_os = "windows")]
    let root = base.data_dir().join("Mozilla/Firefox");
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let root = base.home_dir().join(".mozilla/firefox");

    let nested = read_children(&root.join("Profiles"));
    let direct = read_children(&root);
    let present = nested
        .iter()
        .chain(direct.iter())
        .any(|p| p.join("cookies.sqlite").exists());
    if present {
        vec![CookieBrowser {
            label: "Firefox".to_string(),
            spec: "firefox".to_string(),
        }]
    } else {
        Vec::new()
    }
}

/// Chromium-family: one entry per real login profile of each yt-dlp-supported
/// browser present. Guest/System profiles are skipped — they never hold a
/// user's YouTube session.
fn chromium_family(base: &BaseDirs) -> Vec<CookieBrowser> {
    let mut out = Vec::new();
    for (root, id, label) in chromium_candidates(base) {
        let profiles: Vec<PathBuf> = read_children(&root)
            .into_iter()
            .filter(|p| {
                let n = dir_name(p);
                n != "System Profile"
                    && n != "Guest Profile"
                    && (p.join("Cookies").exists() || p.join("Network").join("Cookies").exists())
            })
            .collect();
        let single = profiles.len() == 1;
        for prof in &profiles {
            let pname = dir_name(prof);
            let spec = if single {
                id.to_string()
            } else {
                format!("{id}:{pname}")
            };
            let label = if single {
                label.to_string()
            } else {
                format!("{label} ({pname})")
            };
            out.push(CookieBrowser { label, spec });
        }
    }
    out
}

/// `(user-data dir, yt-dlp id, display label)` for each supported Chromium
/// browser on this platform.
fn chromium_candidates(base: &BaseDirs) -> Vec<(PathBuf, &'static str, &'static str)> {
    #[cfg(target_os = "macos")]
    {
        let app = base.home_dir().join("Library/Application Support");
        vec![
            (app.join("Google/Chrome"), "chrome", "Chrome"),
            (app.join("Chromium"), "chromium", "Chromium"),
            (app.join("BraveSoftware/Brave-Browser"), "brave", "Brave"),
            (app.join("Microsoft Edge"), "edge", "Edge"),
            (app.join("Vivaldi"), "vivaldi", "Vivaldi"),
            (app.join("com.operasoftware.Opera"), "opera", "Opera"),
        ]
    }
    #[cfg(target_os = "windows")]
    {
        let local = base.data_local_dir();
        vec![
            (local.join("Google/Chrome/User Data"), "chrome", "Chrome"),
            (local.join("Chromium/User Data"), "chromium", "Chromium"),
            (
                local.join("BraveSoftware/Brave-Browser/User Data"),
                "brave",
                "Brave",
            ),
            (local.join("Microsoft/Edge/User Data"), "edge", "Edge"),
            (local.join("Vivaldi/User Data"), "vivaldi", "Vivaldi"),
        ]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let config = base.config_dir();
        vec![
            (config.join("google-chrome"), "chrome", "Chrome"),
            (config.join("chromium"), "chromium", "Chromium"),
            (
                config.join("BraveSoftware/Brave-Browser"),
                "brave",
                "Brave",
            ),
            (config.join("microsoft-edge"), "edge", "Edge"),
            (config.join("vivaldi"), "vivaldi", "Vivaldi"),
            (config.join("opera"), "opera", "Opera"),
        ]
    }
}

fn safari(base: &BaseDirs) -> Vec<CookieBrowser> {
    #[cfg(target_os = "macos")]
    {
        let home = base.home_dir();
        let present = [
            home.join(
                "Library/Containers/com.apple.Safari/Data/Library/Cookies/Cookies.binarycookies",
            ),
            home.join("Library/Cookies/Cookies.binarycookies"),
        ]
        .iter()
        .any(|p| p.exists());
        if present {
            return vec![CookieBrowser {
                label: "Safari".to_string(),
                spec: "safari".to_string(),
            }];
        }
    }
    let _ = base;
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detected_browsers_have_valid_unique_specs() {
        // Filesystem-dependent, so assert the contract: every spec names a
        // known engine (optionally with a profile/path suffix), and the specs
        // are unique.
        let engines = [
            "firefox", "chrome", "chromium", "brave", "edge", "vivaldi", "opera", "safari",
        ];
        let found = installed_cookie_browsers();
        for b in &found {
            assert!(!b.label.is_empty(), "browser must have a label");
            let engine = b.spec.split([':']).next().unwrap_or("");
            assert!(engines.contains(&engine), "unknown engine in spec: {}", b.spec);
        }
        let mut specs: Vec<&str> = found.iter().map(|b| b.spec.as_str()).collect();
        let n = specs.len();
        specs.sort_unstable();
        specs.dedup();
        assert_eq!(specs.len(), n, "detected browser specs must be unique");
    }

    #[test]
    fn firefox_fork_folder_becomes_a_profile_path_spec() {
        // A fake fork with a cookies.sqlite is detected under its real folder
        // name and mapped to a firefox:<path> spec; an empty sibling is not.
        let tmp = std::env::temp_dir().join(format!("whspr-ff-{}", std::process::id()));
        let prof = tmp.join("zen/Profiles/abc.default");
        let _ = std::fs::create_dir_all(&prof);
        let _ = std::fs::write(prof.join("cookies.sqlite"), b"");
        let _ = std::fs::create_dir_all(tmp.join("empty/Profiles/p")); // no cookies.sqlite

        let mut out = Vec::new();
        firefox_from_root(&tmp, &mut out);
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(out.len(), 1, "only the fork with a cookie DB is detected");
        assert_eq!(out[0].label, "zen");
        assert!(
            out[0].spec.starts_with("firefox:") && out[0].spec.ends_with("abc.default"),
            "spec was {}",
            out[0].spec
        );
    }
}
