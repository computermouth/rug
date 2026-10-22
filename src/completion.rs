//! The history dropdown under the URL bar.

use std::time::Duration;

use gtk::gdk::Key;
use gtk::prelude::*;
use gtk::{gdk, glib};
use relm4::factory::{DynamicIndex, FactoryComponent, FactorySender, FactoryVecDeque};
use relm4::prelude::*;

use crate::store::{self, Visit};

const MAX_RESULTS: usize = 8;

pub struct Completion {
    entry: gtk::Entry,
    focused: bool,
    visible: bool,
    rows: FactoryVecDeque<Row>,
    urls: Vec<String>,
    x: i32,
    y: i32,
    width: i32,
}

#[derive(Debug)]
pub enum CompletionMsg {
    TextChanged,
    Focus(bool),
    Hide,
    Next,
    Previous,
    Pick(usize),
}

#[derive(Debug)]
pub enum CompletionOutput {
    Navigate(String),
}

impl Completion {
    fn hide(&mut self) {
        self.visible = false;
        self.rows.widget().unselect_all();
    }

    fn refresh(&mut self, root: &gtk::Box) {
        let query = self.entry.text().to_string();
        let results = if self.focused && !query.is_empty() { store::search(&query, MAX_RESULTS) } else { Vec::new() };
        if results.is_empty() {
            self.hide();
            return;
        }

        self.urls = results.iter().map(|v| v.url.clone()).collect();
        let mut rows = self.rows.guard();
        rows.clear();
        for visit in results {
            rows.push_back((visit, query.clone()));
        }
        drop(rows);

        // Sit just below the entry, in the overlay we're attached to.
        if let Some(bounds) = root.parent().and_then(|p| self.entry.compute_bounds(&p)) {
            self.x = bounds.x() as i32;
            self.y = (bounds.y() + bounds.height()) as i32;
            self.width = bounds.width() as i32;
        }
        self.visible = true;
    }

    fn select(&self, row: Option<gtk::ListBoxRow>) {
        let list = self.rows.widget();
        match row {
            Some(row) => list.select_row(Some(&row)),
            None => list.unselect_all(),
        }
    }
}

#[relm4::component(pub)]
impl Component for Completion {
    /// The URL bar to complete for.
    type Init = gtk::Entry;
    type Input = CompletionMsg;
    type Output = CompletionOutput;
    type CommandOutput = ();

    view! {
        gtk::Box {
            add_css_class: "completion-dropdown",
            set_halign: gtk::Align::Start,
            set_valign: gtk::Align::Start,
            #[watch]
            set_visible: model.visible,
            #[watch]
            set_margin_start: model.x,
            #[watch]
            set_margin_top: model.y,

            #[local_ref]
            list -> gtk::ListBox {
                set_selection_mode: gtk::SelectionMode::Single,
                set_focusable: false,
                #[watch]
                set_width_request: model.width,
            },
        }
    }

