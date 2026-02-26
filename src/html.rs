
pub fn home(pages: &[(String, String)]) -> String {
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
<head><meta charset="UTF-8"><title>rug</title><style>
body{{margin:0;font-family:Arial,sans-serif;display:flex;flex-direction:column;align-items:center;padding-top:20vh;background:#fff}}
h1{{font-size:3em;margin:0 0 1em}}
ul{{list-style:none;padding:4px;margin:0;width:480px;border:4px solid #e0e0e0;border-radius:6px}}
li{{display:flex;align-items:center;gap:8px;padding:6px 8px;border-radius:4px}}
li:hover{{background:#f0f0f0}}
img{{width:16px;height:16px;flex-shrink:0}}
a{{text-decoration:none;color:#333;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}}
a:hover{{text-decoration:underline}}
</style></head>
<body><h1>rug</h1><ul>{}</ul></body></html>"#, items)
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
