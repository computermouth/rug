use gtk4::prelude::*;
use gtk4::gdk::Key;
use webkit6::prelude::*;
use webkit6::WebView;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Image, Label, ListBox, Notebook, Orientation, Entry, Button, Overlay, ProgressBar};
use glib::clone;
use std::cell::RefCell;

mod html;

thread_local! {
    static RECENT_PAGES: RefCell<Vec<(String, String, Option<String>)>> = RefCell::new(Vec::new());
    static FAVICON_CACHE: RefCell<std::collections::HashMap<String, gtk4::gdk::Texture>> =
        RefCell::new(std::collections::HashMap::new());
    static SEARCH_ENGINE: RefCell<String> = RefCell::new("ddg".to_string());
    static CUSTOM_SEARCH_URL: RefCell<String> = RefCell::new(String::new());
    static DARK_MODE: RefCell<bool> = RefCell::new(false);
    static CACHE_ENABLED: RefCell<bool> = RefCell::new(true);
    static NETWORK_SESSION: RefCell<Option<webkit6::NetworkSession>> = RefCell::new(None);
    static PRIVATE: RefCell<bool> = RefCell::new(false);
}

fn update_recent(url: &str, title: &str) {
    if PRIVATE.with(|i| *i.borrow()) { return; }
    if url.is_empty() || url.starts_with("about:") || url.starts_with("rug:") { return; }
    RECENT_PAGES.with(|rp| {
        let mut pages = rp.borrow_mut();
        if let Some(pos) = pages.iter().position(|(u, _, _)| u == url) {
            let (u, old_title, favicon) = pages.remove(pos);
            let t = if title.is_empty() { old_title } else { title.to_string() };
            pages.insert(0, (u, t, favicon));
        } else {
            let t = if title.is_empty() { url.to_string() } else { title.to_string() };
            pages.insert(0, (url.to_string(), t, None));
        }
        pages.truncate(1000);
    });
    save_recent();
}

fn update_recent_favicon(url: &str, texture: &gtk4::gdk::Texture) {
    if PRIVATE.with(|i| *i.borrow()) { return; }
    if url.is_empty() || url.starts_with("about:") || url.starts_with("rug:") { return; }
    let data_uri = texture_to_data_uri(texture);
    RECENT_PAGES.with(|rp| {
        let mut pages = rp.borrow_mut();
        if let Some(entry) = pages.iter_mut().find(|(u, _, _)| u.as_str() == url) {
            entry.2 = Some(data_uri);
        }
    });
    FAVICON_CACHE.with(|fc| fc.borrow_mut().insert(url.to_string(), texture.clone()));
    save_recent();
}

fn recent_pages_snapshot() -> Vec<(String, String, Option<String>)> {
    RECENT_PAGES.with(|rp| rp.borrow().clone())
}

fn url_decode(s: &str) -> String {
    let mut result = Vec::new();
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => { result.push(b' '); i += 1; }
            b'%' if i + 2 < bytes.len() => {
                let hi = (bytes[i + 1] as char).to_digit(16);
                let lo = (bytes[i + 2] as char).to_digit(16);
                if let (Some(h), Some(l)) = (hi, lo) {
                    result.push((h * 16 + l) as u8);
                    i += 3;
                } else {
                    result.push(b'%'); i += 1;
                }
            }
            b => { result.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn parse_query_params(uri: &str) -> std::collections::HashMap<String, String> {
    let mut map = std::collections::HashMap::new();
    if let Some(q) = uri.splitn(2, '?').nth(1) {
        for pair in q.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                map.insert(url_decode(k), url_decode(v));
            }
        }
    }
    map
}

fn data_path() -> std::path::PathBuf {
    #[cfg(debug_assertions)]
    { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("tmp").join("data.json") }
    #[cfg(not(debug_assertions))]
    { glib::home_dir().join(".local").join("share").join("rug").join("data.json") }
}

fn settings_path() -> std::path::PathBuf {
    #[cfg(debug_assertions)]
    { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("tmp").join("settings.json") }
    #[cfg(not(debug_assertions))]
    { glib::home_dir().join(".config").join("rug").join("settings.json") }
}

