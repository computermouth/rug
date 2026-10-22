//! Settings and browsing history, shared by every window on the main thread
//! and persisted as JSON.

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use gtk::prelude::*;
use gtk::{gdk, glib};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::{paths, urls};

const MAX_HISTORY: usize = 1000;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    #[default]
    Ddg,
    Google,
    Bing,
    Custom,
}

impl Engine {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "ddg" => Some(Self::Ddg),
            "google" => Some(Self::Google),
            "bing" => Some(Self::Bing),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub engine: Engine,
    pub custom_url: String,
    pub dark: bool,
    pub cache: bool,
    /// Hosts never to offer saving passwords for.
    pub never_save: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self { engine: Engine::Ddg, custom_url: String::new(), dark: false, cache: true, never_save: Vec::new() }
    }
}

impl Settings {
    pub fn search_url(&self, query: &str) -> String {
        let q = urls::encode(query);
        match self.engine {
            Engine::Ddg => format!("https://duckduckgo.com/?q={q}"),
            Engine::Google => format!("https://www.google.com/search?q={q}"),
            Engine::Bing => format!("https://www.bing.com/search?q={q}"),
            Engine::Custom => format!("{}{q}", self.custom_url),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(from = "StoredVisit", into = "StoredVisit")]
pub struct Visit {
    pub url: String,
    pub title: String,
    /// Milliseconds since the epoch; 0 for visits from before times were kept.
    time: u64,
    /// PNG data URI from older files, which kept favicons in the history.
    legacy_favicon: Option<String>,
}

/// On-disk shape of a [`Visit`]: `[url, title, time]`, or from older
/// versions `[url, title, favicon]` and `[url, title]`.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum StoredVisit {
    Current(String, String, u64),
    WithFavicon(String, String, Option<String>),
    Legacy(String, String),
}

impl From<StoredVisit> for Visit {
    fn from(v: StoredVisit) -> Self {
        let (url, title, time, legacy_favicon) = match v {
            StoredVisit::Current(u, t, time) => (u, t, time, None),
            StoredVisit::WithFavicon(u, t, f) => (u, t, 0, f),
            StoredVisit::Legacy(u, t) => (u, t, 0, None),
        };
        Self { url, title, time, legacy_favicon }
    }
}

impl From<Visit> for StoredVisit {
    fn from(v: Visit) -> Self {
        Self::Current(v.url, v.title, v.time)
    }
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Parses a history file, giving undated visits from older versions
/// distinct times in their original (most recent first) order.
fn parse_history(json: &str) -> Vec<Visit> {
    let mut history: Vec<Visit> = serde_json::from_str(json).unwrap_or_default();
    let len = history.len() as u64;
    for (i, v) in history.iter_mut().enumerate() {
        if v.time == 0 {
            v.time = len - i as u64;
        }
    }
    history
}

/// Three-way merge of history files for git sync: visits from both sides,
/// minus those one side deleted (unless the other has visited them since).
pub fn merge_history(base: &str, ours: &str, theirs: &str) -> String {
    let (base, ours, theirs) = (parse_history(base), parse_history(ours), parse_history(theirs));
    let find = |list: &[Visit], url: &str| list.iter().find(|v| v.url == url).cloned();
    let mut merged: Vec<Visit> = Vec::new();
    for url in ours.iter().chain(&theirs).map(|v| v.url.clone()) {
        if merged.iter().any(|v| v.url == url) {
            continue;
        }
        let kept = match (find(&base, &url), find(&ours, &url), find(&theirs, &url)) {
            (_, Some(o), Some(t)) => Some(if t.time > o.time { t } else { o }),
            (Some(b), Some(v), None) | (Some(b), None, Some(v)) => (v.time > b.time).then_some(v),
            (None, Some(v), None) | (None, None, Some(v)) => Some(v),
            (_, None, None) => None,
        };
        merged.extend(kept);
    }
    merged.sort_by_key(|v| std::cmp::Reverse(v.time));
    merged.truncate(MAX_HISTORY);
    serde_json::to_string(&merged).unwrap_or_default()
}

#[derive(Default)]
struct Store {
    private: bool,
    settings: Settings,
    /// Most recent first.
    history: Vec<Visit>,
    /// Favicons by page URL, with the data URI they're saved as.
    favicons: HashMap<String, (gdk::Texture, String)>,
}

thread_local! {
    static STORE: RefCell<Store> = RefCell::default();
    static PLACEHOLDER_FAVICON: gdk::Texture = gray_circle();
}

fn with<R>(f: impl FnOnce(&mut Store) -> R) -> R {
    STORE.with(|s| f(&mut s.borrow_mut()))
}

/// Loads settings and (outside private mode) history from disk.
pub fn init(private: bool) {
    let settings = read_json(&paths::settings_file()).unwrap_or_default();
    let history =
        if private { Vec::new() } else { parse_history(&std::fs::read_to_string(paths::history_file()).unwrap_or_default()) };
    let stored: HashMap<String, String> = read_json(&paths::favicons_file()).unwrap_or_default();
    let legacy = history.iter().filter_map(|v| Some((&v.url, v.legacy_favicon.as_ref()?)));
    let favicons = stored
        .iter()
        .chain(legacy)
        .filter_map(|(url, data)| Some((url.clone(), (texture_from_data_uri(data)?, data.clone()))))
        .collect();
    with(|s| *s = Store { private, settings, history, favicons });
}

/// Re-reads everything from disk, e.g. after a sync brought in changes.
pub fn reload() {
    init(is_private());
}

pub fn is_private() -> bool {
    with(|s| s.private)
}

pub fn settings() -> Settings {
    with(|s| s.settings.clone())
}

pub fn update_settings(f: impl FnOnce(&mut Settings)) {
    with(|s| {
        f(&mut s.settings);
        write_json(&paths::settings_file(), &s.settings);
    });
}

fn is_tracked(url: &str) -> bool {
    !url.is_empty() && !url.starts_with("about:") && !url.starts_with("rug:")
}

/// Moves `url` to the top of the history, keeping its old title if `title` is empty.
pub fn record_visit(url: &str, title: &str) {
    with(|s| {
        if s.private || !is_tracked(url) {
            return;
        }
        let visit = match s.history.iter().position(|v| v.url == url) {
            Some(i) => {
                let mut v = s.history.remove(i);
                if !title.is_empty() {
                    v.title = title.to_string();
                }
                v
            }
            None => Visit {
                url: url.to_string(),
                title: if title.is_empty() { url } else { title }.to_string(),
                time: 0,
                legacy_favicon: None,
            },
        };
        let visit = Visit { time: now(), ..visit };
        s.history.insert(0, visit);
        s.history.truncate(MAX_HISTORY);
        s.save_history();
    });
}

pub fn record_favicon(url: &str, texture: &gdk::Texture) {
    with(|s| {
        if s.private || !is_tracked(url) {
            return;
        }
        s.favicons.insert(url.to_string(), (texture.clone(), texture_to_data_uri(texture)));
        s.save_favicons();
    });
}

pub fn clear_history() {
    with(|s| {
        s.history.clear();
        s.favicons.clear();
        s.save_history();
        s.save_favicons();
    });
}

/// Removes every visit to exactly `url`.
pub fn forget_url(url: &str) {
    forget(|v| v.url == url);
}

/// Removes every visit to any page on `host`.
pub fn forget_host(host: &str) {
    forget(|v| urls::origin(&v.url).is_some_and(|(_, h)| h == host));
}

fn forget(matches: impl Fn(&Visit) -> bool) {
    with(|s| {
        let (gone, kept): (Vec<Visit>, Vec<Visit>) = s.history.drain(..).partition(|v| matches(v));
        s.history = kept;
        for v in gone {
            s.favicons.remove(&v.url);
        }
        s.save_history();
        s.save_favicons();
    });
}

pub fn recent(max: usize) -> Vec<Visit> {
    with(|s| s.history.iter().take(max).cloned().collect())
}

/// History entries whose URL or title contains `query`, case-insensitively.
pub fn search(query: &str, max: usize) -> Vec<Visit> {
    let q = query.to_lowercase();
    with(|s| {
        s.history
            .iter()
            .filter(|v| v.url.to_lowercase().contains(&q) || v.title.to_lowercase().contains(&q))
            .take(max)
            .cloned()
            .collect()
    })
}

/// The most visited hosts as `(origin, host)`; ties go to the most recent.
pub fn top_sites(max: usize) -> Vec<(String, String)> {
    let mut sites: Vec<(String, String, usize)> = Vec::new();
    with(|s| {
        for v in &s.history {
            let Some((origin, host)) = urls::origin(&v.url) else { continue };
            match sites.iter_mut().find(|(_, h, _)| h == host) {
                Some(site) => site.2 += 1,
                None => sites.push((origin.to_string(), host.to_string(), 1)),
            }
        }
    });
    sites.sort_by(|a, b| b.2.cmp(&a.2));
    sites.into_iter().take(max).map(|(origin, host, _)| (origin, host)).collect()
}

/// The stored favicon for `url`, or a gray placeholder.
pub fn favicon(url: &str) -> gdk::Texture {
    with(|s| s.favicons.get(url).map(|(t, _)| t.clone())).unwrap_or_else(placeholder_favicon)
}

pub fn placeholder_favicon() -> gdk::Texture {
    PLACEHOLDER_FAVICON.with(Clone::clone)
}

impl Store {
    fn save_history(&self) {
        write_json(&paths::history_file(), &self.history);
    }

