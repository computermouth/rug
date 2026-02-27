
const GRAY_CIRCLE: &str = "data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 16 16%22%3E%3Ccircle cx=%228%22 cy=%228%22 r=%227%22 fill=%22%23b4b4b4%22/%3E%3C/svg%3E";

pub fn home(pages: &[(String, String)], favorites: &[(String, String)]) -> String {
    let mut tiles = String::new();
    for (url, host) in favorites {
        let fav = favicon_url(url).unwrap_or_default();
        tiles.push_str(&format!(
            "<a class=\"tile\" href=\"{}\"><img src=\"{}\" onerror=\"this.onerror=null;this.src='{}'\"><span>{}</span></a>",
            esc(url), esc(&fav), GRAY_CIRCLE, esc(host.strip_prefix("www.").unwrap_or(host))
        ));
    }

    let mut items = String::new();
    for (url, title) in pages {
        let fav = favicon_url(url).unwrap_or_default();
        let label = if title.is_empty() { url.as_str() } else { title.as_str() };
        items.push_str(&format!(
            "<li><img src=\"{}\" onerror=\"this.style.visibility='hidden'\"><a href=\"{}\">{}</a></li>",
            esc(&fav), esc(url), esc(label)
        ));
    }

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>home</title><style>
body{{margin:0;font-family:Arial,sans-serif;display:flex;flex-direction:column;align-items:center;padding-top:20vh;background:#fff}}
h1{{font-size:3em;margin:0 0 1em}}
h2{{font-size:.75em;color:#999;text-transform:uppercase;letter-spacing:.08em;margin:0 0 8px;font-weight:600}}
.main{{display:flex;gap:32px;align-items:flex-start}}
.col{{display:flex;flex-direction:column}}
.grid{{display:grid;grid-template-columns:repeat(4,1fr);gap:6px;width:240px}}
.tile{{display:flex;flex-direction:column;align-items:center;padding:10px 4px 8px;border-radius:6px;text-decoration:none;color:#333;gap:5px;min-width:0}}
.tile:hover{{background:#f0f0f0}}
.tile img{{width:24px;height:24px}}
.tile span{{font-size:.65em;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;width:100%;text-align:center;color:#555}}
ul{{list-style:none;padding:4px;margin:0;width:320px;border:4px solid #e0e0e0;border-radius:6px}}
li{{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:4px}}
li:hover{{background:#f0f0f0}}
li img{{width:16px;height:16px;flex-shrink:0}}
li a{{text-decoration:none;color:#333;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
li a:hover{{text-decoration:underline}}
.settings{{position:fixed;top:16px;right:24px;color:#aaa;font-size:.875em;text-decoration:none}}
.settings:hover{{color:#333}}
</style></head>
<body><a class="settings" href="rug://settings">Settings</a><h1>rug</h1>
<div class="main">
<div class="col"><h2>Favorites</h2><div class="grid">{}</div></div>
<div class="col"><h2>Recent</h2><ul>{}</ul></div>
</div></body></html>"#, tiles, items)
}

pub fn settings(cleared: bool) -> String {
    let msg = if cleared { "<p class=\"msg\">History cleared.</p>" } else { "" };
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>Settings — rug</title><style>
body{{margin:0;font-family:Arial,sans-serif;display:flex;flex-direction:column;align-items:center;padding-top:20vh;background:#fff}}
h1{{font-size:3em;margin:0 0 1em}}
.msg{{color:green;margin:0 0 1em}}
a.btn{{display:inline-block;padding:8px 20px;background:#e0e0e0;border-radius:6px;color:#333;text-decoration:none}}
a.btn:hover{{background:#d0d0d0}}
</style></head>
<body><h1>Settings</h1>{}<a class="btn" href="rug://settings?clear=1">Clear History</a></body></html>"#, msg)
}

fn favicon_url(url: &str) -> Option<String> {
    let sep = url.find("://")?;
    let after = &url[sep + 3..];
    let host_end = after.find('/').unwrap_or(after.len());
    let host = &after[..host_end];
    if host.is_empty() { return None; }
    Some(format!("{}://{}/favicon.ico", &url[..sep], host))
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
