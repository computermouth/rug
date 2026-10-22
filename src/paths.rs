//! On-disk locations. Debug builds keep everything under `target/tmp` so
//! development never touches the real profile.
//!
//! Settings, history, bookmarks and passwords live together in the profile
//! directory, which is a git repository so it can be synced between machines.
//! WebKit's data, cookies and favicons are machine-local and stay outside it.

use std::path::PathBuf;

use gtk::glib;

fn dev_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp")
}

fn pick(dev: PathBuf, release: PathBuf) -> PathBuf {
    if cfg!(debug_assertions) { dev } else { release }
}

/// The synced directory.
pub fn profile_dir() -> PathBuf {
    pick(dev_dir().join("profile"), glib::user_data_dir().join("rug").join("profile"))
}

pub fn settings_file() -> PathBuf {
    profile_dir().join("settings.json")
}

pub fn history_file() -> PathBuf {
    profile_dir().join("history.json")
}

pub fn bookmarks_file() -> PathBuf {
    profile_dir().join("bookmarks.json")
}

/// A `pass`-compatible password store.
pub fn passwords_dir() -> PathBuf {
    profile_dir().join("passwords")
}

/// Favicons for history entries; too bulky and churny to sync.
pub fn favicons_file() -> PathBuf {
    pick(dev_dir(), glib::user_data_dir().join("rug")).join("favicons.json")
}

/// WebKit's persistent website data (local storage, cookies, ...).
pub fn webkit_data_dir() -> PathBuf {
    pick(dev_dir().join("wk_data"), glib::user_data_dir().join("rug"))
}

pub fn webkit_cache_dir() -> PathBuf {
    pick(dev_dir().join("wk_cache"), glib::user_cache_dir().join("rug"))
}

pub fn cookies_file() -> PathBuf {
    webkit_data_dir().join("cookies.sqlite")
}

/// Moves files from where older versions kept them into the profile.
pub fn migrate() {
    let old_data = pick(dev_dir(), glib::user_data_dir().join("rug"));
    let old_config = pick(dev_dir(), glib::user_config_dir().join("rug"));
    for (old, new) in [
        (old_config.join("settings.json"), settings_file()),
        (old_data.join("data.json"), history_file()),
        (old_data.join("bookmarks.json"), bookmarks_file()),
    ] {
        if old.exists() && !new.exists() {
            let _ = std::fs::create_dir_all(profile_dir());
            let _ = std::fs::rename(&old, &new);
        }
    }
}
