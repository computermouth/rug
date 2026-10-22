mod completion;
mod pages;
mod paths;
mod store;
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
    let private = std::env::args().any(|a| a == "--private");

    // Keep our flags away from GApplication's option parsing.
    let argv0 = std::env::args().next().unwrap_or_default();
    let app = RelmApp::new("com.computermouth.rug").with_args(vec![argv0]);
    app.allow_multiple_instances(true);

    store::init(private);
    apply_theme(private || store::settings().dark);
    relm4::set_global_css(include_str!("../assets/gtk.css"));
    web::init();

    app.run::<BrowserWindow>(Page::Home);
}