fn wk_cache_path() -> std::path::PathBuf {
    #[cfg(debug_assertions)]
    { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/wk_cache") }
    #[cfg(not(debug_assertions))]
    { glib::home_dir().join(".cache/rug") }
}

fn cookies_path() -> std::path::PathBuf {
    #[cfg(debug_assertions)]
    { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/wk_data/cookies.sqlite") }
    #[cfg(not(debug_assertions))]
    { glib::home_dir().join(".local/share/rug/cookies.sqlite") }
}

fn clear_cache() {
    let path = wk_cache_path();
    let _ = std::fs::remove_dir_all(&path);
    let _ = std::fs::create_dir_all(&path);
}

fn clear_cookies() {
    let _ = std::fs::remove_file(cookies_path());
}

fn apply_dark_mode(dark: bool) {
    if let Some(s) = gtk4::Settings::default() {
        s.set_gtk_application_prefer_dark_theme(dark);
    }
}

fn save_settings() {
    let engine = SEARCH_ENGINE.with(|e| e.borrow().clone());
    let custom_url = CUSTOM_SEARCH_URL.with(|u| u.borrow().clone());
    let dark = DARK_MODE.with(|d| *d.borrow());
    let cache = CACHE_ENABLED.with(|c| *c.borrow());
    let path = settings_path();
    if let Some(parent) = path.parent() { let _ = std::fs::create_dir_all(parent); }
    if let (Ok(e), Ok(u)) = (serde_json::to_string(&engine), serde_json::to_string(&custom_url)) {
        let _ = std::fs::write(path, format!(
            "{{\"engine\":{},\"custom_url\":{},\"dark\":{},\"cache\":{}}}",
            e, u, dark, cache
        ));
    }
}

fn load_settings() {
    let path = settings_path();
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
            if let Some(e) = v["engine"].as_str() {
                SEARCH_ENGINE.with(|s| *s.borrow_mut() = e.to_string());
            }
            if let Some(u) = v["custom_url"].as_str() {
                CUSTOM_SEARCH_URL.with(|s| *s.borrow_mut() = u.to_string());
            }
            if let Some(d) = v["dark"].as_bool() {
                DARK_MODE.with(|s| *s.borrow_mut() = d);
            }
            if let Some(c) = v["cache"].as_bool() {
                CACHE_ENABLED.with(|s| *s.borrow_mut() = c);
            }
        }
    }
}

fn search_url(query: &str) -> String {
    let engine = SEARCH_ENGINE.with(|e| e.borrow().clone());
    match engine.as_str() {
        "google" => format!("https://www.google.com/search?q={}", url_encode(query)),
        "bing"   => format!("https://www.bing.com/search?q={}", url_encode(query)),
        "custom" => {
            let base = CUSTOM_SEARCH_URL.with(|u| u.borrow().clone());
            format!("{}{}", base, url_encode(query))
        }
        _ => format!("https://duckduckgo.com/?q={}", url_encode(query)),
    }
}

fn save_recent() {
    RECENT_PAGES.with(|rp| {
        let pages = rp.borrow();
        let path = data_path();
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_string(&*pages) {
            let _ = std::fs::write(path, json);
        }
    });
}

fn clear_history() {
    RECENT_PAGES.with(|rp| rp.borrow_mut().clear());
    FAVICON_CACHE.with(|fc| fc.borrow_mut().clear());
    save_recent();
}

fn load_recent() {
    let path = data_path();
    if let Ok(content) = std::fs::read_to_string(path) {
        let pages: Vec<(String, String, Option<String>)> =
            if let Ok(p) = serde_json::from_str::<Vec<(String, String, Option<String>)>>(&content) {
                p
            } else if let Ok(p) = serde_json::from_str::<Vec<(String, String)>>(&content) {
                p.into_iter().map(|(u, t)| (u, t, None)).collect()
            } else {
                return;
            };
        FAVICON_CACHE.with(|fc| {
            let mut cache = fc.borrow_mut();
            for (url, _, favicon) in &pages {
                if let Some(data_uri) = favicon {
                    if let Some(texture) = texture_from_data_uri(data_uri) {
                        cache.insert(url.clone(), texture);
                    }
                }
            }
        });
        RECENT_PAGES.with(|rp| *rp.borrow_mut() = pages);
    }
}

fn url_encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            b => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

fn smart_uri(input: &str) -> String {
    let s = input.trim();
    if s.contains("://") {
        return s.to_string();
    }
    if s.contains(' ')
        || (!s.contains('.') && !s.starts_with("localhost"))
    {
        return search_url(s);
    }
    format!("http://{}", s)
}

fn top_domains(max: usize) -> Vec<(String, String)> {
    let mut counts: std::collections::HashMap<String, (String, usize)> = std::collections::HashMap::new();
    RECENT_PAGES.with(|rp| {
        for (url, _, _) in rp.borrow().iter() {
            let Some(sep) = url.find("://") else { continue };
            let after = &url[sep + 3..];
            let end = after.find('/').unwrap_or(after.len());
            let h = &after[..end];
            if h.is_empty() { continue; }
            let root = format!("{}://{}", &url[..sep], h);
            let e = counts.entry(h.to_string()).or_insert((root, 0));
            e.1 += 1;
        }
    });
    let mut v: Vec<(String, String, usize)> = counts.into_iter()
        .map(|(host, (root, n))| (host, root, n))
        .collect();
    v.sort_by(|a, b| b.2.cmp(&a.2));
    v.into_iter().take(max).map(|(host, root, _)| (root, host)).collect()
}

fn search_history(query: &str, max: usize) -> Vec<(String, String, Option<String>)> {
    let q = query.to_lowercase();
    RECENT_PAGES.with(|rp| {
        rp.borrow()
            .iter()
            .filter(|(url, title, _)| url.to_lowercase().contains(&q) || title.to_lowercase().contains(&q))
            .take(max)
            .cloned()
            .collect()
    })
}

fn default_favicon() -> gtk4::gdk::Texture {
    const S: usize = 16;
    let mut px = vec![0u8; S * S * 4];
    let c = S as f64 / 2.0;
    let r = c - 1.0;
    for y in 0..S {
        for x in 0..S {
            let dx = x as f64 + 0.5 - c;
            let dy = y as f64 + 0.5 - c;
            if dx * dx + dy * dy <= r * r {
                let i = (y * S + x) * 4;
                px[i] = 180; px[i+1] = 180; px[i+2] = 180; px[i+3] = 255;
            }
        }
    }
    let bytes = glib::Bytes::from(px.as_slice());
    gtk4::gdk::MemoryTexture::new(S as i32, S as i32, gtk4::gdk::MemoryFormat::R8g8b8a8, &bytes, S * 4)
        .upcast()
}

