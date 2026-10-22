//! A browser window: navigation bar, progress bar and a notebook of tabs.

use gtk::gdk::Key;
use gtk::prelude::*;
use gtk::{gio, glib};
use relm4::prelude::*;
use webkit6::prelude::*;
use webkit6::{LoadEvent, WebView};

use crate::completion::{Completion, CompletionMsg, CompletionOutput};
use crate::passwords::{self, Login};
use crate::tab::{SaveOffer, Tab, TabId, TabOutput};
use crate::web::{self, Page};
use crate::{store, sync, urls};

struct TabHandle {
    id: TabId,
    webview: WebView,
    controller: Controller<Tab>,
}

pub struct BrowserWindow {
    notebook: gtk::Notebook,
    url_bar: gtk::Entry,
    completion: Controller<Completion>,
    tabs: Vec<TabHandle>,
    next_tab_id: usize,
    /// Navigation state of the active tab.
    uri: String,
    /// Set when `uri` should overwrite the URL bar this update; otherwise
    /// what the user is typing is left alone.
    sync_uri: bool,
    can_go_back: bool,
    can_go_forward: bool,
    progress: f64,
    /// The 🔑 button for filling saved logins, and its popover.
    key_button: gtk::MenuButton,
    key_popover: gtk::Popover,
    /// Saved logins for the active tab's site.
    logins: Vec<Login>,
    /// A just-submitted login to offer saving.
    offer: Option<SaveOffer>,
    /// An error to show in the key popover.
    key_error: Option<String>,
}

#[derive(Debug)]
pub enum WindowMsg {
    NewTab(Page),
    NewWindow(Page),
    CloseTab(TabId),
    CloseOtherTabs(TabId),
    TabSwitched,
    TabLoadChanged(TabId, LoadEvent),
    TabProgress(TabId),
    Go,
    Navigate(String),
    Back,
    Forward,
    Reload,
    OfferSave(TabId, SaveOffer),
    Fill(Login),
    Filled(Login, Result<String, String>),
    SaveLogin,
    SavedLogin(Result<(), String>),
    NeverSave,
    DismissOffer,
    ManagePasswords,
}

impl BrowserWindow {
    pub fn open(page: Page) {
        let builder = Self::builder();
        relm4::main_application().add_window(&builder.root);
        builder.launch(page).detach_runtime();
    }

    fn active(&self) -> Option<&TabHandle> {
        let page = self.notebook.nth_page(self.notebook.current_page())?;
        self.tabs.iter().find(|t| t.controller.widget().upcast_ref::<gtk::Widget>() == &page)
    }

    fn is_active(&self, id: TabId) -> bool {
        self.active().is_some_and(|t| t.id == id)
    }

    fn sync_nav(&mut self, sync_uri: bool) {
        let Some(webview) = self.active().map(|t| t.webview.clone()) else { return };
        self.can_go_back = webview.can_go_back();
        self.can_go_forward = webview.can_go_forward();
        self.progress = if webview.is_loading() { webview.estimated_load_progress() } else { 0.0 };
        if sync_uri {
            self.uri = webview.uri().unwrap_or_default().into();
            self.sync_uri = true;
            self.logins = self.host().map(|h| passwords::logins(&h)).unwrap_or_default();
        }
    }

    /// The active tab's host, for web pages.
    fn host(&self) -> Option<String> {
        let uri = self.active()?.webview.uri()?;
        let (_, host) = urls::origin(&uri).filter(|_| uri.starts_with("http"))?;
        Some(host.to_string())
    }

