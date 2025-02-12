use gtk4::prelude::*;
use gtk4::gdk::Key;
use webkit6::prelude::*;
use webkit6::WebView;
use gtk4::{Application, ApplicationWindow, Box as GtkBox, Orientation, Entry, Button, ProgressBar};
use glib::clone; // Keep this import

mod html;

fn main() {
    let app = Application::builder()
        .application_id("com.computermouth.rug")
        .build();

    app.connect_activate(|app| {
        let window = ApplicationWindow::builder()
            .application(app)
            .default_width(800)
            .default_height(600)
            .title("rug")
            .build();

        let container = GtkBox::new(Orientation::Vertical, 0);

        let webview = WebView::new();
        webview.load_html(html::HOME, None);

        let url_bar = Entry::new();
        // url_bar.set_placeholder_text(Some("Enter URL..."));
        url_bar.set_hexpand(true);
        
        let url_box = GtkBox::new(Orientation::Horizontal, 5);
        url_box.set_margin_bottom(5);
        url_box.set_margin_end(5);
        url_box.set_margin_start(5);
        url_box.set_margin_top(5);

        // Inside `app.connect_activate(|app| {`
        let progress_bar = ProgressBar::new();
        progress_bar.set_hexpand(true);
        progress_bar.set_show_text(false);
        // progress_bar.set_margin_top(5);
        
        let go_button = Button::with_label("Go");
        let back_button = Button::with_label("←");
        let forward_button = Button::with_label("→");

        go_button.set_action_name(Some("GO_BUTTON"));
        back_button.set_sensitive(false);
        forward_button.set_sensitive(false);

        url_bar.connect_activate(clone!(#[weak] go_button, move |_| {
            go_button.emit_clicked();
        }));

        go_button.connect_clicked(clone!(#[weak] webview, #[weak] url_bar, move |_| {

            let raw = url_bar.text();

            if !raw.contains("://") {
                let with_http = format!("http://{}", raw);
                webview.load_uri(&with_http);
            } else {
                webview.load_uri(&raw);
            }

        }));

        back_button.connect_clicked(clone!(#[weak] webview, move |_| {
            if webview.can_go_back() {
                webview.go_back();
            }
        }));
        forward_button.connect_clicked(clone!(#[weak] webview, move |_| {
            if webview.can_go_forward() {
                webview.go_forward();
            }
        }));

        webview.connect_load_changed(clone!(#[weak] back_button, #[weak] forward_button, #[weak] webview, #[weak] url_bar, move |_, _| {
            back_button.set_sensitive(webview.can_go_back());
            forward_button.set_sensitive(webview.can_go_forward());
            url_bar.set_text(&webview.uri().unwrap_or_default());
        }));

        webview.connect_notify_local(
            Some("estimated-load-progress"),
            clone!(#[weak] progress_bar, #[weak] webview, move |_, _| {
                let progress = webview.estimated_load_progress();
                progress_bar.set_fraction(progress);
                progress_bar.set_sensitive(progress < 1.0);
            }),
        );

        let ev_ctrl = gtk4::EventControllerKey::new();
        ev_ctrl.connect_key_pressed(|_, key, _, _| {
            match key {
                Key::Escape => {
                    std::process::exit(0);
                }
                _ => (),
            }
            glib::Propagation::Proceed
        });

        // let ev_ctrl = gtk4::EventControllerKey::new();
        // ev_ctrl.connect_key_pressed(clone!(#[weak] go_button, move |a,b,c,d| {
        //     glib::Propagation::Stop
        // }));
        url_box.append(&back_button);
        url_box.append(&forward_button);
        url_box.append(&url_bar);
        url_box.append(&go_button);

        container.append(&url_box);
        container.append(&progress_bar);

        // Enable WebView to expand with the window
        webview.set_hexpand(true);
        webview.set_vexpand(true);

        // Ensure the container also expands
        container.set_hexpand(true);
        container.set_vexpand(true);

        container.append(&webview);
        window.set_child(Some(&container));
        window.add_controller(ev_ctrl);
        window.show();
    });

    app.run();
}