fn highlight_match(text: &str, query: &str) -> String {
    let lower = text.to_lowercase();
    let q = query.to_lowercase();
    if let Some(start) = lower.find(&q) {
        let end = start + q.len();
        if text.is_char_boundary(start) && text.is_char_boundary(end) {
            return format!(
                "{}<b>{}</b>{}",
                pango_esc(&text[..start]),
                pango_esc(&text[start..end]),
                pango_esc(&text[end..])
            );
        }
    }
    pango_esc(text)
}

fn pango_esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn texture_to_data_uri(texture: &gtk4::gdk::Texture) -> String {
    let bytes = texture.save_to_png_bytes();
    let encoded = glib::base64_encode(bytes.as_ref());
    format!("data:image/png;base64,{}", encoded)
}

fn texture_from_data_uri(data_uri: &str) -> Option<gtk4::gdk::Texture> {
    let b64 = data_uri.strip_prefix("data:image/png;base64,")?;
    let bytes = glib::base64_decode(b64);
    let gbytes = glib::Bytes::from(bytes.as_slice());
    gtk4::gdk::Texture::from_bytes(&gbytes).ok()
}

fn is_active_tab(notebook: &Notebook, webview: &WebView) -> bool {
    notebook.page_num(webview) == notebook.current_page()
}

fn current_webview(notebook: &Notebook) -> Option<WebView> {
    notebook.current_page()
        .and_then(|p| notebook.nth_page(Some(p)))
        .and_then(|w| w.downcast::<WebView>().ok())
}

