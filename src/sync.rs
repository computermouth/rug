//! The profile directory as a git repository: changes are committed
//! automatically, and synced with its `origin` remote at startup, hourly,
//! on exit, and soon after a login or bookmark changes.
//!
//! Merges go through rug itself (registered as a git merge driver), which
//! merges settings, history and bookmarks entry by entry, so syncing never
//! stops at a conflict. That includes a plain `git pull` in a terminal.

use std::cell::{Cell, RefCell};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;

use gtk::{gio, glib};

use crate::{bookmarks, paths, store};

/// Seconds to wait after a change before committing, so a burst of page
/// loads makes one commit.
const COMMIT_DELAY: u32 = 30;

/// Seconds to wait after a login or bookmark change before syncing, so a
/// few edits in a row sync once.
const SYNC_DELAY: u32 = 5;

/// Seconds between background syncs.
const SYNC_INTERVAL: u32 = 60 * 60;

/// Held for every git operation; auto-commits run on worker threads.
static GIT: Mutex<()> = Mutex::new(());

thread_local! {
    static COMMIT_TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    static STATUS: RefCell<String> = const { RefCell::new(String::new()) };
    static SYNC_TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    static SYNCING: Cell<bool> = const { Cell::new(false) };
    /// Another sync was asked for while one was running.
    static SYNC_AGAIN: Cell<bool> = const { Cell::new(false) };
}

fn git(args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(paths::profile_dir())
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        // Nobody can answer ssh's questions either, and syncing on exit
        // shouldn't hang on an unreachable host.
        .env("GIT_SSH_COMMAND", std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh -o BatchMode=yes -o ConnectTimeout=10".into()))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("Couldn't run git: {e}"))?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let message = stderr.lines().rfind(|l| !l.trim().is_empty()).unwrap_or("git failed");
        Err(message.trim().to_string())
    }
}

/// Makes the profile a repository if it isn't one, and points git at rug
/// for merging. Called at startup.
pub fn init() {
    let _lock = GIT.lock();
    let dir = paths::profile_dir();
    let _ = std::fs::create_dir_all(&dir);
    if !dir.join(".git").exists() && git(&["init", "-q", "-b", "main"]).is_err() {
        return;
    }
    let attributes = dir.join(".gitattributes");
    if !attributes.exists() {
        let _ = std::fs::write(attributes, "* merge=rug\n");
    }
    if let Ok(exe) = std::env::current_exe() {
        let driver = format!("'{}' --merge-driver %O %A %B %P", exe.display());
        let _ = git(&["config", "merge.rug.name", "rug"]);
        let _ = git(&["config", "merge.rug.driver", &driver]);
    }
    // Commits need an identity; don't make the user set one up first.
    if git(&["config", "user.email"]).is_err() {
        let _ = git(&["config", "user.name", "rug"]);
        let _ = git(&["config", "user.email", &format!("rug@{}", glib::host_name())]);
    }
    let _ = commit_locked();
}

/// Notes that something in the profile changed; it's committed shortly.
pub fn changed() {
    COMMIT_TIMER.with(|timer| {
        if let Some(pending) = timer.borrow_mut().take() {
            pending.remove();
        }
        let id = glib::timeout_add_seconds_local_once(COMMIT_DELAY, || {
            COMMIT_TIMER.with(|timer| timer.borrow_mut().take());
            gio::spawn_blocking(commit);
        });
        *timer.borrow_mut() = Some(id);
    });
}

/// Commits anything outstanding.
pub fn commit() {
    let _lock = GIT.lock();
    let _ = commit_locked();
}

/// Syncs now and every hour after. Called once the main loop is set up.
pub fn start() {
    glib::spawn_future_local(sync_now(true));
    glib::timeout_add_seconds_local(SYNC_INTERVAL, || {
        glib::spawn_future_local(sync_now(true));
        glib::ControlFlow::Continue
    });
}

/// Syncs shortly, for changes worth having on other machines right away.
pub fn sync_soon() {
    SYNC_TIMER.with(|timer| {
        if let Some(pending) = timer.borrow_mut().take() {
            pending.remove();
        }
        let id = glib::timeout_add_seconds_local_once(SYNC_DELAY, || {
            SYNC_TIMER.with(|timer| timer.borrow_mut().take());
            glib::spawn_future_local(sync_now(true));
        });
        *timer.borrow_mut() = Some(id);
    });
}

/// The last sync, on exit. Blocks, but git gives up on an unreachable remote.
pub fn sync_on_exit() {
    if let Err(e) = sync() {
        // Still commit, so nothing is lost before the next sync.
        commit();
        eprintln!("rug: sync failed: {e}");
    }
}

fn commit_locked() -> Result<(), String> {
    git(&["add", "-A"])?;
    if git(&["diff", "--cached", "--quiet"]).is_ok() {
        return Ok(());
    }
    git(&["commit", "-q", "-m", &format!("rug on {}", glib::host_name())]).map(drop)
}

pub fn remote() -> String {
    git(&["remote", "get-url", "origin"]).unwrap_or_default()
}