    fn init(entry: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let model = Completion {
            entry: entry.clone(),
            focused: false,
            visible: false,
            rows: FactoryVecDeque::builder().launch(gtk::ListBox::default()).detach(),
            urls: Vec::new(),
            x: 0,
            y: 0,
            width: -1,
        };
        let list = model.rows.widget();
        let widgets = view_output!();

        entry.connect_changed(glib::clone!(
            #[strong] sender,
            move |_| sender.input(CompletionMsg::TextChanged)
        ));

        let focus = gtk::EventControllerFocus::new();
        focus.connect_enter(glib::clone!(#[strong] sender, move |_| sender.input(CompletionMsg::Focus(true))));
        focus.connect_leave(glib::clone!(#[strong] sender, move |_| sender.input(CompletionMsg::Focus(false))));
        entry.add_controller(focus);

        // Capture phase, so arrows and Enter reach us before the entry.
        let keys = gtk::EventControllerKey::new();
        keys.set_propagation_phase(gtk::PropagationPhase::Capture);
        keys.connect_key_pressed(glib::clone!(
            #[weak] root, #[weak] list, #[strong] sender,
            #[upgrade_or] glib::Propagation::Proceed,
            move |_, key, _, _| {
                if !root.is_visible() {
                    return glib::Propagation::Proceed;
                }
                let msg = match key {
                    Key::Down => CompletionMsg::Next,
                    Key::Up => CompletionMsg::Previous,
                    Key::Escape => CompletionMsg::Hide,
                    Key::Return | Key::KP_Enter => match list.selected_row() {
                        Some(row) => CompletionMsg::Pick(row.index() as usize),
                        None => return glib::Propagation::Proceed,
                    },
                    _ => return glib::Propagation::Proceed,
                };
                sender.input(msg);
                glib::Propagation::Stop
            }
        ));
        entry.add_controller(keys);

        // Act on press rather than release: the entry losing focus hides us.
        let click = gtk::GestureClick::new();
        click.connect_pressed(glib::clone!(
            #[weak] list, #[strong] sender,
            move |_, _, _, y| {
                if let Some(row) = list.row_at_y(y as i32) {
                    sender.input(CompletionMsg::Pick(row.index() as usize));
                }
            }
        ));
        list.add_controller(click);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, root: &Self::Root) {
        let list = self.rows.widget();
        match msg {
            CompletionMsg::TextChanged => self.refresh(root),
            CompletionMsg::Focus(true) => self.focused = true,
            CompletionMsg::Focus(false) => {
                self.focused = false;
                // Give a click on a row time to land first.
                glib::timeout_add_local_once(Duration::from_millis(150), move || sender.input(CompletionMsg::Hide));
            }
            CompletionMsg::Hide => self.hide(),
            CompletionMsg::Next => {
                let next = match list.selected_row() {
                    None => list.row_at_index(0),
                    Some(row) => list.row_at_index(row.index() + 1),
                };
                self.select(next);
            }
            CompletionMsg::Previous => {
                let prev = match list.selected_row() {
                    None => list.row_at_index(self.rows.len() as i32 - 1),
                    Some(row) if row.index() == 0 => None,
                    Some(row) => list.row_at_index(row.index() - 1),
                };
                self.select(prev);
            }
            CompletionMsg::Pick(index) => {
                if let Some(url) = self.urls.get(index).cloned() {
                    self.hide();
                    let _ = sender.output(CompletionOutput::Navigate(url));
                }
            }
        }
    }
}

struct Row {
    title: String,
    url: String,
    favicon: gdk::Texture,
}

#[relm4::factory]
impl FactoryComponent for Row {
    /// A matching visit and the query it matched.
    type Init = (Visit, String);
    type Input = ();
    type Output = ();
    type CommandOutput = ();
    type ParentWidget = gtk::ListBox;

    view! {
        #[root]
        gtk::Box {
            set_spacing: 8,
            set_margin_start: 8,
            set_margin_end: 8,
            set_margin_top: 4,
            set_margin_bottom: 4,

            gtk::Image {
                set_pixel_size: 16,
                set_valign: gtk::Align::Center,
                set_paintable: Some(&self.favicon),
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 2,
                set_hexpand: true,

                gtk::Label {
                    set_markup: &self.title,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },
                gtk::Label {
                    set_markup: &self.url,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },
            },
        },

        #[local_ref]
        returned_widget -> gtk::ListBoxRow {
            set_focusable: false,
        },
    }

    fn init_model((visit, query): Self::Init, _index: &DynamicIndex, _sender: FactorySender<Self>) -> Self {
        let title = if visit.title.is_empty() { &visit.url } else { &visit.title };
        Self {
            title: highlight(title, &query),
            url: format!("<small><span foreground='#888888'>{}</span></small>", highlight(&visit.url, &query)),
            favicon: store::favicon(&visit.url),
        }
    }
}

/// Pango markup for `text` with the first case-insensitive match of `query` in bold.
fn highlight(text: &str, query: &str) -> String {
    let start = text.to_lowercase().find(&query.to_lowercase());
    if let Some(start) = start {
        let end = start + query.len();
        if text.is_char_boundary(start) && text.is_char_boundary(end) {
            return format!(
                "{}<b>{}</b>{}",
                glib::markup_escape_text(&text[..start]),
                glib::markup_escape_text(&text[start..end]),
                glib::markup_escape_text(&text[end..]),
            );
        }
    }
    glib::markup_escape_text(text).into()
}