    fn save_favicons(&self) {
        let favicons: HashMap<&String, &String> = self
            .favicons
            .iter()
            .filter(|(url, _)| self.history.iter().any(|v| &v.url == *url))
            .map(|(url, (_, data))| (url, data))
            .collect();
        write_json(&paths::favicons_file(), &favicons);
    }
}

pub(crate) fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

pub(crate) fn write_json<T: Serialize + ?Sized>(path: &Path, value: &T) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(value) {
        let _ = std::fs::write(path, json);
    }
    if path == paths::bookmarks_file() {
        crate::sync::sync_soon();
    } else if path.starts_with(paths::profile_dir()) {
        crate::sync::changed();
    }
}

fn texture_to_data_uri(texture: &gdk::Texture) -> String {
    let png = texture.save_to_png_bytes();
    format!("data:image/png;base64,{}", glib::base64_encode(&png))
}

fn texture_from_data_uri(uri: &str) -> Option<gdk::Texture> {
    let b64 = uri.strip_prefix("data:image/png;base64,")?;
    gdk::Texture::from_bytes(&glib::Bytes::from_owned(glib::base64_decode(b64))).ok()
}

fn gray_circle() -> gdk::Texture {
    const S: usize = 16;
    let mut px = vec![0u8; S * S * 4];
    let c = S as f64 / 2.0;
    let r = c - 1.0;
    for y in 0..S {
        for x in 0..S {
            let (dx, dy) = (x as f64 + 0.5 - c, y as f64 + 0.5 - c);
            if dx * dx + dy * dy <= r * r {
                px[(y * S + x) * 4..][..4].copy_from_slice(&[180, 180, 180, 255]);
            }
        }
    }
    gdk::MemoryTexture::new(S as i32, S as i32, gdk::MemoryFormat::R8g8b8a8, &glib::Bytes::from_owned(px), S * 4)
        .into()
}
