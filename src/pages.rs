//! The internal `rug://` pages. Templates and their assets live in
//! `assets/pages`; `{{name}}` placeholders are filled in here.

use gtk::prelude::*;
use gtk::{gio, glib};
use webkit6::{URISchemeRequest, WebContext};

use crate::passwords::{self, Login};
use crate::store::{self, Engine};
use crate::{bookmarks, paths, sync, urls};

pub const HOME: &str = "rug://home";
pub const PASSWORDS: &str = "rug://passwords";
const BOOKMARKS: &str = "rug://bookmarks";
const SETTINGS: &str = "rug://settings";

const HOME_TEMPLATE: &str = include_str!("../assets/pages/home.html");
const PRIVATE_HOME: &str = include_str!("../assets/pages/private.html");
const SETTINGS_TEMPLATE: &str = include_str!("../assets/pages/settings.html");
const BOOKMARKS_TEMPLATE: &str = include_str!("../assets/pages/bookmarks.html");
const BOOKMARK_DIALOG: &str = include_str!("../assets/pages/bookmark-dialog.html");
const PASSWORDS_TEMPLATE: &str = include_str!("../assets/pages/passwords.html");
const NOT_FOUND: &str = "<!DOCTYPE html><p>Not found</p>";

/// Served at `rug://assets/<name>`.
const ASSETS: &[(&str, &str, &[u8])] = &[
    ("rug.css", "text/css", include_bytes!("../assets/pages/rug.css")),
    ("favicons.js", "text/javascript", include_bytes!("../assets/pages/favicons.js")),
    ("bookmarks.js", "text/javascript", include_bytes!("../assets/pages/bookmarks.js")),
    ("settings.js", "text/javascript", include_bytes!("../assets/pages/settings.js")),
    ("favicon.svg", "image/svg+xml", include_bytes!("../assets/pages/favicon.svg")),
];

pub fn register(context: &WebContext) {
    context.register_uri_scheme("rug", handle);
}

/// Answered asynchronously, since some actions (syncing, re-encrypting
/// passwords) take a while.
fn handle(request: &URISchemeRequest) {
    let request = request.clone();
    glib::spawn_future_local(async move {
        let uri = request.uri().unwrap_or_default();
        let form = match request.http_body() {
            Some(stream) => read_all(stream).await,
            None => String::new(),
        };
        let (mime, body) = route(&uri, &form).await;
        let stream = gio::MemoryInputStream::from_bytes(&body);
        request.finish(&stream, body.len() as i64, Some(mime));
    });
}

/// A POSTed form's body. Forms carrying passwords are POSTed, keeping the
/// password out of the URL (and so out of history).
async fn read_all(stream: gio::InputStream) -> String {
    let mut body = Vec::new();
    while let Ok(chunk) = stream.read_bytes_future(64 * 1024, glib::Priority::DEFAULT).await {
        if chunk.is_empty() {
            break;
        }
        body.extend_from_slice(&chunk);
    }
    String::from_utf8_lossy(&body).into_owned()
}

async fn route(uri: &str, form: &str) -> (&'static str, glib::Bytes) {
    let rest = uri.strip_prefix("rug://").unwrap_or_default();
    let rest = rest.split('#').next().unwrap_or_default();
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));

    match path {
        "home" => html(home(query)),
        "settings" => html(settings(query).await),
        "bookmarks" => html(bookmarks_page(query)),
        "passwords" => html(passwords_page(query, form).await),
        "private" => {
            open_private_window();
            html(redirect(HOME))
        }
        _ => match ASSETS.iter().find(|(name, ..)| path.strip_prefix("assets/") == Some(name)) {
            Some(&(_, mime, bytes)) => (mime, glib::Bytes::from_static(bytes)),
            None => html(NOT_FOUND.into()),
        },
    }
}

fn html(page: String) -> (&'static str, glib::Bytes) {
    ("text/html", glib::Bytes::from_owned(page.into_bytes()))
}

