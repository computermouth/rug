//! The WebKit network session and web views.

use std::cell::OnceCell;

use gtk::prelude::*;
use gtk::{gio, glib};
use webkit6::prelude::*;
use webkit6::{CacheModel, CookiePersistentStorage, Download, NetworkSession, WebContext, WebView};

use crate::{pages, paths, store};

thread_local! {
    static SESSION: OnceCell<NetworkSession> = const { OnceCell::new() };
}

/// What a new tab starts out showing.
#[derive(Debug)]
pub enum Page {
    Home,
    Uri(String),
    /// A view WebKit already created, e.g. for a page's `window.open`.
    Adopt(WebView),
}

pub fn init() {
    let private = store::is_private();

    let session = if private {
        NetworkSession::new_ephemeral()
    } else {
        let session = NetworkSession::new(
            Some(&paths::webkit_data_dir().to_string_lossy()),
            Some(&paths::webkit_cache_dir().to_string_lossy()),
        );
        if let Some(cookies) = session.cookie_manager() {
            cookies.set_persistent_storage(&paths::cookies_file().to_string_lossy(), CookiePersistentStorage::Sqlite);
        }
        session
    };
    if let Some(data) = session.website_data_manager() {
        data.set_favicons_enabled(true);
    }
    session.connect_download_started(|_, download| {
        download.connect_decide_destination(choose_download_destination);
    });
    SESSION.with(|s| s.set(session).expect("web::init called twice"));

    let context = WebContext::default().expect("no default WebKit context");
    pages::register(&context);
    if !private {
        let model = if store::settings().cache { CacheModel::WebBrowser } else { CacheModel::DocumentViewer };
        context.set_cache_model(model);
    }
}

pub fn webview(page: Page) -> WebView {
    let uri = match page {
        Page::Adopt(webview) => return webview,
        Page::Home => pages::HOME.to_string(),
        Page::Uri(uri) => uri,
    };
    let webview = SESSION.with(|s| {
        WebView::builder().network_session(s.get().expect("web::init not called")).build()
    });
    webview.load_uri(&uri);
    webview
}

/// A view sharing `view`'s session and process, as WebKit wants for popups.
pub fn related(view: &WebView) -> WebView {
    WebView::builder().related_view(view).build()
}

fn choose_download_destination(download: &Download, suggested_name: &str) -> bool {
    let dialog = gtk::FileDialog::new();
    dialog.set_initial_name(Some(suggested_name));
    dialog.set_initial_folder(Some(&gio::File::for_path(glib::home_dir().join("Downloads"))));
    let window = relm4::main_application().active_window();
    let download = download.clone();
    dialog.save(window.as_ref(), gio::Cancellable::NONE, move |result| {
        match result.ok().and_then(|file| file.path()) {
            Some(path) => download.set_destination(&path.to_string_lossy()),
            None => download.cancel(),
        }
    });
    true
}