fn add_tab(
    notebook: &Notebook,
    window: &ApplicationWindow,
    url_bar: &Entry,
    back_button: &Button,
    forward_button: &Button,
    progress_bar: &ProgressBar,
    app: &Application,
    related_view: Option<&WebView>,
    initial_uri: Option<&str>,
) -> WebView {
    let webview = match related_view {
        Some(rv) => webkit6::WebView::builder().related_view(rv).build(),
        None => NETWORK_SESSION.with(|s| match s.borrow().as_ref() {
            Some(ns) => webkit6::WebView::builder().network_session(ns).build(),
            None => WebView::new(),
        }),
    };

    match initial_uri {
        Some(uri) => webview.load_uri(uri),
        None if related_view.is_none() => webview.load_uri("rug://home"),
        _ => {}
    }

    webview.set_hexpand(true);
    webview.set_vexpand(true);

    webview.connect_load_changed(clone!(
        #[weak] notebook, #[weak] back_button, #[weak] forward_button,
        #[weak] webview, #[weak] url_bar, #[weak] progress_bar,
        move |_, load_event| {
            if load_event == webkit6::LoadEvent::Finished {
                let uri = webview.uri().unwrap_or_default();
                let title = webview.title().unwrap_or_default();
                update_recent(&uri, &title);
            }
            if !is_active_tab(&notebook, &webview) { return; }
            back_button.set_sensitive(webview.can_go_back());
            forward_button.set_sensitive(webview.can_go_forward());
            url_bar.set_text(&webview.uri().unwrap_or_default());
            if load_event == webkit6::LoadEvent::Started {
                webview.grab_focus();
            }
            if load_event == webkit6::LoadEvent::Finished {
                progress_bar.set_fraction(0.0);
            }
        }
    ));

    webview.connect_notify_local(
        Some("estimated-load-progress"),
        clone!(#[weak] notebook, #[weak] progress_bar, #[weak] webview, move |_, _| {
            if !is_active_tab(&notebook, &webview) { return; }
            progress_bar.set_fraction(webview.estimated_load_progress());
        }),
    );

    webview.connect_context_menu(clone!(
        #[strong] notebook, #[strong] window, #[strong] url_bar,
        #[strong] back_button, #[strong] forward_button, #[strong] progress_bar, #[strong] app,
        move |_, menu, hit_test| {
            if hit_test.context_is_link() {
                if let Some(link_uri) = hit_test.link_uri() {
                    let uri = link_uri.to_string();
                    let action = gtk4::gio::SimpleAction::new("open-link-in-new-tab", None);
                    action.connect_activate(clone!(
                        #[weak] notebook, #[weak] window, #[weak] url_bar,
                        #[weak] back_button, #[weak] forward_button, #[weak] progress_bar, #[strong] app,
                        move |_, _| {
                            add_tab(&notebook, &window, &url_bar, &back_button,
                                    &forward_button, &progress_bar, &app, None, Some(&uri));
                        }
                    ));
                    let items = menu.items();
                    let pos = items.iter().position(|item| {
                        item.stock_action() == webkit6::ContextMenuAction::OpenLinkInNewWindow
                    });
                    let new_tab_item = webkit6::ContextMenuItem::from_gaction(
                        &action, "Open Link in New Tab", None
                    );
                    match pos {
                        Some(p) => menu.insert(&new_tab_item, p as i32),
                        None => menu.append(&new_tab_item),
                    }
                }
            }

            if hit_test.context_is_image() {
                if let Some(image_uri) = hit_test.image_uri() {
                    let uri = image_uri.to_string();
                    let copy_uri = uri.clone();

                    // Open Image in New Tab
                    let open_action = gtk4::gio::SimpleAction::new("open-image-in-new-tab", None);
                    open_action.connect_activate(clone!(
                        #[weak] notebook, #[weak] window, #[weak] url_bar,
                        #[weak] back_button, #[weak] forward_button, #[weak] progress_bar, #[strong] app,
                        move |_, _| {
                            add_tab(&notebook, &window, &url_bar, &back_button,
                                    &forward_button, &progress_bar, &app, None, Some(&uri));
                        }
                    ));
                    let open_item = webkit6::ContextMenuItem::from_gaction(
                        &open_action, "Open Image in New Tab", None
                    );
                    menu.append(&open_item);

                    // Replace broken built-in Copy Image
                    let items = menu.items();
                    let copy_pos = items.iter().position(|item| {
                        item.stock_action() == webkit6::ContextMenuAction::CopyImageToClipboard
                    });
                    if let Some(p) = copy_pos {
                        menu.remove(&items[p]);
                    }
                    let copy_action = gtk4::gio::SimpleAction::new("copy-image", None);
                    copy_action.connect_activate(move |_, _| {
                        let file = gtk4::gio::File::for_uri(&copy_uri);
                        file.load_bytes_async(gtk4::gio::Cancellable::NONE, move |result| {
                            if let Ok((bytes, _)) = result {
                                if let Ok(texture) = gtk4::gdk::Texture::from_bytes(&bytes) {
                                    if let Some(display) = gtk4::gdk::Display::default() {
                                        display.clipboard().set_texture(&texture);
                                    }
                                }
                            }
                        });
                    });
                    let copy_item = webkit6::ContextMenuItem::from_gaction(
                        &copy_action, "Copy Image", None
                    );
                    match copy_pos {
                        Some(p) => menu.insert(&copy_item, p as i32),
                        None => menu.append(&copy_item),
                    }
                }
            }

            false
        }
    ));

    webview.connect_create(clone!(#[strong] app, move |webview, _| {
        let new_webview = create_browser_window(&app, Some(webview));
        new_webview.upcast::<gtk4::Widget>()
    }));

    // Tab label: favicon + truncated title + close button
    let tab_box = GtkBox::new(Orientation::Horizontal, 4);
    tab_box.set_size_request(160, -1);
    tab_box.set_margin_start(2);
    tab_box.set_margin_end(2);
    let favicon_img = Image::new();
    favicon_img.set_pixel_size(16);
    favicon_img.set_paintable(Some(&default_favicon()));
    if PRIVATE.with(|p| *p.borrow()) {
        favicon_img.add_css_class("private-favicon");
    }
    let title_label = Label::new(Some("New Tab"));
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_hexpand(true);
    let close_btn = Button::with_label("×");
    close_btn.set_has_frame(false);
    close_btn.set_margin_start(4);
    tab_box.append(&favicon_img);
    tab_box.append(&title_label);
    tab_box.append(&close_btn);

    // Right-click context menu on tab label
    let tab_menu_model = gtk4::gio::Menu::new();
    tab_menu_model.append(Some("Open in New Window"), Some("tabctx.open-new-window"));
    let mute_section = gtk4::gio::Menu::new();
    mute_section.append(Some("Mute Tab"), Some("tabctx.toggle-mute"));
    tab_menu_model.append_section(None, &mute_section);
    let close_section = gtk4::gio::Menu::new();
    close_section.append(Some("Close Tab"), Some("tabctx.close"));
    close_section.append(Some("Close Other Tabs"), Some("tabctx.close-others"));
    tab_menu_model.append_section(None, &close_section);
    let tab_popup = gtk4::PopoverMenu::from_model(Some(&tab_menu_model));
    tab_popup.set_parent(&tab_box);
    tab_popup.set_has_arrow(false);

    let tab_action_group = gtk4::gio::SimpleActionGroup::new();

    let open_new_window_action = gtk4::gio::SimpleAction::new("open-new-window", None);
    open_new_window_action.connect_activate(clone!(
        #[strong] app, #[weak] webview,
        move |_, _| {
            let uri = webview.uri().unwrap_or_default().to_string();
            let new_wv = create_browser_window(&app, None);
            if !uri.is_empty() && !uri.starts_with("rug:") {
                new_wv.load_uri(&uri);
            }
        }
    ));
    tab_action_group.add_action(&open_new_window_action);

    let close_tab_action = gtk4::gio::SimpleAction::new("close", None);
    close_tab_action.connect_activate(clone!(
        #[weak] notebook, #[weak] webview, #[weak] window,
        move |_, _| {
            if let Some(n) = notebook.page_num(&webview) {
                notebook.remove_page(Some(n));
            }
            if notebook.n_pages() == 0 {
                window.close();
            }
        }
    ));
    tab_action_group.add_action(&close_tab_action);

    let close_others_action = gtk4::gio::SimpleAction::new("close-others", None);
    close_others_action.connect_activate(clone!(
        #[weak] notebook, #[weak] webview,
        move |_, _| {
            let count = notebook.n_pages();
            let my_idx = notebook.page_num(&webview);
            for i in (0..count).rev() {
                if Some(i) != my_idx {
                    notebook.remove_page(Some(i));
                }
            }
        }
    ));
    tab_action_group.add_action(&close_others_action);

    let toggle_mute_action = gtk4::gio::SimpleAction::new("toggle-mute", None);
    toggle_mute_action.connect_activate(clone!(
        #[weak] webview,
        move |_, _| {
            webview.set_is_muted(!webview.is_muted());
        }
    ));
    tab_action_group.add_action(&toggle_mute_action);

    tab_box.insert_action_group("tabctx", Some(&tab_action_group));

    let tab_right_click = gtk4::GestureClick::new();
    tab_right_click.set_button(3);
    tab_right_click.connect_pressed(clone!(
        #[weak] tab_popup, #[weak] mute_section, #[weak] webview,
        move |gesture, _, x, y| {
            while mute_section.n_items() > 0 { mute_section.remove(0); }
            let label = if webview.is_muted() { "Unmute Tab" } else { "Mute Tab" };
            mute_section.append(Some(label), Some("tabctx.toggle-mute"));
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            tab_popup.set_pointing_to(Some(&rect));
            tab_popup.popup();
        }
    ));
    tab_box.add_controller(tab_right_click);

    let is_private = PRIVATE.with(|p| *p.borrow());

    webview.connect_notify_local(
        Some("title"),
        clone!(#[weak] title_label, #[weak] webview, move |_, _| {
            let title = webview.title().unwrap_or_default();
            let base = if title.is_empty() { "New Tab".to_string() } else { title.to_string() };
            let base = if webview.is_muted() { format!("{} (muted)", base) } else { base };
            let display = if is_private { format!("(Private) {}", base) } else { base };
            title_label.set_text(&display);
            let uri = webview.uri().unwrap_or_default();
            update_recent(&uri, &title);
        }),
    );

    webview.connect_notify_local(
        Some("is-muted"),
        clone!(#[weak] title_label, #[weak] webview, move |_, _| {
            let title = webview.title().unwrap_or_default();
            let base = if title.is_empty() { "New Tab".to_string() } else { title.to_string() };
            let base = if webview.is_muted() { format!("{} (muted)", base) } else { base };
            let display = if is_private { format!("(Private) {}", base) } else { base };
            title_label.set_text(&display);
        }),
    );

    webview.connect_load_changed(clone!(
        #[weak] favicon_img, #[weak] webview,
        move |_, load_event| {
            if load_event == webkit6::LoadEvent::Finished {
                if let Some(texture) = webview.favicon() {
                    favicon_img.set_paintable(Some(&texture));
                    if let Some(uri) = webview.uri() {
                        update_recent_favicon(&uri, &texture);
                    }
                }
            }
        }
    ));

    webview.connect_favicon_notify(clone!(#[weak] favicon_img, move |webview| {
        let favicon = webview.favicon();
        if let Some(texture) = &favicon {
            favicon_img.set_paintable(Some(texture));
            if let Some(uri) = webview.uri() {
                update_recent_favicon(&uri, texture);
            }
        } else {
            favicon_img.set_paintable(Some(&default_favicon()));
        }
    }));

    let page_idx = notebook.append_page(&webview, Some(&tab_box));
    notebook.set_tab_reorderable(&webview, true);
    notebook.set_current_page(Some(page_idx));
    notebook.page(&webview).set_tab_expand(false);
    notebook.page(&webview).set_tab_fill(true);

    close_btn.connect_clicked(clone!(#[weak] notebook, #[weak] webview, #[weak] window, move |_| {
        if let Some(n) = notebook.page_num(&webview) {
            notebook.remove_page(Some(n));
        }
        if notebook.n_pages() == 0 {
            window.close();
        }
    }));

    webview
}

