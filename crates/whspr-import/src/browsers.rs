//! Detecting installed browsers by the **structural signature** of their cookie
//! stores, rather than a hardcoded list of vendor names or paths.
//!
//! We walk each platform's app-data root(s) one and two levels deep — enough to
//! reach both `<browser>` and `<vendor>/<browser>` layouts — and classify every
//! directory by what it contains:
//!
//! - **Firefox family** — a profile with `cookies.sqlite` (Firefox, Zen,
//!   IceCat, LibreWolf, camoufox, …). Fully generic; borrowed via
//!   `firefox:<profile-path>` (verified equivalent to plain `firefox`), which
//!   works for every Firefox-format fork yt-dlp has no dedicated id for.
//! - **Chromium family** — a profile with `Cookies` (or `Network/Cookies`)
//!   *and* the browser markers `History` + `Login Data`. That pair is the key
//!   discriminator: real browsers have them; the dozens of Electron apps that
//!   embed Chromium and also ship a `Cookies` DB (Claude, Postman, Element, …)
//!   do not. yt-dlp can only *decrypt* Chromium cookies for browsers it has an
//!   OS-keychain mapping for, so the engine id is inferred from the path
//!   (chrome/chromium/brave/edge/vivaldi/opera); an unrecognized Chromium
//!   browser is skipped rather than offered with cookies yt-dlp can't read.
//! - **Safari** — its `.binarycookies` file.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use directories::BaseDirs;

/// An installed browser whose logged-in cookies `yt-dlp` can borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CookieBrowser {
    /// Real display name — a Firefox browser/fork's folder name, or the
    /// Chromium browser's folder — with a profile suffix when there's >1.
    pub label: String,
    /// The `yt-dlp --cookies-from-browser` argument (`firefox:/path`,
    /// `chrome:/path`, `safari`).
    pub spec: String,
}

/// Every installed browser with a real, borrowable cookie store, discovered by
/// signature. Filesystem probes only, no subprocess.
pub fn installed_cookie_browsers() -> Vec<CookieBrowser> {
    let Some(base) = BaseDirs::new() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for root in app_data_roots(&base) {
        for child in read_children(&root) {
            classify(&child, &mut out);
            for grandchild in read_children(&child) {
                classify(&grandchild, &mut out);
            }
        }
    }
    out.extend(safari(&base));
    dedup_by_spec(out)
}

/// The roots under which browsers keep their profiles on this platform.
fn app_data_roots(base: &BaseDirs) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        vec![base.home_dir().join("Library/Application Support")]
    }
    #[cfg(target_os = "windows")]
    {
        vec![
            base.data_local_dir().to_path_buf(),
            base.data_dir().to_path_buf(),
        ]
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        vec![
            base.config_dir().to_path_buf(),
            base.home_dir().join(".mozilla"),
        ]
    }
}

/// Classifies one directory as a browser (or not) and appends any profiles it
/// exposes as [`CookieBrowser`] entries.
fn classify(dir: &Path, out: &mut Vec<CookieBrowser>) {
    let ff = firefox_profiles(dir);
    let single_ff = ff.len() == 1;
    for prof in &ff {
        out.push(CookieBrowser {
            label: labeled(dir_name(dir), prof, single_ff),
            spec: format!("firefox:{}", prof.display()),
        });
    }

    if let Some(engine) = chromium_engine(dir) {
        let profs = chromium_profiles(dir);
        let single = profs.len() == 1;
        for prof in &profs {
            out.push(CookieBrowser {
                label: labeled(chromium_label(dir), prof, single),
                spec: format!("{engine}:{}", prof.display()),
            });
        }
    }
}

/// `name`, plus the profile folder in parentheses when the browser exposes more
/// than one profile (so two logins are distinguishable).
fn labeled(name: String, profile: &Path, single: bool) -> String {
    if single {
        name
    } else {
        format!("{name} ({})", dir_name(profile))
    }
}

/// Firefox-format profiles under `dir`: any `Profiles/<p>/cookies.sqlite`
/// (macOS/Windows) or `<p>/cookies.sqlite` (Linux's flat layout).
fn firefox_profiles(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = read_children(&dir.join("Profiles"))
        .into_iter()
        .filter(|p| p.join("cookies.sqlite").exists())
        .collect();
    v.extend(
        read_children(dir)
            .into_iter()
            .filter(|p| p.join("cookies.sqlite").exists()),
    );
    v
}

