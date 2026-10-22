//! On-disk locations. Debug builds keep everything under `target/tmp` so
//! development never touches the real profile.

use std::path::PathBuf;

use gtk::glib;

fn dev_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp")
}

fn pick(dev: PathBuf, release: PathBuf) -> PathBuf {
    if cfg!(debug_assertions) { dev } else { release }
}

pub fn settings_file() -> PathBuf {
    pick(dev_dir(), glib::user_config_dir().join("rug")).join("settings.json")
}

pub fn history_file() -> PathBuf {
    pick(dev_dir(), glib::user_data_dir().join("rug")).join("data.json")
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