/// Renders the home page, or applies a `forget` (one recent page) or
/// `forget_host` (a frequent site) from the query and redirects back.
fn home(query: &str) -> String {
    if store::is_private() {
        return PRIVATE_HOME.into();
    }

    let params = urls::parse_query(query);
    if let Some(url) = params.get("forget") {
        store::forget_url(url);
        return redirect(HOME);
    }
    if let Some(host) = params.get("forget_host") {
        store::forget_host(host);
        return redirect(HOME);
    }

    let bookmarks: String = bookmarks::children(None)
        .iter()
        .map(|b| match &b.url {
            Some(url) => format!(
                r#"<li><img src="{}"><a href="{}">{}</a></li>"#,
                esc(&favicon_url(url)), esc(url), esc(&b.title),
            ),
            None => format!(
                r#"<li><span class="icon">📁</span><a href="{BOOKMARKS}#f{}">{}</a></li>"#,
                b.id, esc(&b.title),
            ),
        })
        .collect();

    let frequent: String = store::top_sites(16)
        .iter()
        .map(|(origin, host)| {
            format!(
                r#"<div class="tile"><a href="{origin}"><img src="{origin}/favicon.ico" data-fallback="rug://assets/favicon.svg"><span>{label}</span></a><a class="remove" href="{HOME}?forget_host={forget}" title="Remove from history">×</a></div>"#,
                origin = esc(origin),
                forget = urls::encode(host),
                label = esc(host.strip_prefix("www.").unwrap_or(host)),
            )
        })
        .collect();

    let recent: String = store::recent(8)
        .iter()
        .map(|v| {
            let label = if v.title.is_empty() { &v.url } else { &v.title };
            format!(
                r#"<li><img src="{}"><a href="{}">{}</a><a class="remove" href="{HOME}?forget={}" title="Remove from history">×</a></li>"#,
                esc(&favicon_url(&v.url)), esc(&v.url), esc(label), urls::encode(&v.url),
            )
        })
        .collect();

    render(HOME_TEMPLATE, &[
        ("theme", theme(store::settings().dark)),
        ("bookmarks", &bookmarks),
        ("frequent", &frequent),
        ("recent", &recent),
        ("bookmark_dialog", &bookmark_dialog("home")),
    ])
}

/// Renders the bookmark manager, or applies the add/edit/delete in the
/// query string and redirects to where it came from.
fn bookmarks_page(query: &str) -> String {
    let params = urls::parse_query(query);
    let param = |k: &str| params.get(k).map(String::as_str).unwrap_or_default();
    let id = |k: &str| param(k).parse::<u64>().ok();

    if !param("action").is_empty() {
        let url = (param("kind") != "folder").then(|| param("url"));
        match (param("action"), id("id")) {
            ("add", _) => bookmarks::add(id("parent"), param("title"), url),
            ("edit", Some(bookmark)) => bookmarks::edit(bookmark, param("title"), url, id("parent")),
            ("delete", Some(bookmark)) => bookmarks::delete(bookmark),
            ("move", Some(bookmark)) => bookmarks::move_to(bookmark, id("parent"), id("before")),
            _ => {}
        }
        return redirect(if param("return") == "home" { HOME } else { BOOKMARKS });
    }

    render(BOOKMARKS_TEMPLATE, &[
        ("theme", theme(store::settings().dark)),
        ("tree", &bookmark_tree(None)),
        ("bookmark_dialog", &bookmark_dialog("bookmarks")),
    ])
}

fn bookmark_tree(parent: Option<u64>) -> String {
    bookmarks::children(parent)
        .iter()
        .map(|b| {
            let id = b.id;
            let title = esc(&b.title);
            let parent = b.parent.map(|p| p.to_string()).unwrap_or_default();
            match &b.url {
                None => format!(
                    r#"<li class="folder" id="f{id}" data-id="{id}" data-parent="{parent}" draggable="true"><details open><summary><span class="name">{title}</span><span class="actions">
<button data-bookmark="add" data-parent="{id}" title="Add bookmark here">＋</button>
<button data-bookmark="add-folder" data-parent="{id}" title="New subfolder">📁</button>
<button data-bookmark="edit" data-folder data-id="{id}" data-title="{title}" data-parent="{parent}" title="Edit">✏️</button>
<button data-bookmark="delete" data-folder data-id="{id}" data-title="{title}" title="Delete">🗑️</button>
</span></summary><ul>{children}</ul></details></li>"#,
                    children = bookmark_tree(Some(id)),
                ),
                Some(url) => format!(
                    r#"<li class="link" data-id="{id}" data-parent="{parent}" draggable="true"><img src="{icon}"><a href="{url}">{title}</a><span class="host">{host}</span><span class="actions">
<button data-bookmark="edit" data-id="{id}" data-title="{title}" data-url="{url}" data-parent="{parent}" title="Edit">✏️</button>
<button data-bookmark="delete" data-id="{id}" data-title="{title}" title="Delete">🗑️</button>
</span></li>"#,
                    icon = esc(&favicon_url(url)),
                    url = esc(url),
                    host = esc(urls::origin(url).map(|(_, h)| h).unwrap_or_default()),
                ),
            }
        })
        .collect()
}

