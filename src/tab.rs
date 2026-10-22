//! One browser tab: a web view plus the label shown in the notebook.

use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use relm4::actions::{RelmAction, RelmActionGroup};
use relm4::prelude::*;
use webkit6::prelude::*;
use webkit6::{ContextMenu, ContextMenuAction, ContextMenuItem, HitTestResult, LoadEvent, WebView};

use crate::bookmark_dialog::BookmarkDialog;
use crate::store;
use crate::web::{self, Page};
use crate::window::BrowserWindow;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TabId(pub usize);

pub struct Tab {
    id: TabId,
    webview: WebView,
    title: String,
    muted: bool,
    favicon: gdk::Texture,
    menu: gtk::PopoverMenu,
}

#[derive(Debug)]
pub enum TabMsg {
    LoadChanged(LoadEvent),
    TitleChanged,
    MutedChanged,
    FaviconChanged,
}

#[derive(Debug)]
pub enum TabOutput {
    LoadChanged(TabId, LoadEvent),
    Progress(TabId),
    OpenInNewTab(String),
    OpenInNewWindow(Page),
    Close(TabId),
    CloseOthers(TabId),
}

relm4::new_action_group!(TabActions, "tab");
relm4::new_stateless_action!(OpenInWindowAction, TabActions, "open-in-window");
relm4::new_stateless_action!(BookmarkAction, TabActions, "bookmark");
relm4::new_stateful_action!(MuteAction, TabActions, "mute", (), bool);
relm4::new_stateless_action!(CloseAction, TabActions, "close");
relm4::new_stateless_action!(CloseOthersAction, TabActions, "close-others");

impl Tab {
    fn label(&self) -> String {
        let mut label = if self.title.is_empty() { "New Tab".to_string() } else { self.title.clone() };
        if self.muted {
            label.push_str(" (muted)");
        }
        if store::is_private() {
            label.insert_str(0, "(Private) ");
        }
        label
    }

    fn set_favicon(&mut self, texture: gdk::Texture) {
        if let Some(uri) = self.webview.uri() {
            store::record_favicon(&uri, &texture);
        }
        self.favicon = texture;
    }
}

#[relm4::component(pub)]
impl SimpleComponent for Tab {
    type Init = (TabId, WebView);
    type Input = TabMsg;
    type Output = TabOutput;

    view! {
        #[root]
        gtk::Box {
            #[local_ref]
            webview -> WebView {
                set_hexpand: true,
                set_vexpand: true,
            },
        },

        #[name(label)]
        gtk::Box {
            set_spacing: 4,
            set_width_request: 160,
            set_margin_start: 2,
            set_margin_end: 2,

            add_controller = gtk::GestureClick {
                set_button: gdk::BUTTON_SECONDARY,
                connect_pressed[menu = model.menu.clone()] => move |gesture, _, x, y| {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    menu.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                    menu.popup();
                },
            },

            gtk::Image {
                set_pixel_size: 16,
                set_css_classes: if store::is_private() { &["private-favicon"] } else { &[] },
                #[watch]
                set_paintable: Some(&model.favicon),
            },

            gtk::Label {
                set_hexpand: true,
                set_ellipsize: gtk::pango::EllipsizeMode::End,
                #[watch]
                set_label: &model.label(),
            },

            gtk::Button {
                set_label: "×",
                set_has_frame: false,
                set_margin_start: 4,
                connect_clicked[sender, id = model.id] => move |_| {
                    let _ = sender.output(TabOutput::Close(id));
                },
            },
        },
    }