fn create_browser_window(app: &Application, related_view: Option<&WebView>) -> WebView {
    let title = if PRIVATE.with(|i| *i.borrow()) { "rug — private browsing" } else { "rug" };
    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(800)
        .default_height(600)
        .title(title)
        .build();

    let container = GtkBox::new(Orientation::Vertical, 0);

    let url_bar = Entry::new();
    url_bar.set_hexpand(true);

    let completion_list = ListBox::new();
    completion_list.set_selection_mode(gtk4::SelectionMode::Single);
    completion_list.set_focusable(false);
    let completion_box = GtkBox::new(Orientation::Vertical, 0);
    completion_box.add_css_class("completion-dropdown");
    completion_box.set_visible(false);
    completion_box.set_halign(gtk4::Align::Start);
    completion_box.set_valign(gtk4::Align::Start);
    completion_box.append(&completion_list);
    let completion_css = gtk4::CssProvider::new();
    completion_css.load_from_data(
        ".completion-dropdown { background: @theme_bg_color; border: 1px solid @borders; border-radius: 0 0 4px 4px; }"
    );
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display, &completion_css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    if PRIVATE.with(|p| *p.borrow()) {
        let private_css = gtk4::CssProvider::new();
        private_css.load_from_data(".private-favicon { filter: invert(1); }");
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display, &private_css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    let bar_focused = std::rc::Rc::new(std::cell::Cell::new(false));

    let focus_ctrl = gtk4::EventControllerFocus::new();
    focus_ctrl.connect_enter(clone!(#[weak] url_bar, #[strong] bar_focused, move |_| {
        bar_focused.set(true);
        glib::idle_add_local_once(clone!(#[weak] url_bar, move || {
            url_bar.select_region(0, -1);
        }));
    }));
    focus_ctrl.connect_leave(clone!(#[weak] completion_box, #[strong] bar_focused, move |_| {
        bar_focused.set(false);
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(150),
            clone!(#[weak] completion_box, move || {
                completion_box.set_visible(false);
            }),
        );
    }));
    url_bar.add_controller(focus_ctrl);

    let url_box = GtkBox::new(Orientation::Horizontal, 5);
    url_box.set_margin_bottom(5);
    url_box.set_margin_end(5);
    url_box.set_margin_start(5);
    url_box.set_margin_top(5);

    let progress_bar = ProgressBar::new();
    progress_bar.set_hexpand(true);
    progress_bar.set_show_text(false);

    let go_button = Button::with_label("Go");
    let back_button = Button::with_label("←");
    let forward_button = Button::with_label("→");

    back_button.set_sensitive(false);
    forward_button.set_sensitive(false);

    url_bar.connect_activate(clone!(#[weak] go_button, move |_| {
        go_button.emit_clicked();
    }));

    let notebook = Notebook::new();
    notebook.set_hexpand(true);
    notebook.set_vexpand(true);
    notebook.set_scrollable(true);

    notebook.connect_switch_page(clone!(
        #[weak] url_bar, #[weak] back_button, #[weak] forward_button, #[weak] progress_bar,
        move |_, page, _| {
            if let Some(webview) = page.downcast_ref::<WebView>() {
                url_bar.set_text(&webview.uri().unwrap_or_default());
                back_button.set_sensitive(webview.can_go_back());
                forward_button.set_sensitive(webview.can_go_forward());
                let p = webview.estimated_load_progress();
                progress_bar.set_fraction(if p >= 1.0 { 0.0 } else { p });
            }
        }
    ));

    let url_key_ctrl = gtk4::EventControllerKey::new();
    url_key_ctrl.set_propagation_phase(gtk4::PropagationPhase::Capture);
    url_key_ctrl.connect_key_pressed(clone!(
        #[strong] completion_box, #[strong] completion_list, #[strong] url_bar, #[strong] notebook,
        move |_, key, _, _| {
            if !completion_box.is_visible() {
                return glib::Propagation::Proceed;
            }
            match key {
                Key::Down => {
                    match completion_list.selected_row() {
                        None => {
                            if let Some(row) = completion_list.row_at_index(0) {
                                completion_list.select_row(Some(&row));
                            }
                        }
                        Some(row) => {
                            match completion_list.row_at_index(row.index() + 1) {
                                Some(next) => completion_list.select_row(Some(&next)),
                                None => completion_list.unselect_all(),
                            }
                        }
                    }
                    glib::Propagation::Stop
                }
                Key::Up => {
                    match completion_list.selected_row() {
                        None => {
                            let mut last = 0i32;
                            while completion_list.row_at_index(last + 1).is_some() { last += 1; }
                            if let Some(row) = completion_list.row_at_index(last) {
                                completion_list.select_row(Some(&row));
                            }
                        }
                        Some(row) => {
                            let idx = row.index();
                            if idx == 0 {
                                completion_list.unselect_all();
                            } else if let Some(prev) = completion_list.row_at_index(idx - 1) {
                                completion_list.select_row(Some(&prev));
                            }
                        }
                    }
                    glib::Propagation::Stop
                }
                Key::Return | Key::KP_Enter => {
                    if let Some(row) = completion_list.selected_row() {
                        let url = row.widget_name().to_string();
                        url_bar.set_text(&url);
                        completion_box.set_visible(false);
                        completion_list.unselect_all();
                        if let Some(wv) = current_webview(&notebook) {
                            wv.load_uri(&url);
                        }
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                }
                Key::Escape => {
                    completion_box.set_visible(false);
                    completion_list.unselect_all();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        }
    ));
    url_bar.add_controller(url_key_ctrl);

    go_button.connect_clicked(clone!(#[weak] notebook, #[weak] url_bar, #[weak] completion_box, move |_| {
        completion_box.set_visible(false);
        if let Some(webview) = current_webview(&notebook) {
            webview.load_uri(&smart_uri(&url_bar.text()));
        }
    }));

    back_button.connect_clicked(clone!(#[weak] notebook, move |_| {
        if let Some(webview) = current_webview(&notebook) {
            if webview.can_go_back() { webview.go_back(); }
        }
    }));

    forward_button.connect_clicked(clone!(#[weak] notebook, move |_| {
        if let Some(webview) = current_webview(&notebook) {
            if webview.can_go_forward() { webview.go_forward(); }
        }
    }));

    url_bar.connect_changed(clone!(
        #[weak] completion_box, #[weak] completion_list,
        #[weak] url_bar, #[weak] notebook, #[weak] container, #[strong] bar_focused,
        move |_| {
            while let Some(child) = completion_list.first_child() {
                completion_list.remove(&child);
            }
            if !bar_focused.get() {
                completion_box.set_visible(false);
                return;
            }
            let text = url_bar.text().to_string();
            if text.is_empty() {
                completion_box.set_visible(false);
                return;
            }
            let results = search_history(&text, 8);
            if results.is_empty() {
                completion_box.set_visible(false);
                return;
            }
            for (url, title, _) in results {
                let row = gtk4::ListBoxRow::new();
                row.set_focusable(false);
                let row_box = GtkBox::new(Orientation::Horizontal, 8);
                row_box.set_margin_start(8);
                row_box.set_margin_end(8);
                row_box.set_margin_top(4);
                row_box.set_margin_bottom(4);
                let fav_img = gtk4::Image::new();
                fav_img.set_pixel_size(16);
                fav_img.set_valign(gtk4::Align::Center);
                FAVICON_CACHE.with(|fc| {
                    match fc.borrow().get(&url) {
                        Some(t) => fav_img.set_paintable(Some(t)),
                        None => fav_img.set_paintable(Some(&default_favicon())),
                    }
                });
                row_box.append(&fav_img);
                let text_box = GtkBox::new(Orientation::Vertical, 2);
                text_box.set_hexpand(true);
                let display_title = if title.is_empty() { url.clone() } else { title.clone() };
                let title_lbl = Label::new(None);
                title_lbl.set_markup(&highlight_match(&display_title, &text));
                title_lbl.set_halign(gtk4::Align::Start);
                title_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                let url_lbl = Label::new(None);
                url_lbl.set_markup(&format!(
                    "<small><span foreground='#888888'>{}</span></small>",
                    highlight_match(&url, &text)
                ));
                url_lbl.set_halign(gtk4::Align::Start);
                url_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                text_box.append(&title_lbl);
                text_box.append(&url_lbl);
                row_box.append(&text_box);
                row.set_child(Some(&row_box));
                row.set_widget_name(&url);
                let click = gtk4::GestureClick::new();
                let nav_url = url.clone();
                click.connect_pressed(clone!(
                    #[weak] notebook, #[weak] url_bar, #[weak] completion_box,
                    move |_, _, _, _| {
                        url_bar.set_text(&nav_url);
                        if let Some(wv) = current_webview(&notebook) {
                            wv.load_uri(&nav_url);
                        }
                        completion_box.set_visible(false);
                    }
                ));
                row.add_controller(click);
                completion_list.append(&row);
            }
            if let Some(bounds) = url_bar.compute_bounds(&container) {
                let x = bounds.x() as i32;
                let y = (bounds.y() + bounds.height()) as i32;
                let w = bounds.width() as i32;
                completion_box.set_margin_start(x);
                completion_box.set_margin_top(y);
                completion_list.set_size_request(w, -1);
            }
            completion_box.set_visible(true);
        }
    ));

    // "+" button in tab bar for new blank tabs
    let new_tab_btn = Button::with_label("+");
    new_tab_btn.set_has_frame(false);
    new_tab_btn.connect_clicked(clone!(
        #[weak] notebook, #[weak] window, #[weak] url_bar,
        #[weak] back_button, #[weak] forward_button, #[weak] progress_bar, #[strong] app,
        move |_| {
            add_tab(&notebook, &window, &url_bar, &back_button, &forward_button, &progress_bar, &app, None, None);
        }
    ));
    notebook.set_action_widget(&new_tab_btn, gtk4::PackType::End);

    let webview = add_tab(&notebook, &window, &url_bar, &back_button, &forward_button, &progress_bar, app, related_view, None);

    let ev_ctrl = gtk4::EventControllerKey::new();
    ev_ctrl.connect_key_pressed(clone!(#[strong] notebook, move |_, key, _, _| {
        match key {
            Key::Escape => { std::process::exit(0); }
            Key::F5 => {
                if let Some(wv) = current_webview(&notebook) { wv.reload(); }
            }
            _ => (),
        }
        glib::Propagation::Proceed
    }));

    url_box.append(&back_button);
    url_box.append(&forward_button);
    url_box.append(&url_bar);
    url_box.append(&go_button);

    container.append(&url_box);
    container.append(&progress_bar);
    container.append(&notebook);
    container.set_hexpand(true);
    container.set_vexpand(true);

    let window_overlay = Overlay::new();
    window_overlay.set_child(Some(&container));
    window_overlay.add_overlay(&completion_box);
    window.set_child(Some(&window_overlay));
    window.add_controller(ev_ctrl);
    window.present();

    webview
}