    /// Rebuilds the key popover: the pending save offer, or the saved logins.
    fn refresh_key_popover(&self, sender: &ComponentSender<Self>) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
        content.set_margin_top(6);
        content.set_margin_bottom(6);
        content.set_margin_start(6);
        content.set_margin_end(6);
        let button = |label: &str, msg: fn() -> WindowMsg| {
            let button = gtk::Button::with_label(label);
            button.connect_clicked(glib::clone!(#[strong] sender, move |_| sender.input(msg())));
            button
        };

        if let Some(error) = &self.key_error {
            let label = gtk::Label::builder().label(error).wrap(true).max_width_chars(40).xalign(0.0).build();
            label.add_css_class("error");
            content.append(&label);
        }

        if let Some(offer) = &self.offer {
            let who = if offer.login.username.is_empty() { "the password" } else { &offer.login.username };
            let question = format!(
                "{} {who} for {}?",
                if offer.update { "Update the saved password for" } else { "Save" },
                offer.login.host,
            );
            content.append(&gtk::Label::builder().label(question).wrap(true).max_width_chars(40).xalign(0.0).build());
            let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            buttons.set_halign(gtk::Align::End);
            buttons.set_margin_top(6);
            buttons.append(&button("Never for This Site", || WindowMsg::NeverSave));
            buttons.append(&button("Not Now", || WindowMsg::DismissOffer));
            let save = button(if offer.update { "Update" } else { "Save" }, || WindowMsg::SaveLogin);
            save.add_css_class("suggested-action");
            buttons.append(&save);
            content.append(&buttons);
        } else {
            for login in &self.logins {
                let name = if login.username.is_empty() { "(no username)" } else { &login.username };
                let label = match self.host() {
                    Some(host) if host.trim_start_matches("www.") != login.host => format!("{name} ({})", login.host),
                    _ => name.to_string(),
                };
                let fill = gtk::Button::with_label(&label);
                fill.set_has_frame(false);
                fill.set_tooltip_text(Some("Fill in this login"));
                fill.connect_clicked(glib::clone!(
                    #[strong] sender, #[strong] login,
                    move |_| sender.input(WindowMsg::Fill(login.clone()))
                ));
                content.append(&fill);
            }
            content.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
            let manage = button("Manage Passwords…", || WindowMsg::ManagePasswords);
            manage.set_has_frame(false);
            content.append(&manage);
        }
        self.key_popover.set_child(Some(&content));
    }

    /// Opens the key popover once the button is showing.
    fn show_key_popover(&self) {
        let button = self.key_button.clone();
        glib::idle_add_local_once(move || button.popup());
    }

    fn load(&mut self, uri: &str) {
        if let Some(tab) = self.active() {
            tab.webview.load_uri(uri);
            // Take focus from the URL bar now, so the text update below
            // doesn't pop the completion back open.
            tab.webview.grab_focus();
        }
        self.uri = uri.to_string();
        self.sync_uri = true;
    }

    fn add_tab(&mut self, page: Page, sender: &ComponentSender<Self>) {
        let id = TabId(self.next_tab_id);
        self.next_tab_id += 1;
        let webview = web::webview(page);
        let controller = Tab::builder().launch((id, webview.clone())).forward(sender.input_sender(), |out| match out {
            TabOutput::LoadChanged(id, event) => WindowMsg::TabLoadChanged(id, event),
            TabOutput::Progress(id) => WindowMsg::TabProgress(id),
            TabOutput::OpenInNewTab(uri) => WindowMsg::NewTab(Page::Uri(uri)),
            TabOutput::OpenInNewWindow(page) => WindowMsg::NewWindow(page),
            TabOutput::Close(id) => WindowMsg::CloseTab(id),
            TabOutput::CloseOthers(id) => WindowMsg::CloseOtherTabs(id),
            TabOutput::OfferSave(id, offer) => WindowMsg::OfferSave(id, offer),
        });

        let child = controller.widget();
        let label = controller.widgets().label.clone();
        let index = self.notebook.append_page(child, Some(&label));
        self.notebook.set_tab_reorderable(child, true);
        let page = self.notebook.page(child);
        page.set_tab_expand(false);
        page.set_tab_fill(true);

        self.tabs.push(TabHandle { id, webview, controller });
        self.notebook.set_current_page(Some(index));
    }

    fn remove_tab(&mut self, id: TabId) {
        if let Some(i) = self.tabs.iter().position(|t| t.id == id) {
            let tab = self.tabs.remove(i);
            self.notebook.remove_page(self.notebook.page_num(tab.controller.widget()));
        }
    }
}

#[relm4::component(pub)]
impl Component for BrowserWindow {
    type Init = Page;
    type Input = WindowMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::ApplicationWindow {
            set_default_size: (1100, 750),
            set_title: Some(if store::is_private() { "rug — private browsing" } else { "rug" }),
            set_visible: true,

            add_controller = gtk::EventControllerKey {
                connect_key_pressed[sender] => move |_, key, _, _| {
                    if key == Key::F5 {
                        sender.input(WindowMsg::Reload);
                    }
                    glib::Propagation::Proceed
                },
            },

            gtk::Overlay {
                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    gtk::Box {
                        set_spacing: 5,
                        set_margin_all: 5,

                        gtk::Button {
                            set_label: "←",
                            #[watch]
                            set_sensitive: model.can_go_back,
                            connect_clicked => WindowMsg::Back,
                        },
                        gtk::Button {
                            set_label: "→",
                            #[watch]
                            set_sensitive: model.can_go_forward,
                            connect_clicked => WindowMsg::Forward,
                        },
                        #[local_ref]
                        url_bar -> gtk::Entry {
                            set_hexpand: true,
                            #[track(model.sync_uri)]
                            set_text: &model.uri,
                            connect_activate => WindowMsg::Go,
                        },
                        #[local_ref]
                        key_button -> gtk::MenuButton {
                            set_label: "🔑",
                            set_tooltip_text: Some("Saved passwords"),
                            set_popover: Some(key_popover),
                            #[watch]
                            set_visible: !model.logins.is_empty() || model.offer.is_some(),
                        },
                        gtk::Button {
                            set_label: "Go",
                            connect_clicked => WindowMsg::Go,
                        },
                    },

                    gtk::ProgressBar {
                        #[watch]
                        set_fraction: model.progress,
                    },

                    #[local_ref]
                    notebook -> gtk::Notebook {
                        set_vexpand: true,
                        set_scrollable: true,
                        connect_switch_page[sender] => move |_, _, _| sender.input(WindowMsg::TabSwitched),
                    },
                },

                add_overlay: model.completion.widget(),
            },
        }
    }

    fn init(page: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let url_bar = gtk::Entry::new();
        let completion = Completion::builder()
            .launch(url_bar.clone())
            .forward(sender.input_sender(), |CompletionOutput::Navigate(url)| WindowMsg::Navigate(url));

        let mut model = BrowserWindow {
            notebook: gtk::Notebook::new(),
            url_bar,
            completion,
            tabs: Vec::new(),
            next_tab_id: 0,
            uri: String::new(),
            sync_uri: false,
            can_go_back: false,
            can_go_forward: false,
            progress: 0.0,
            key_button: gtk::MenuButton::new(),
            key_popover: gtk::Popover::new(),
            logins: Vec::new(),
            offer: None,
            key_error: None,
        };
        let notebook = &model.notebook;
        let url_bar = &model.url_bar;
        let key_button = &model.key_button;
        let key_popover = &model.key_popover;
        let widgets = view_output!();

        let new_tab = gtk::Button::builder().label("+").has_frame(false).build();
        new_tab.connect_clicked(glib::clone!(
            #[strong] sender,
            move |_| sender.input(WindowMsg::NewTab(Page::Home))
        ));
        model.notebook.set_action_widget(&new_tab, gtk::PackType::End);

        // Select the whole URL when the bar is focused, once the click that
        // focused it has placed its cursor.
        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(glib::clone!(
            #[weak(rename_to = url_bar)] model.url_bar,
            move |_| {
                glib::idle_add_local_once(move || url_bar.select_region(0, -1));
            }
        ));
        model.url_bar.add_controller(focus);

        model.add_tab(page, &sender);
        model.refresh_key_popover(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        self.sync_uri = false;
        let refresh_popover = !matches!(msg, WindowMsg::TabProgress(_));
        match msg {
            WindowMsg::NewTab(page) => self.add_tab(page, &sender),
            WindowMsg::NewWindow(page) => Self::open(page),
            WindowMsg::CloseTab(id) => {
                self.remove_tab(id);
                if self.tabs.is_empty() {
                    root.close();
                }
            }
            WindowMsg::CloseOtherTabs(id) => {
                let others: Vec<TabId> = self.tabs.iter().map(|t| t.id).filter(|&t| t != id).collect();
                for other in others {
                    self.remove_tab(other);
                }
            }
            WindowMsg::TabSwitched => {
                self.offer = None;
                self.key_error = None;
                self.sync_nav(true);
            }
            WindowMsg::TabLoadChanged(id, event) => {
                if self.is_active(id) {
                    if event == LoadEvent::Started {
                        if let Some(tab) = self.active() {
                            tab.webview.grab_focus();
                        }
                    }
                    self.sync_nav(true);
                }
            }
            WindowMsg::TabProgress(id) => {
                if self.is_active(id) {
                    self.sync_nav(false);
                }
            }
            WindowMsg::Go => {
                self.completion.emit(CompletionMsg::Hide);
                let uri = urls::resolve(&self.url_bar.text(), &store::settings());
                self.load(&uri);
            }
            WindowMsg::Navigate(uri) => self.load(&uri),
            WindowMsg::Back => {
                if let Some(tab) = self.active() {
                    tab.webview.go_back();
                }
            }
            WindowMsg::Forward => {
                if let Some(tab) = self.active() {
                    tab.webview.go_forward();
                }
            }
            WindowMsg::Reload => {
                if let Some(tab) = self.active() {
                    tab.webview.reload();
                }
            }
            WindowMsg::OfferSave(id, offer) => {
                if self.is_active(id) {
                    self.offer = Some(offer);
                    self.key_error = None;
                    self.show_key_popover();
                }
            }
            WindowMsg::Fill(login) => {
                self.key_popover.popdown();
                let decrypting = login.clone();
                relm4::spawn_local(glib::clone!(
                    #[strong] sender,
                    async move {
                        let result = gio::spawn_blocking(move || passwords::decrypt(&decrypting))
                            .await
                            .unwrap_or_else(|_| Err("gpg crashed".into()));
                        sender.input(WindowMsg::Filled(login, result));
                    }
                ));
            }
            WindowMsg::Filled(login, Ok(password)) => {
                // Only into the site it's for, in case the tab moved on.
                let Some(tab) = self.active().filter(|_| self.host().is_some_and(|h| login.matches(&h))) else {
                    return;
                };
                let script = format!(
                    "rugFill({}, {})",
                    serde_json::to_string(&login.username).unwrap_or_default(),
                    serde_json::to_string(&password).unwrap_or_default(),
                );
                tab.webview.evaluate_javascript(&script, Some(web::SCRIPT_WORLD), None, None::<&gio::Cancellable>, |_| {});
                passwords::remember_filled(&login, &password);
                self.key_error = None;
            }
            WindowMsg::Filled(_, Err(error)) => {
                self.key_error = Some(format!("Couldn't decrypt the password: {error}"));
                self.show_key_popover();
            }
            WindowMsg::SaveLogin => {
                let Some(offer) = self.offer.take() else { return };
                self.key_popover.popdown();
                relm4::spawn_local(glib::clone!(
                    #[strong] sender,
                    async move {
                        let result = gio::spawn_blocking(move || passwords::save(&offer.login, &offer.password, &offer.url))
                            .await
                            .unwrap_or_else(|_| Err("gpg crashed".into()));
                        sender.input(WindowMsg::SavedLogin(result));
                    }
                ));
            }
            WindowMsg::SavedLogin(result) => {
                sync::sync_soon();
                self.logins = self.host().map(|h| passwords::logins(&h)).unwrap_or_default();
                if let Err(error) = result {
                    self.key_error = Some(format!("Couldn't save the password: {error}"));
                    self.show_key_popover();
                }
            }
            WindowMsg::NeverSave => {
                if let Some(offer) = self.offer.take() {
                    store::update_settings(|s| s.never_save.push(offer.login.host));
                }
                self.key_popover.popdown();
            }
            WindowMsg::DismissOffer => {
                self.offer = None;
                self.key_popover.popdown();
            }
            WindowMsg::ManagePasswords => {
                self.key_popover.popdown();
                self.add_tab(Page::Uri(crate::pages::PASSWORDS.into()), &sender);
            }
        }
        if refresh_popover {
            self.refresh_key_popover(&sender);
        }
    }
}