    fn init((id, webview): Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        relm4::menu! {
            tab_menu: {
                "Open in New Window" => OpenInWindowAction,
                "Bookmark Tab…" => BookmarkAction,
                section! {
                    "Mute Tab" => MuteAction,
                },
                section! {
                    "Close Tab" => CloseAction,
                    "Close Other Tabs" => CloseOthersAction,
                },
            }
        }
        let menu = gtk::PopoverMenu::from_model(Some(&tab_menu));
        menu.set_has_arrow(false);

        let model = Tab {
            id,
            webview,
            title: String::new(),
            muted: false,
            favicon: store::placeholder_favicon(),
            menu,
        };
        let webview = &model.webview;
        let widgets = view_output!();
        model.menu.set_parent(&widgets.label);

        let mut actions = RelmActionGroup::<TabActions>::new();
        actions.add_action(RelmAction::<OpenInWindowAction>::new_stateless(glib::clone!(
            #[strong] sender, #[weak] webview,
            move |_| {
                let page = match webview.uri() {
                    Some(uri) if !uri.is_empty() && !uri.starts_with("rug:") => Page::Uri(uri.into()),
                    _ => Page::Home,
                };
                let _ = sender.output(TabOutput::OpenInNewWindow(page));
            }
        )));
        actions.add_action(RelmAction::<BookmarkAction>::new_stateless(glib::clone!(
            #[weak] webview, #[weak(rename_to = label)] widgets.label,
            move |_| {
                let title = webview.title().unwrap_or_default().to_string();
                let uri = webview.uri().unwrap_or_default().to_string();
                BookmarkDialog::builder().transient_for(&label).launch((title, uri)).detach_runtime();
            }
        )));
        actions.add_action(RelmAction::<MuteAction>::new_stateful(&false, glib::clone!(
            #[weak] webview,
            move |_, muted: &mut bool| {
                *muted = !*muted;
                webview.set_is_muted(*muted);
            }
        )));
        actions.add_action(RelmAction::<CloseAction>::new_stateless(glib::clone!(
            #[strong] sender,
            move |_| { let _ = sender.output(TabOutput::Close(id)); }
        )));
        actions.add_action(RelmAction::<CloseOthersAction>::new_stateless(glib::clone!(
            #[strong] sender,
            move |_| { let _ = sender.output(TabOutput::CloseOthers(id)); }
        )));
        actions.register_for_widget(&widgets.label);

        webview.connect_load_changed(glib::clone!(
            #[strong] sender,
            move |_, event| sender.input(TabMsg::LoadChanged(event))
        ));
        webview.connect_estimated_load_progress_notify(glib::clone!(
            #[strong] sender,
            move |_| { let _ = sender.output(TabOutput::Progress(id)); }
        ));
        webview.connect_title_notify(glib::clone!(
            #[strong] sender,
            move |_| sender.input(TabMsg::TitleChanged)
        ));
        webview.connect_is_muted_notify(glib::clone!(
            #[strong] sender,
            move |_| sender.input(TabMsg::MutedChanged)
        ));
        webview.connect_favicon_notify(glib::clone!(
            #[strong] sender,
            move |_| sender.input(TabMsg::FaviconChanged)
        ));
        webview.connect_context_menu(glib::clone!(
            #[strong] sender,
            move |_, menu, hit| {
                extend_context_menu(menu, hit, &sender);
                false
            }
        ));
        webview.connect_create(|webview, _| {
            let popup = web::related(webview);
            BrowserWindow::open(Page::Adopt(popup.clone()));
            Some(popup.upcast())
        });

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>) {
        match msg {
            TabMsg::LoadChanged(event) => {
                if event == LoadEvent::Finished {
                    let uri = self.webview.uri().unwrap_or_default();
                    store::record_visit(&uri, &self.webview.title().unwrap_or_default());
                    if let Some(texture) = self.webview.favicon() {
                        self.set_favicon(texture);
                    }
                }
                let _ = sender.output(TabOutput::LoadChanged(self.id, event));
            }
            TabMsg::TitleChanged => {
                self.title = self.webview.title().unwrap_or_default().into();
                store::record_visit(&self.webview.uri().unwrap_or_default(), &self.title);
            }
            TabMsg::MutedChanged => self.muted = self.webview.is_muted(),
            TabMsg::FaviconChanged => match self.webview.favicon() {
                Some(texture) => self.set_favicon(texture),
                None => self.favicon = store::placeholder_favicon(),
            },
        }
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.menu.unparent();
    }
}

fn find_item(menu: &ContextMenu, action: ContextMenuAction) -> Option<(i32, ContextMenuItem)> {
    menu.items()
        .into_iter()
        .enumerate()
        .find(|(_, item)| item.stock_action() == action)
        .map(|(i, item)| (i as i32, item))
}

fn menu_item(name: &str, label: &str, activate: impl Fn() + 'static) -> ContextMenuItem {
    let action = gio::SimpleAction::new(name, None);
    action.connect_activate(move |_, _| activate());
    ContextMenuItem::from_gaction(&action, label, None)
}

fn extend_context_menu(menu: &ContextMenu, hit: &HitTestResult, sender: &ComponentSender<Tab>) {
    if let Some(uri) = hit.link_uri().filter(|_| hit.context_is_link()) {
        let item = menu_item("open-link-in-new-tab", "Open Link in New Tab", glib::clone!(
            #[strong] sender,
            move || { let _ = sender.output(TabOutput::OpenInNewTab(uri.to_string())); }
        ));
        match find_item(menu, ContextMenuAction::OpenLinkInNewWindow) {
            Some((pos, _)) => menu.insert(&item, pos),
            None => menu.append(&item),
        }
    }

    if let Some(uri) = hit.image_uri().filter(|_| hit.context_is_image()) {
        let uri = uri.to_string();
        menu.append(&menu_item("open-image-in-new-tab", "Open Image in New Tab", glib::clone!(
            #[strong] sender, #[strong] uri,
            move || { let _ = sender.output(TabOutput::OpenInNewTab(uri.clone())); }
        )));

        // WebKit's own "Copy Image" doesn't work here, so swap in one that
        // fetches the image and puts the texture on the clipboard.
        let copy = menu_item("copy-image", "Copy Image", move || copy_image(&uri));
        match find_item(menu, ContextMenuAction::CopyImageToClipboard) {
            Some((pos, builtin)) => {
                menu.remove(&builtin);
                menu.insert(&copy, pos);
            }
            None => menu.append(&copy),
        }
    }
}

fn copy_image(uri: &str) {
    gio::File::for_uri(uri).load_bytes_async(gio::Cancellable::NONE, |result| {
        let Ok((bytes, _)) = result else { return };
        let (Ok(texture), Some(display)) = (gdk::Texture::from_bytes(&bytes), gdk::Display::default()) else {
            return;
        };
        display.clipboard().set_texture(&texture);
    });
}