fn main() {
    if std::env::args().any(|a| a == "--private") {
        PRIVATE.with(|i| *i.borrow_mut() = true);
    }

    let app = Application::builder()
        .application_id("com.computermouth.rug")
        .flags(gtk4::gio::ApplicationFlags::NON_UNIQUE)
        .build();

    app.connect_activate(|app| {
        let incognito = PRIVATE.with(|i| *i.borrow());
        if !incognito { load_recent(); }
        load_settings();
        if incognito {
            apply_dark_mode(true);
        } else {
            apply_dark_mode(DARK_MODE.with(|d| *d.borrow()));
        }

        #[cfg(debug_assertions)]
        let (data_dir, cache_dir) = (
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/wk_data"),
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/tmp/wk_cache"),
        );
        #[cfg(not(debug_assertions))]
        let (data_dir, cache_dir) = (
            glib::home_dir().join(".local/share/rug"),
            glib::home_dir().join(".cache/rug"),
        );
        let ns = if incognito {
            webkit6::NetworkSession::new_ephemeral()
        } else {
            webkit6::NetworkSession::new(
                Some(&data_dir.to_string_lossy()),
                Some(&cache_dir.to_string_lossy()),
            )
        };
        if let Some(wdm) = ns.website_data_manager() {
            wdm.set_favicons_enabled(true);
        }
        if !incognito {
            if let Some(cm) = ns.cookie_manager() {
                let cookie_file = data_dir.join("cookies.sqlite");
                cm.set_persistent_storage(
                    &cookie_file.to_string_lossy(),
                    webkit6::CookiePersistentStorage::Sqlite,
                );
            }
        }
        NETWORK_SESSION.with(|s| *s.borrow_mut() = Some(ns));

        let webview = create_browser_window(app, None);

        webview.web_context().unwrap().register_uri_scheme("rug", |request| {
            let html = match request.uri().unwrap_or_default().as_str() {
                "rug://home" => {
                    let p = recent_pages_snapshot();
                    let top: Vec<(String, String)> = p.iter().take(8).map(|(u, t, _)| (u.clone(), t.clone())).collect();
                    html::home(&top, &top_domains(16), DARK_MODE.with(|d| *d.borrow()), PRIVATE.with(|i| *i.borrow()))
                }
                s if s.starts_with("rug://settings") => {
                    let params = parse_query_params(s);
                    let cleared = params.get("clear").map(|v| v == "1").unwrap_or(false);
                    if cleared { clear_history(); }
                    let cache_cleared = params.get("clear_cache").map(|v| v == "1").unwrap_or(false);
                    if cache_cleared { clear_cache(); }
                    let cookies_cleared = params.get("clear_cookies").map(|v| v == "1").unwrap_or(false);
                    if cookies_cleared { clear_cookies(); }
                    if let Some(engine) = params.get("engine") {
                        let valid = ["ddg", "google", "bing", "custom"];
                        if valid.contains(&engine.as_str()) {
                            SEARCH_ENGINE.with(|e| *e.borrow_mut() = engine.clone());
                            let custom = params.get("custom_url").cloned().unwrap_or_default();
                            CUSTOM_SEARCH_URL.with(|u| *u.borrow_mut() = custom);
                            let dark = params.get("theme").map(|t| t == "dark").unwrap_or(false);
                            DARK_MODE.with(|d| *d.borrow_mut() = dark);
                            apply_dark_mode(dark);
                            let cache = params.get("cache").map(|v| v == "enabled").unwrap_or(true);
                            CACHE_ENABLED.with(|c| *c.borrow_mut() = cache);
                            save_settings();
                        }
                    }
                    let engine = SEARCH_ENGINE.with(|e| e.borrow().clone());
                    let custom_url = CUSTOM_SEARCH_URL.with(|u| u.borrow().clone());
                    let dark = DARK_MODE.with(|d| *d.borrow());
                    let cache_enabled = CACHE_ENABLED.with(|c| *c.borrow());
                    html::settings(cleared, &engine, &custom_url, dark, cache_enabled, cache_cleared, cookies_cleared)
                }
                "rug://private" => {
                    if let Ok(exe) = std::env::current_exe() {
                        std::process::Command::new(exe).arg("--private").spawn().ok();
                    }
                    String::from("<!DOCTYPE html><html><head><meta http-equiv=\"refresh\" content=\"0;url=rug://home\"></head><body></body></html>")
                }
                _ => String::from("<!DOCTYPE html><html><body>Not found</body></html>"),
            };
            let bytes = glib::Bytes::from(html.as_bytes());
            let stream = gtk4::gio::MemoryInputStream::from_bytes(&bytes);
            request.finish(&stream, bytes.len() as i64, Some("text/html"));
        });

        if !incognito {
            let cache_model = if CACHE_ENABLED.with(|c| *c.borrow()) {
                webkit6::CacheModel::WebBrowser
            } else {
                webkit6::CacheModel::DocumentViewer
            };
            if let Some(ctx) = webview.web_context() {
                ctx.set_cache_model(cache_model);
            }
        }

        let session = webview.network_session().unwrap();
        session.connect_download_started(clone!(#[strong] app, move |_, download| {
            download.connect_decide_destination(clone!(#[strong] app, move |download, suggested_filename| {
                let download = download.clone();
                let dialog = gtk4::FileDialog::new();
                dialog.set_initial_name(Some(suggested_filename));
                let downloads_folder = gtk4::gio::File::for_path(glib::home_dir().join("Downloads"));
                dialog.set_initial_folder(Some(&downloads_folder));
                let window = app.active_window();
                dialog.save(window.as_ref(), gtk4::gio::Cancellable::NONE, move |result| {
                    match result {
                        Ok(file) => {
                            if let Some(path) = file.path() {
                                download.set_destination(&path.to_string_lossy());
                            } else {
                                download.cancel();
                            }
                        }
                        Err(_) => { download.cancel(); }
                    }
                });
                true
            }));
        }));
    });

    let argv0 = std::env::args().next().unwrap_or_default();
    app.run_with_args(&[argv0]);
}