/// The yt-dlp Chromium engine id whose OS-keychain entry decrypts cookies under
/// `dir`, inferred from the path. `None` => not a browser yt-dlp can decrypt
/// (so it's skipped instead of offered as broken).
fn chromium_engine(dir: &Path) -> Option<&'static str> {
    let s = dir.to_string_lossy().to_ascii_lowercase();
    // Order matters: match the specific vendors before the generic "chrome"
    // (which is also a substring of "chrome for testing", handled by scanning).
    for (keyword, id) in [
        ("brave", "brave"),
        ("edge", "edge"),
        ("vivaldi", "vivaldi"),
        ("opera", "opera"),
        ("chromium", "chromium"),
        ("chrome", "chrome"),
    ] {
        if s.contains(keyword) {
            return Some(id);
        }
    }
    None
}

/// Chromium *browser* profiles under a user-data `dir`: a `Cookies` DB plus the
/// `History` + `Login Data` markers that separate a browser from an Electron
/// app. Guest/System profiles are skipped — never a user's session.
fn chromium_profiles(dir: &Path) -> Vec<PathBuf> {
    read_children(dir)
        .into_iter()
        .filter(|p| {
            let n = dir_name(p);
            n != "System Profile"
                && n != "Guest Profile"
                && (p.join("Cookies").exists() || p.join("Network").join("Cookies").exists())
                && p.join("History").exists()
                && p.join("Login Data").exists()
        })
        .collect()
}

/// A Chromium browser's display name from its user-data dir (`Brave-Browser` ->
/// `Brave`; `Chrome`, `Chrome for Testing`, `Vivaldi` pass through).
fn chromium_label(dir: &Path) -> String {
    dir_name(dir).replace("-Browser", "").trim().to_string()
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

/// First occurrence wins per `spec`, preserving discovery order (so a browser
/// reached both as `<root>/X` and `<root>/X/Profiles` isn't listed twice).
fn dedup_by_spec(list: Vec<CookieBrowser>) -> Vec<CookieBrowser> {
    let mut seen = HashSet::new();
    list.into_iter()
        .filter(|b| seen.insert(b.spec.clone()))
        .collect()
}

/// The immediate children of `dir`, sorted; empty if unreadable. Owned paths,
/// so no directory handle outlives the call.
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
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: PathBuf) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"").unwrap();
    }

    #[test]
    fn browsers_are_classified_but_electron_apps_are_not() {
        let tmp = std::env::temp_dir().join(format!("whspr-cls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);

        // A Chrome-format browser: profile has Cookies + History + Login Data,
        // and its path carries a decryptable engine keyword.
        let chrome = tmp.join("Chrome/Default");
        touch(chrome.join("Cookies"));
        touch(chrome.join("History"));
        touch(chrome.join("Login Data"));
        // An Electron app: a Cookies DB but no browser markers.
        touch(tmp.join("SomeChatApp/Default/Cookies"));
        // A Firefox fork.
        touch(tmp.join("zen/Profiles/abc.default/cookies.sqlite"));

        let mut out = Vec::new();
        classify(&tmp.join("Chrome"), &mut out);
        classify(&tmp.join("SomeChatApp"), &mut out);
        classify(&tmp.join("zen"), &mut out);
        let _ = std::fs::remove_dir_all(&tmp);

        let specs: Vec<&str> = out.iter().map(|b| b.spec.as_str()).collect();
        assert!(
            specs
                .iter()
                .any(|s| s.starts_with("chrome:") && s.contains("Chrome/Default")),
            "Chrome should be detected: {specs:?}"
        );
        assert!(
            specs
                .iter()
                .any(|s| s.starts_with("firefox:") && s.contains("zen")),
            "the zen fork should be detected: {specs:?}"
        );
        assert!(
            !specs.iter().any(|s| s.contains("SomeChatApp")),
            "an Electron app (no History/Login Data) must be excluded: {specs:?}"
        );
    }

    #[test]
    fn engine_is_inferred_before_the_generic_chrome_keyword() {
        assert_eq!(
            chromium_engine(Path::new("/x/BraveSoftware/Brave-Browser")),
            Some("brave")
        );
        assert_eq!(
            chromium_engine(Path::new("/x/Google/Chrome for Testing")),
            Some("chrome")
        );
        assert_eq!(
            chromium_engine(Path::new("/x/Microsoft Edge")),
            Some("edge")
        );
        assert_eq!(chromium_engine(Path::new("/x/SomeChatApp")), None);
    }

    #[test]
    fn detected_specs_are_valid_and_unique() {
        let engines = [
            "firefox", "chrome", "chromium", "brave", "edge", "vivaldi", "opera", "safari",
        ];
        let found = installed_cookie_browsers();
        for b in &found {
            assert!(!b.label.is_empty());
            let engine = b.spec.split(':').next().unwrap_or("");
            assert!(engines.contains(&engine), "unknown engine: {}", b.spec);
        }
        let mut specs: Vec<&str> = found.iter().map(|b| b.spec.as_str()).collect();
        let n = specs.len();
        specs.sort_unstable();
        specs.dedup();
        assert_eq!(specs.len(), n, "specs must be unique");
    }
}