pub fn set_remote(url: &str) -> Result<(), String> {
    let _lock = GIT.lock();
    let url = url.trim();
    match (remote().is_empty(), url.is_empty()) {
        (true, true) => Ok(()),
        (true, false) => git(&["remote", "add", "origin", url]).map(drop),
        (false, true) => git(&["remote", "remove", "origin"]).map(drop),
        (false, false) => git(&["remote", "set-url", "origin", url]).map(drop),
    }
}

/// The outcome of the last sync, for the settings page.
pub fn status() -> String {
    STATUS.with(|s| s.borrow().clone())
}

/// Syncs on a worker thread, then reloads whatever the sync changed.
/// Automatic (`auto`) syncs stay quiet when no remote is set.
pub async fn sync_now(auto: bool) {
    // Sync Now waits out a background sync, then reports its own.
    while !auto && SYNCING.get() {
        glib::timeout_future(std::time::Duration::from_millis(200)).await;
    }
    if SYNCING.replace(true) {
        SYNC_AGAIN.set(true);
        return;
    }
    loop {
        sync_once(auto).await;
        if !SYNC_AGAIN.replace(false) {
            break;
        }
    }
    SYNCING.set(false);
}

async fn sync_once(auto: bool) {
    let result = gio::spawn_blocking(sync).await.unwrap_or_else(|_| Err("Sync crashed".into()));
    let status = match result {
        Err(e) if auto && e == NO_REMOTE => return,
        Ok((message, pulled)) => {
            if pulled {
                store::reload();
                bookmarks::init();
                crate::apply_theme(store::is_private() || store::settings().dark);
            }
            message.to_string()
        }
        Err(e) => format!("Sync failed: {e}"),
    };
    let time = glib::DateTime::now_local().and_then(|t| t.format("%H:%M")).unwrap_or_default();
    STATUS.with(|s| *s.borrow_mut() = format!("{status} ({time})"));
}

const NO_REMOTE: &str = "no remote set";

/// Commits, merges in `origin`, and pushes. Returns a summary and whether
/// anything came in.
fn sync() -> Result<(&'static str, bool), String> {
    let _lock = GIT.lock();
    commit_locked()?;
    if remote().is_empty() {
        return Err(NO_REMOTE.into());
    }
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])?;
    git(&["fetch", "-q", "origin"])?;
    let upstream = format!("refs/remotes/origin/{branch}");
    if git(&["rev-parse", "--verify", "-q", &upstream]).is_err() {
        git(&["push", "-q", "-u", "origin", &branch])?;
        return Ok(("Pushed", false));
    }

    let before = git(&["rev-parse", "HEAD"])?;
    let message = format!("rug on {}: merge", glib::host_name());
    if git(&["merge", "-q", "--no-edit", "--allow-unrelated-histories", "-m", &message, &upstream]).is_err() {
        resolve_leftovers()?;
    }
    let pulled = git(&["rev-parse", "HEAD"])? != before;

    let ahead = git(&["rev-list", "--count", &format!("{upstream}..HEAD")])? != "0";
    if ahead {
        git(&["push", "-q", "origin", &branch])?;
    }
    Ok(match (pulled, ahead) {
        (false, false) => ("Already up to date", false),
        (true, false) => ("Pulled changes", true),
        (false, true) => ("Pushed changes", false),
        (true, true) => ("Pulled and pushed changes", true),
    })
}

/// The merge driver handles edits; what's left is one side deleting what
/// the other changed. Keeps whichever is in the working tree.
fn resolve_leftovers() -> Result<(), String> {
    let unmerged = git(&["diff", "--name-only", "--diff-filter=U"]).unwrap_or_default();
    for path in unmerged.lines() {
        let action = if paths::profile_dir().join(path).exists() { "add" } else { "rm" };
        let _ = git(&[action, "-q", "--", path]);
    }
    git(&["commit", "-q", "--no-edit"]).map(drop).inspect_err(|_| {
        let _ = git(&["merge", "--abort"]);
    })
}

/// `rug --merge-driver BASE OURS THEIRS PATH`, run by git: merges into OURS.
/// Files rug doesn't understand (passwords) keep our side.
pub fn merge_driver(args: &[String]) -> i32 {
    let [base, ours, theirs, path] = args else { return 1 };
    let read = |p: &str| std::fs::read_to_string(p).unwrap_or_default();
    let merged = match Path::new(path).file_name().and_then(|n| n.to_str()) {
        Some("settings.json") => merge_objects(&read(base), &read(ours), &read(theirs)),
        Some("history.json") => store::merge_history(&read(base), &read(ours), &read(theirs)),
        Some("bookmarks.json") => bookmarks::merge(&read(base), &read(ours), &read(theirs)),
        _ => return 0,
    };
    match std::fs::write(ours, merged) {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

/// Field-by-field: each field takes whichever side changed it, ours if both did.
fn merge_objects(base: &str, ours: &str, theirs: &str) -> String {
    use serde_json::{Map, Value};
    let parse = |json: &str| match serde_json::from_str(json) {
        Ok(Value::Object(map)) => map,
        _ => Map::new(),
    };
    let (base, mut merged, theirs) = (parse(base), parse(ours), parse(theirs));
    for (key, value) in theirs {
        if merged.get(&key) == base.get(&key) {
            merged.insert(key, value);
        }
    }
    serde_json::to_string_pretty(&merged).unwrap_or_default()
}
