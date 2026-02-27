use gtk4::prelude::*;
use gtk4::gdk::Key;
use webkit6::prelude::*;
use webkit6::WebView;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Image, Label, ListBox, Notebook, Orientation, Entry, Button, Popover, ProgressBar, ScrolledWindow};
use glib::clone;
use std::cell::RefCell;

mod html;

thread_local! {
    static RECENT_PAGES: RefCell<Vec<(String, String, Option<String>)>> = RefCell::new(Vec::new());
    static FAVICON_CACHE: RefCell<std::collections::HashMap<String, gtk4::gdk::Texture>> =
        RefCell::new(std::collections::HashMap::new());
}

fn update_recent(url: &str, title: &str) {
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

fn data_path() -> std::path::PathBuf {
    #[cfg(debug_assertions)]
    { std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target").join("rug_data.json") }
    #[cfg(not(debug_assertions))]
    { glib::home_dir().join(".local").join("share").join("rug").join("data.json") }
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
        return format!("https://duckduckgo.com/?q={}", url_encode(s));
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
        None => WebView::new(),
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
    let close_section = gtk4::gio::Menu::new();
    close_section.append(Some("Close"), Some("tabctx.close"));
    close_section.append(Some("Close Others"), Some("tabctx.close-others"));
    close_section.append(Some("Close All"), Some("tabctx.close-all"));
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

    let close_all_action = gtk4::gio::SimpleAction::new("close-all", None);
    close_all_action.connect_activate(clone!(
        #[weak] window,
        move |_, _| {
            window.close();
        }
    ));
    tab_action_group.add_action(&close_all_action);

    tab_box.insert_action_group("tabctx", Some(&tab_action_group));

    let tab_right_click = gtk4::GestureClick::new();
    tab_right_click.set_button(3);
    tab_right_click.connect_pressed(clone!(
        #[weak] tab_popup,
        move |gesture, _, x, y| {
            gesture.set_state(gtk4::EventSequenceState::Claimed);
            let rect = gtk4::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
            tab_popup.set_pointing_to(Some(&rect));
            tab_popup.popup();
        }
    ));
    tab_box.add_controller(tab_right_click);

    webview.connect_notify_local(
        Some("title"),
        clone!(#[weak] title_label, #[weak] webview, move |_, _| {
            let title = webview.title().unwrap_or_default();
            title_label.set_text(if title.is_empty() { "New Tab" } else { &title });
            let uri = webview.uri().unwrap_or_default();
            update_recent(&uri, &title);
        }),
    );

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
    let window = ApplicationWindow::builder()
        .application(app)
        .default_width(800)
        .default_height(600)
        .title("rug")
        .build();

    let container = GtkBox::new(Orientation::Vertical, 0);

    let url_bar = Entry::new();
    url_bar.set_hexpand(true);

    let completion_popover = Popover::new();
    completion_popover.set_parent(&url_bar);
    completion_popover.set_has_arrow(false);
    completion_popover.set_autohide(false);
    completion_popover.set_position(gtk4::PositionType::Bottom);
    let completion_list = ListBox::new();
    completion_list.set_selection_mode(gtk4::SelectionMode::Single);
    let completion_scroll = ScrolledWindow::new();
    completion_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Never);
    completion_scroll.set_child(Some(&completion_list));
    completion_popover.set_child(Some(&completion_scroll));
    completion_popover.add_css_class("url-completion");
    // Zero out popover padding so surface width == content width exactly
    let popover_css = gtk4::CssProvider::new();
    popover_css.load_from_data("popover.url-completion > contents { padding: 0; }");
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display, &popover_css, gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let bar_focused = std::rc::Rc::new(std::cell::Cell::new(false));

    let focus_ctrl = gtk4::EventControllerFocus::new();
    focus_ctrl.connect_enter(clone!(#[weak] url_bar, #[strong] bar_focused, move |_| {
        bar_focused.set(true);
        glib::idle_add_local_once(clone!(#[weak] url_bar, move || {
            url_bar.select_region(0, -1);
        }));
    }));
    focus_ctrl.connect_leave(clone!(#[weak] completion_popover, #[strong] bar_focused, move |_| {
        bar_focused.set(false);
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(150),
            clone!(#[weak] completion_popover, move || {
                completion_popover.popdown();
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
        #[strong] completion_popover, #[strong] completion_list, #[strong] url_bar, #[strong] notebook,
        move |_, key, _, _| {
            if !completion_popover.is_visible() {
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
                        completion_popover.popdown();
                        completion_list.unselect_all();
                        if let Some(wv) = current_webview(&notebook) {
                            wv.load_uri(&url);
                        }
                        return glib::Propagation::Stop;
                    }
                    glib::Propagation::Proceed
                }
                Key::Escape => {
                    completion_popover.popdown();
                    completion_list.unselect_all();
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        }
    ));
    url_bar.add_controller(url_key_ctrl);

    go_button.connect_clicked(clone!(#[weak] notebook, #[weak] url_bar, #[weak] completion_popover, move |_| {
        completion_popover.popdown();
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
        #[weak] completion_popover, #[weak] completion_list, #[weak] completion_scroll,
        #[weak] url_bar, #[weak] notebook, #[strong] bar_focused,
        move |_| {
            while let Some(child) = completion_list.first_child() {
                completion_list.remove(&child);
            }
            if !bar_focused.get() {
                completion_popover.popdown();
                return;
            }
            let text = url_bar.text().to_string();
            if text.is_empty() {
                completion_popover.popdown();
                return;
            }
            let results = search_history(&text, 8);
            if results.is_empty() {
                completion_popover.popdown();
                return;
            }
            let w = url_bar.width();
            completion_scroll.set_min_content_width(w);
            completion_scroll.set_max_content_width(w);
            for (url, title, _) in results {
                let row = gtk4::ListBoxRow::new();
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
                let title_lbl = Label::new(Some(&display_title));
                title_lbl.set_halign(gtk4::Align::Start);
                title_lbl.set_ellipsize(gtk4::pango::EllipsizeMode::End);
                let url_lbl = Label::new(None);
                url_lbl.set_markup(&format!(
                    "<small><span foreground='#888888'>{}</span></small>",
                    pango_esc(&url)
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
                    #[weak] notebook, #[weak] url_bar, #[weak] completion_popover,
                    move |_, _, _, _| {
                        url_bar.set_text(&nav_url);
                        if let Some(wv) = current_webview(&notebook) {
                            wv.load_uri(&nav_url);
                        }
                        completion_popover.popdown();
                    }
                ));
                row.add_controller(click);
                completion_list.append(&row);
            }
            let (_, nat_h, _, _) = completion_list.measure(gtk4::Orientation::Vertical, -1);
            completion_scroll.set_max_content_height(-1);
            completion_scroll.set_min_content_height(nat_h);
            completion_scroll.set_max_content_height(nat_h);
            completion_popover.set_size_request(w, -1);
            completion_popover.queue_resize();
            if !completion_popover.is_visible() {
                completion_popover.popup();
            }
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

    window.set_child(Some(&container));
    window.add_controller(ev_ctrl);
    window.present();

    webview
}

fn main() {
    let app = Application::builder()
        .application_id("com.computermouth.rug")
        .build();

    app.connect_activate(|app| {
        load_recent();
        let webview = create_browser_window(app, None);

        webview.web_context().unwrap().register_uri_scheme("rug", |request| {
            let html = match request.uri().unwrap_or_default().as_str() {
                "rug://home" => {
                    let p = recent_pages_snapshot();
                    let top: Vec<(String, String)> = p.iter().take(8).map(|(u, t, _)| (u.clone(), t.clone())).collect();
                    html::home(&top, &top_domains(12))
                }
                s if s.starts_with("rug://settings") => {
                    let cleared = s.contains("?clear=1");
                    if cleared { clear_history(); }
                    html::settings(cleared)
                }
                _ => String::from("<!DOCTYPE html><html><body>Not found</body></html>"),
            };
            let bytes = glib::Bytes::from(html.as_bytes());
            let stream = gtk4::gio::MemoryInputStream::from_bytes(&bytes);
            request.finish(&stream, bytes.len() as i64, Some("text/html"));
        });

        let session = webview.network_session().unwrap();
        session.website_data_manager().unwrap().set_favicons_enabled(true);

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

    app.run();
}