/// The add/edit modal; `return_to` is the page it sends you back to.
fn bookmark_dialog(return_to: &str) -> String {
    let options: String = bookmarks::folders()
        .iter()
        .map(|f| {
            let path: Vec<String> = f.path.iter().map(u64::to_string).collect();
            format!(
                r#"<option value="{}" data-path="{}">{}{}</option>"#,
                f.id,
                path.join(" "),
                "&nbsp;&nbsp;&nbsp;&nbsp;".repeat(f.path.len()),
                esc(&f.title),
            )
        })
        .collect();
    render(BOOKMARK_DIALOG, &[("return", return_to), ("folder_options", &options)])
}

/// Renders the settings page, first applying whatever actions or form
/// submission are in the query string.
async fn settings(query: &str) -> String {
    let params = urls::parse_query(query);
    let param = |k: &str| params.get(k).map(String::as_str);

    let history_cleared = param("clear") == Some("1");
    let cache_cleared = param("clear_cache") == Some("1");
    let cookies_cleared = param("clear_cookies") == Some("1");
    if history_cleared {
        store::clear_history();
    }
    if cache_cleared {
        let dir = paths::webkit_cache_dir();
        let _ = std::fs::remove_dir_all(&dir);
        let _ = std::fs::create_dir_all(&dir);
    }
    if cookies_cleared {
        let _ = std::fs::remove_file(paths::cookies_file());
    }

    if let Some(engine) = param("engine").and_then(Engine::parse) {
        store::update_settings(|s| {
            s.engine = engine;
            s.custom_url = param("custom_url").unwrap_or_default().to_string();
            s.dark = param("theme") == Some("dark");
            s.cache = param("cache") != Some("disabled");
        });
        crate::apply_theme(param("theme") == Some("dark"));
    }

    let mut password_error = String::new();
    if let Some(key) = param("gpg_key").filter(|k| !k.is_empty() && passwords::keys().first().map(String::as_str) != Some(k)) {
        let key = key.to_string();
        match gio::spawn_blocking(move || passwords::set_key(&key)).await {
            Ok(Ok(())) => sync::sync_soon(),
            Ok(Err(e)) => password_error = format!("Couldn't switch keys: {e}"),
            Err(_) => password_error = "Couldn't switch keys: gpg crashed".into(),
        }
    }
    let mut sync_error = String::new();
    if let Some(remote) = param("remote").filter(|r| r.trim() != sync::remote()) {
        if let Err(e) = sync::set_remote(remote) {
            sync_error = format!("Couldn't set the remote: {e}");
        }
    }
    // Sync Now submits the form, so the remote it uses is saved above.
    if param("sync") == Some("1") && sync_error.is_empty() {
        sync::sync_now(false).await;
        return redirect(SETTINGS);
    }

    let sync_failed = !sync_error.is_empty() || sync::status().starts_with("Sync failed");
    let sync_status = if sync_error.is_empty() { sync::status() } else { sync_error };

    let current_key = passwords::keys().into_iter().next().unwrap_or_default();
    let mut key_options = String::new();
    if current_key.is_empty() {
        key_options.push_str(r#"<option value="" selected>Choose a key…</option>"#);
    }
    for (fingerprint, uid) in passwords::secret_keys() {
        let selected = if fingerprint == current_key { "selected" } else { "" };
        let short = &fingerprint[fingerprint.len().saturating_sub(8)..];
        key_options.push_str(&format!(r#"<option value="{}" {selected}>{} ({short})</option>"#, esc(&fingerprint), esc(&uid)));
    }

    let s = store::settings();
    let checked = |on: bool| if on { "checked" } else { "" };
    let hidden = |on: bool| if on { "" } else { "hidden" };
    render(SETTINGS_TEMPLATE, &[
        ("theme", theme(s.dark)),
        ("ddg", checked(s.engine == Engine::Ddg)),
        ("google", checked(s.engine == Engine::Google)),
        ("bing", checked(s.engine == Engine::Bing)),
        ("custom", checked(s.engine == Engine::Custom)),
        ("custom_url", &esc(&s.custom_url)),
        ("custom_url_hidden", hidden(s.engine == Engine::Custom)),
        ("light", checked(!s.dark)),
        ("dark", checked(s.dark)),
        ("cache_enabled", checked(s.cache)),
        ("cache_disabled", checked(!s.cache)),
        ("history_msg_hidden", hidden(history_cleared)),
        ("cache_msg_hidden", hidden(cache_cleared)),
        ("cookies_msg_hidden", hidden(cookies_cleared)),
        ("key_options", &key_options),
        ("password_error", &esc(&password_error)),
        ("password_error_hidden", hidden(!password_error.is_empty())),
        ("remote", &esc(&sync::remote())),
        ("profile_dir", &esc(&paths::profile_dir().to_string_lossy())),
        ("sync_status", &esc(&sync_status)),
        ("sync_class", if sync_failed { "error" } else { "msg" }),
    ])
}

/// Lists saved logins; deletes the one in the query, or adds the one in
/// the POSTed `form`.
async fn passwords_page(query: &str, form: &str) -> String {
    let params = urls::parse_query(query);
    if let (Some(host), Some(username)) = (params.get("host"), params.get("username")) {
        passwords::delete(&Login::new(host, username));
        sync::sync_soon();
        return redirect(PASSWORDS);
    }

    let added = urls::parse_query(form);
    let field = |k: &str| added.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
    let (site, username) = (field("site"), field("username"));
    let password = added.get("password").cloned().unwrap_or_default();
    let mut error = String::new();
    if !site.is_empty() {
        match site_host(&site) {
            None => error = format!("“{site}” isn't a site address."),
            Some(_) if password.is_empty() => error = "Enter a password.".into(),
            Some(host) => {
                let login = Login::new(&host, &username);
                let url = format!("https://{host}");
                match gio::spawn_blocking(move || passwords::save(&login, &password, &url)).await {
                    Ok(Ok(())) => {
                        sync::sync_soon();
                        return redirect(PASSWORDS);
                    }
                    Ok(Err(e)) => error = format!("Couldn't save: {e}"),
                    Err(_) => error = "Couldn't save: gpg crashed".into(),
                }
            }
        }
    }

    let rows: String = passwords::all()
        .iter()
        .map(|l| {
            format!(
                r#"<li><img src="https://{host}/favicon.ico" data-fallback="rug://assets/favicon.svg"><a href="https://{host}">{host}</a><span class="username">{name}</span><a class="remove" href="{PASSWORDS}?host={host_q}&username={user_q}" title="Delete" data-confirm="Delete the saved password for {name} on {host}?">×</a></li>"#,
                host = esc(&l.host),
                name = esc(if l.username.is_empty() { "(no username)" } else { &l.username }),
                host_q = urls::encode(&l.host),
                user_q = urls::encode(&l.username),
            )
        })
        .collect();
    let key = passwords::keys().join(", ");
    render(PASSWORDS_TEMPLATE, &[
        ("theme", theme(store::settings().dark)),
        ("rows", &rows),
        ("key", &esc(if key.is_empty() { "no key chosen" } else { &key })),
        ("dir", &esc(&paths::passwords_dir().to_string_lossy())),
        ("error", &esc(&error)),
        // Keep what was typed (except the password) when saving failed.
        ("site", &esc(&site)),
        ("username", &esc(&username)),
    ])
}

/// The host to save a login under, from a host or a pasted URL: what the
/// save prompt would use on that site.
fn site_host(site: &str) -> Option<String> {
    let url = if site.contains("://") { site.to_string() } else { format!("https://{site}") };
    let (_, host) = urls::origin(&url)?;
    let host = host.rsplit('@').next()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    (!host.is_empty() && !host.contains(char::is_whitespace)).then(|| host.to_string())
}

fn open_private_window() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--private").spawn();
    }
}

/// A page that replaces itself in history with `target`, so going back
/// doesn't repeat whatever action led here.
fn redirect(target: &str) -> String {
    format!(r#"<!DOCTYPE html><meta http-equiv="refresh" content="0;url={target}"><script>location.replace("{target}")</script>"#)
}

fn favicon_url(url: &str) -> String {
    urls::origin(url).map(|(origin, _)| format!("{origin}/favicon.ico")).unwrap_or_default()
}

fn theme(dark: bool) -> &'static str {
    if dark || store::is_private() { "dark" } else { "light" }
}

/// Single pass, so substituted values (page titles, ...) are never re-expanded.
fn render(template: &str, vars: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start..].find("}}") else { break };
        let name = &rest[start + 2..start + len];
        out.push_str(&rest[..start]);
        match vars.iter().find(|(n, _)| *n == name) {
            Some((_, value)) => out.push_str(value),
            None => out.push_str(&rest[start..start + len + 2]),
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
