//! Bookmarks: links and nested folders, persisted as JSON. Stored flat, each
//! entry pointing at its parent folder (`None` is the top level).

use std::cell::RefCell;

use serde::{Deserialize, Serialize};

use crate::{paths, store, urls};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bookmark {
    pub id: u64,
    pub parent: Option<u64>,
    pub title: String,
    /// `None` for folders.
    pub url: Option<String>,
}

impl Bookmark {
    pub fn is_folder(&self) -> bool {
        self.url.is_none()
    }
}

/// A folder and the ids from the top level down to it, itself included.
pub struct Folder {
    pub id: u64,
    pub title: String,
    pub path: Vec<u64>,
}

#[derive(Default, Serialize, Deserialize)]
struct Bookmarks {
    next_id: u64,
    items: Vec<Bookmark>,
}

thread_local! {
    static BOOKMARKS: RefCell<Bookmarks> = RefCell::default();
}

fn with<R>(f: impl FnOnce(&mut Bookmarks) -> R) -> R {
    BOOKMARKS.with(|b| f(&mut b.borrow_mut()))
}

pub fn init() {
    let loaded = store::read_json(&paths::bookmarks_file()).unwrap_or_default();
    with(|b| *b = loaded);
}

/// Contents of a folder, in order.
pub fn children(parent: Option<u64>) -> Vec<Bookmark> {
    with(|b| b.items.iter().filter(|i| i.parent == parent).cloned().collect())
}

/// Every folder, depth first.
pub fn folders() -> Vec<Folder> {
    fn walk(items: &[Bookmark], parent: Option<u64>, path: &[u64], out: &mut Vec<Folder>) {
        for f in items.iter().filter(|i| i.parent == parent && i.is_folder()) {
            let path = [path, &[f.id]].concat();
            out.push(Folder { id: f.id, title: f.title.clone(), path: path.clone() });
            walk(items, Some(f.id), &path, out);
        }
    }
    let mut out = Vec::new();
    with(|b| walk(&b.items, None, &[], &mut out));
    out
}

/// Adds a link, or a folder when `url` is `None`.
pub fn add(parent: Option<u64>, title: &str, url: Option<&str>) {
    with(|b| {
        let id = b.next_id;
        b.next_id += 1;
        let url = url.map(normalize_url);
        let title = default_title(title, url.as_deref());
        let parent = b.valid_parent(parent, None);
        b.items.push(Bookmark { id, parent, title, url });
        b.save();
    });
}

/// Renames, re-targets or moves an entry. `url` is ignored for folders.
pub fn edit(id: u64, title: &str, url: Option<&str>, parent: Option<u64>) {
    with(|b| {
        let new_parent = b.valid_parent(parent, Some(id));
        let Some(item) = b.items.iter_mut().find(|i| i.id == id) else { return };
        // An impossible move (e.g. into its own subfolder) leaves it where it is.
        let parent = if parent.is_some() && new_parent.is_none() { item.parent } else { new_parent };
        if let (Some(old), Some(new)) = (&mut item.url, url) {
            *old = normalize_url(new);
        }
        item.title = default_title(title, item.url.as_deref());
        if item.parent != parent {
            // Land at the end of the new folder.
            item.parent = parent;
            let pos = b.items.iter().position(|i| i.id == id).expect("just found");
            let item = b.items.remove(pos);
            b.items.push(item);
        }
        b.save();
    });
}

/// Moves an entry into `parent`, just before its sibling `before`, or to
/// the end of that folder if `before` is `None` or not in it.
pub fn move_to(id: u64, parent: Option<u64>, before: Option<u64>) {
    with(|b| {
        if parent.is_some() && b.valid_parent(parent, Some(id)).is_none() {
            return;
        }
        let Some(pos) = b.items.iter().position(|i| i.id == id) else { return };
        let mut item = b.items.remove(pos);
        item.parent = parent;
        let at = before
            .and_then(|x| b.items.iter().position(|i| i.id == x && i.parent == parent))
            .unwrap_or(b.items.len());
        b.items.insert(at, item);
        b.save();
    });
}

/// Removes an entry, and everything inside it if it's a folder.
pub fn delete(id: u64) {
    with(|b| {
        let doomed: Vec<u64> = b.items.iter().map(|i| i.id).filter(|&i| b.is_within(Some(i), id)).collect();
        b.items.retain(|i| !doomed.contains(&i.id));
        b.save();
    });
}

impl Bookmarks {
    fn save(&self) {
        store::write_json(&paths::bookmarks_file(), self);
    }

    /// Whether `folder` is `ancestor` or somewhere inside it.
    fn is_within(&self, mut folder: Option<u64>, ancestor: u64) -> bool {
        // Bounded, in case a hand-edited file has a cycle.
        for _ in 0..=self.items.len() {
            match folder {
                Some(id) if id == ancestor => return true,
                Some(id) => folder = self.items.iter().find(|i| i.id == id).and_then(|i| i.parent),
                None => return false,
            }
        }
        false
    }

    /// `parent` if it's an existing folder that `moving` may go into
    /// (not itself or its own subfolder), otherwise the top level.
    fn valid_parent(&self, parent: Option<u64>, moving: Option<u64>) -> Option<u64> {
        let id = parent?;
        let is_folder = self.items.iter().any(|i| i.id == id && i.is_folder());
        let is_cycle = moving.is_some_and(|m| self.is_within(Some(id), m));
        (is_folder && !is_cycle).then_some(id)
    }
}

fn normalize_url(url: &str) -> String {
    urls::resolve(url, &store::settings())
}

fn default_title(title: &str, url: Option<&str>) -> String {
    match (title.trim(), url) {
        ("", Some(url)) => url.to_string(),
        ("", None) => "New Folder".to_string(),
        (title, _) => title.to_string(),
    }
}
