
const GRAY_CIRCLE: &str = "data:image/svg+xml,%3Csvg xmlns=%22http://www.w3.org/2000/svg%22 viewBox=%220 0 16 16%22%3E%3Ccircle cx=%228%22 cy=%228%22 r=%227%22 fill=%22%23b4b4b4%22/%3E%3C/svg%3E";

pub fn home(pages: &[(String, String)], favorites: &[(String, String)], dark: bool, private: bool) -> String {
    let dark = dark || private;

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

    let dark_css = if dark { r"
body{background:#18181b;color:#e4e4e7}
h2{color:#52525b}
.tile{color:#e4e4e7;background:#27272a}
.tile:hover{background:#3f3f46}
.tile span{color:#a1a1aa}
ul{border-color:#3f3f46}
li a{color:#e4e4e7}
li:hover{background:#27272a}
.settings{color:#52525b}
.settings:hover{color:#e4e4e7}
.private{color:#52525b}
.private:hover{color:#e4e4e7}
" } else { "" };

    let private_link = if private { "" } else { "<a class=\"private\" href=\"rug://private\">Private Browsing</a>" };
    let settings_link = if private { "" } else { "<a class=\"settings\" href=\"rug://settings\">Settings</a>" };

    let main_content = if private {
        String::from("<div class=\"private-badge\">Private Browsing</div>")
    } else {
        format!(
            "<div class=\"main\">\n<div class=\"col\"><h2>Favorites</h2><div class=\"grid\">{}</div></div>\n<div class=\"col\"><h2>Recent</h2><ul>{}</ul></div>\n</div>",
            tiles, items
        )
    };

    format!(r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>home</title><style>
body{{margin:0;font-family:Arial,sans-serif;display:flex;flex-direction:column;align-items:center;padding-top:10vh;background:#fff}}
h1{{font-size:3em;margin:0 0 .5em}}
h2{{font-size:.75em;color:#999;text-transform:uppercase;letter-spacing:.08em;margin:0 0 8px;font-weight:600}}
.main{{display:flex;gap:32px;align-items:flex-start}}
.col{{display:flex;flex-direction:column}}
.grid{{display:grid;grid-template-columns:repeat(4,1fr);gap:6px;width:240px}}
.tile{{display:flex;flex-direction:column;align-items:center;padding:10px 4px 8px;border-radius:14px;text-decoration:none;color:#333;gap:5px;min-width:0;background:#f0f0f0}}
.tile:hover{{background:#e0e0e0}}
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
.private{{position:fixed;top:16px;left:24px;color:#aaa;font-size:.875em;text-decoration:none}}
.private:hover{{color:#333}}
.private-badge{{background:#3f3f46;color:#a1a1aa;border-radius:20px;padding:4px 16px;font-size:.75em;letter-spacing:.08em;text-transform:uppercase;font-weight:600;margin-bottom:2em}}
{}</style></head>
<body>{}{}<h1>rug</h1>
{}</body></html>"#, dark_css, private_link, settings_link, main_content)
}

pub fn settings(cleared: bool, engine: &str, custom_url: &str, dark: bool, cache_enabled: bool, cache_cleared: bool, cookies_cleared: bool) -> String {
    let msg = if cleared { "<p class=\"msg\">History cleared.</p>" } else { "" };
    let cache_msg = if cache_cleared { "<p class=\"msg\">Cache cleared.</p>" } else { "" };
    let cookies_msg = if cookies_cleared { "<p class=\"msg\">Cookies cleared. Takes effect after restart.</p>" } else { "" };
    let chk = |e: &str| if engine == e { " checked" } else { "" };
    let custom_display = if engine == "custom" { "block" } else { "none" };
    let dark_css = if dark { r"
body{background:#18181b;color:#e4e4e7}
h2{color:#52525b}
input[type=text]{background:#27272a;color:#e4e4e7;border-color:#3f3f46}
button,.btn{background:#27272a;color:#e4e4e7}
button:hover,.btn:hover{background:#3f3f46}
button.dirty{background:#3b82f6;color:#fff}
button.dirty:hover{background:#2563eb}
.home{color:#52525b}
.home:hover{color:#e4e4e7}
" } else { "" };
    format!(r#"<!DOCTYPE html>
<html lang="en">
<head><meta charset="UTF-8"><title>settings</title><style>
body{{margin:0;font-family:Arial,sans-serif;display:flex;flex-direction:column;align-items:center;padding-top:10vh;background:#fff}}
h1{{font-size:3em;margin:0 0 .75em}}
h2{{font-size:.85em;color:#999;text-transform:uppercase;letter-spacing:.08em;margin:0 0 10px;font-weight:600}}
.cols{{display:flex;gap:48px;align-items:stretch}}
.col{{display:flex;flex-direction:column;min-width:220px}}
.section{{margin-bottom:28px}}
label{{display:block;margin:6px 0;cursor:pointer;font-size:.95em}}
input[type=radio]{{margin-right:6px}}
input[type=text]{{width:100%;box-sizing:border-box;padding:6px 8px;border:1px solid #ccc;border-radius:4px;font-size:.9em;margin-top:6px}}
button,.btn{{display:inline-block;padding:7px 18px;background:#e0e0e0;border:none;border-radius:6px;color:#333;text-decoration:none;font-size:.9em;cursor:pointer}}
button:hover,.btn:hover{{background:#d0d0d0}}
button:disabled{{background:#e0e0e0;color:#bbb;cursor:not-allowed;opacity:.5}}
button.dirty{{background:#3b82f6;color:#fff;opacity:1}}
button.dirty:hover{{background:#2563eb}}
.msg{{color:green;margin:0 0 10px;font-size:.9em}}
.note{{font-size:.8em;color:#999;margin:4px 0 0}}
.home{{position:fixed;top:16px;right:24px;color:#aaa;font-size:.875em;text-decoration:none}}
.home:hover{{color:#333}}
{}</style></head>
<body><a class="home" href="rug://home">Home</a><h1>settings</h1>
<div class="cols">
<form id="settings-form" method="get" action="rug://settings" class="col">
<div class="section">
  <h2>Search</h2>
  <label><input type="radio" name="engine" value="ddg"{}> DuckDuckGo</label>
  <label><input type="radio" name="engine" value="google"{}> Google</label>
  <label><input type="radio" name="engine" value="bing"{}> Bing</label>
  <label><input type="radio" name="engine" value="custom"{}> Custom</label>
  <div id="cr" style="display:{}">
    <input type="text" name="custom_url" value="{}" placeholder="https://example.com/search?q=">
  </div>
</div>
<div class="section">
  <h2>Theme</h2>
  <label><input type="radio" name="theme" value="light"{}> Light</label>
  <label><input type="radio" name="theme" value="dark"{}> Dark</label>
</div>
<div class="section">
  <h2>Cache</h2>
  <label><input type="radio" name="cache" value="enabled"{}> Enabled</label>
  <label><input type="radio" name="cache" value="disabled"{}> Disabled</label>
  <p class="note">Takes effect on next launch.</p>
</div>
</form>
<div class="col">
<div class="section">
  <h2>History</h2>
  {}<a class="btn" href="rug://settings?clear=1">Clear History</a>
</div>
<div class="section">
  <h2>Cache Data</h2>
  {}<a class="btn" href="rug://settings?clear_cache=1">Clear Cache</a>
</div>
<div class="section">
  <h2>Cookies</h2>
  {}<a class="btn" href="rug://settings?clear_cookies=1">Clear Cookies</a>
</div>
<button type="submit" form="settings-form" id="apply" disabled style="margin-top:auto;margin-bottom:20px">Apply Changes</button>
</div>
</div>
<script>
(function(){{
  var btn = document.getElementById('apply');
  var init = {{}};
  document.querySelectorAll('input[name=engine],input[name=theme],input[name=cache]').forEach(function(r){{
    if(r.checked) init[r.name] = r.value;
  }});
  var cu = document.querySelector('input[name=custom_url]');
  if(cu) init['custom_url'] = cu.value;
  function check(){{
    var dirty = false;
    document.querySelectorAll('input[name=engine],input[name=theme],input[name=cache]').forEach(function(r){{
      if(r.checked && init[r.name] !== r.value) dirty = true;
    }});
    var cu = document.querySelector('input[name=custom_url]');
    if(cu && cu.value !== init['custom_url']) dirty = true;
    btn.classList.toggle('dirty', dirty);
    btn.disabled = !dirty;
  }}
  document.querySelectorAll('input[name=engine],input[name=theme],input[name=cache]').forEach(function(r){{
    r.addEventListener('change', function(){{
      if(r.name === 'engine') document.getElementById('cr').style.display = document.querySelector('input[value=custom]').checked ? 'block' : 'none';
      check();
    }});
  }});
  if(cu) cu.addEventListener('input', check);
}})();
</script>
</body></html>"#,
        dark_css,
        chk("ddg"), chk("google"), chk("bing"), chk("custom"),
        custom_display, esc(custom_url),
        if dark { "" } else { " checked" }, if dark { " checked" } else { "" },
        if cache_enabled { " checked" } else { "" }, if !cache_enabled { " checked" } else { "" },
        msg, cache_msg, cookies_msg)
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
