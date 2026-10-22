//! The internal `rug://` pages. Templates and their assets live in
//! `assets/pages`; `{{name}}` placeholders are filled in here.

use gtk::{gio, glib};
use webkit6::{URISchemeRequest, WebContext};

use crate::store::{self, Engine};
use crate::{paths, urls};

pub const HOME: &str = "rug://home";

const HOME_TEMPLATE: &str = include_str!("../assets/pages/home.html");
const PRIVATE_HOME: &str = include_str!("../assets/pages/private.html");
const SETTINGS_TEMPLATE: &str = include_str!("../assets/pages/settings.html");
const REDIRECT_HOME: &str =
    r#"<!DOCTYPE html><meta http-equiv="refresh" content="0;url=rug://home">"#;
const NOT_FOUND: &str = "<!DOCTYPE html><p>Not found</p>";

/// Served at `rug://assets/<name>`.
const ASSETS: &[(&str, &str, &[u8])] = &[
    ("rug.css", "text/css", include_bytes!("../assets/pages/rug.css")),
    ("home.js", "text/javascript", include_bytes!("../assets/pages/home.js")),
    ("settings.js", "text/javascript", include_bytes!("../assets/pages/settings.js")),
    ("favicon.svg", "image/svg+xml", include_bytes!("../assets/pages/favicon.svg")),
];

pub fn register(context: &WebContext) {
    context.register_uri_scheme("rug", handle);
}

fn handle(request: &URISchemeRequest) {
    let uri = request.uri().unwrap_or_default();
    let rest = uri.strip_prefix("rug://").unwrap_or_default();
    let (path, query) = rest.split_once('?').unwrap_or((rest, ""));

    let (mime, body) = match path {
        "home" => html(home()),
        "settings" => html(settings(query)),
        "private" => {
            open_private_window();
            html(REDIRECT_HOME.into())
        }
        _ => match ASSETS.iter().find(|(name, ..)| path.strip_prefix("assets/") == Some(name)) {
            Some(&(_, mime, bytes)) => (mime, glib::Bytes::from_static(bytes)),
            None => html(NOT_FOUND.into()),
        },
    };
    let stream = gio::MemoryInputStream::from_bytes(&body);
    request.finish(&stream, body.len() as i64, Some(mime));
}

fn html(page: String) -> (&'static str, glib::Bytes) {
    ("text/html", glib::Bytes::from_owned(page.into_bytes()))
}

fn home() -> String {
    if store::is_private() {
        return PRIVATE_HOME.into();
    }

    let favorites: String = store::top_sites(16)
        .iter()
        .map(|(origin, host)| {
            format!(
                r#"<a class="tile" href="{origin}"><img src="{origin}/favicon.ico" data-fallback="rug://assets/favicon.svg"><span>{label}</span></a>"#,
                origin = esc(origin),
                label = esc(host.strip_prefix("www.").unwrap_or(host)),
            )
        })
        .collect();

    let recent: String = store::recent(8)
        .iter()
        .map(|v| {
            let icon = urls::origin(&v.url).map(|(o, _)| format!("{o}/favicon.ico")).unwrap_or_default();
            let label = if v.title.is_empty() { &v.url } else { &v.title };
            format!(r#"<li><img src="{}"><a href="{}">{}</a></li>"#, esc(&icon), esc(&v.url), esc(label))
        })
        .collect();

    render(HOME_TEMPLATE, &[
        ("theme", theme(store::settings().dark)),
        ("favorites", &favorites),
        ("recent", &recent),
    ])
}

/// Renders the settings page, first applying whatever actions or form
/// submission are in the query string.
fn settings(query: &str) -> String {
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
    ])
}

fn open_private_window() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("--private").spawn();
    }
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
