mod bookmark_dialog;
mod bookmarks;
mod completion;
mod pages;
mod passwords;
mod paths;
mod store;
mod sync;
mod tab;
mod urls;
mod web;
mod window;

use relm4::RelmApp;

use crate::web::Page;
use crate::window::BrowserWindow;

pub fn apply_theme(dark: bool) {
    if let Some(settings) = gtk::Settings::default() {
        settings.set_gtk_application_prefer_dark_theme(dark);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).is_some_and(|a| a == "--merge-driver") {
        std::process::exit(sync::merge_driver(&args[2..]));
    }
    let private = std::env::args().any(|a| a == "--private");

    // Keep our flags away from GApplication's option parsing.
    let argv0 = std::env::args().next().unwrap_or_default();
    let app = RelmApp::new("com.computermouth.rug").with_args(vec![argv0]);
    app.allow_multiple_instances(true);

    paths::migrate();
    sync::init();
    store::init(private);
    bookmarks::init();
    apply_theme(private || store::settings().dark);
    relm4::set_global_css(include_str!("../assets/gtk.css"));
    web::init();
    if !private {
        sync::start();
    }

    // `rug [--private] [URL or search]`
    let start = match args.iter().skip(1).find(|a| !a.starts_with("--")) {
        Some(input) => Page::Uri(urls::resolve(input, &store::settings())),
        None => Page::Home,
    };
    app.run::<BrowserWindow>(start);
    if private {
        sync::commit();
    } else {
        sync::sync_on_exit();
    }
}
