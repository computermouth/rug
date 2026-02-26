use gtk4::prelude::*;
use gtk4::gdk::Key;
use webkit6::prelude::*;
use webkit6::WebView;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Image, Label, Notebook, Orientation, Entry, Button, ProgressBar};
use glib::clone;
use std::cell::RefCell;

mod html;

thread_local! {
    static RECENT_PAGES: RefCell<Vec<(String, String)>> = RefCell::new(Vec::new());
}

fn update_recent(url: &str, title: &str) {
    if url.is_empty() || url.starts_with("about:") || url.starts_with("rug:") { return; }
    RECENT_PAGES.with(|rp| {
        let mut pages = rp.borrow_mut();
        if let Some(pos) = pages.iter().position(|(u, _)| u == url) {
            let (u, old_title) = pages.remove(pos);
            let t = if title.is_empty() { old_title } else { title.to_string() };
            pages.insert(0, (u, t));
        } else {
            let t = if title.is_empty() { url.to_string() } else { title.to_string() };
            pages.insert(0, (url.to_string(), t));
        }
        pages.truncate(8);
    });
    save_recent();
}

fn recent_pages_snapshot() -> Vec<(String, String)> {
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

fn load_recent() {
    let path = data_path();
    if let Ok(content) = std::fs::read_to_string(path) {
        if let Ok(pages) = serde_json::from_str::<Vec<(String, String)>>(&content) {
            RECENT_PAGES.with(|rp| *rp.borrow_mut() = pages);
        }
    }
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
    let title_label = Label::new(Some("New Tab"));
    title_label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    title_label.set_hexpand(true);
    let close_btn = Button::with_label("×");
    close_btn.set_has_frame(false);
    close_btn.set_margin_start(4);
    tab_box.append(&favicon_img);
    tab_box.append(&title_label);
    tab_box.append(&close_btn);

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
        favicon_img.set_paintable(webview.favicon().as_ref());
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

    let focus_ctrl = gtk4::EventControllerFocus::new();
    focus_ctrl.connect_enter(clone!(#[weak] url_bar, move |_| {
        glib::idle_add_local_once(clone!(#[weak] url_bar, move || {
            url_bar.select_region(0, -1);
        }));
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

    go_button.connect_clicked(clone!(#[weak] notebook, #[weak] url_bar, move |_| {
        if let Some(webview) = current_webview(&notebook) {
            let raw = url_bar.text();
            if !raw.contains("://") {
                webview.load_uri(&format!("http://{}", raw));
            } else {
                webview.load_uri(&raw);
            }
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
                "rug://home" => html::home(&recent_pages_snapshot()),
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
