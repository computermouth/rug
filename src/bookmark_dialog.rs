//! The "Bookmark Tab" dialog.

use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;

use crate::bookmarks;

pub struct BookmarkDialog {
    title: gtk::Entry,
    url: gtk::Entry,
    folder: gtk::DropDown,
    /// The folder id behind each entry of `folder`.
    folder_ids: Vec<Option<u64>>,
}

#[derive(Debug)]
pub enum BookmarkDialogMsg {
    Save,
    Cancel,
}

#[relm4::component(pub)]
impl Component for BookmarkDialog {
    /// Title and URL to start with.
    type Init = (String, String);
    type Input = BookmarkDialogMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Window {
            set_title: Some("Add Bookmark"),
            set_modal: true,
            set_resizable: false,
            set_default_width: 420,
            set_visible: true,

            add_controller = gtk::EventControllerKey {
                connect_key_pressed[sender] => move |_, key, _, _| {
                    if key != gtk::gdk::Key::Escape {
                        return glib::Propagation::Proceed;
                    }
                    sender.input(BookmarkDialogMsg::Cancel);
                    glib::Propagation::Stop
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                set_margin_all: 16,

                gtk::Label { set_label: "Name", set_xalign: 0.0, add_css_class: "dim-label" },
                #[local_ref]
                title_entry -> gtk::Entry {
                    connect_activate => BookmarkDialogMsg::Save,
                },

                gtk::Label { set_label: "URL", set_xalign: 0.0, set_margin_top: 6, add_css_class: "dim-label" },
                #[local_ref]
                url_entry -> gtk::Entry {
                    connect_activate => BookmarkDialogMsg::Save,
                },

                gtk::Label { set_label: "Folder", set_xalign: 0.0, set_margin_top: 6, add_css_class: "dim-label" },
                #[local_ref]
                folder_dropdown -> gtk::DropDown {},

                gtk::Box {
                    set_halign: gtk::Align::End,
                    set_spacing: 8,
                    set_margin_top: 12,

                    gtk::Button {
                        set_label: "Cancel",
                        connect_clicked => BookmarkDialogMsg::Cancel,
                    },
                    gtk::Button {
                        set_label: "Save",
                        add_css_class: "suggested-action",
                        connect_clicked => BookmarkDialogMsg::Save,
                    },
                },
            },
        }
    }

    fn init((title, url): Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let folders = bookmarks::folders();
        // Full paths ("Dev / Rust") rather than indentation, which would also
        // show up in the closed dropdown.
        let title_of = |id: u64| folders.iter().find(|f| f.id == id).map_or("", |f| f.title.as_str());
        let names: Vec<String> = std::iter::once("Bookmarks".to_string())
            .chain(folders.iter().map(|f| f.path.iter().map(|&id| title_of(id)).collect::<Vec<_>>().join(" / ")))
            .collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();

        let model = BookmarkDialog {
            title: gtk::Entry::builder().text(title).build(),
            url: gtk::Entry::builder().text(url).build(),
            folder: gtk::DropDown::from_strings(&names),
            folder_ids: std::iter::once(None).chain(folders.iter().map(|f| Some(f.id))).collect(),
        };
        let title_entry = &model.title;
        let url_entry = &model.url;
        let folder_dropdown = &model.folder;
        let widgets = view_output!();

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, _sender: ComponentSender<Self>, root: &Self::Root) {
        match msg {
            BookmarkDialogMsg::Save => {
                let url = self.url.text();
                if url.trim().is_empty() {
                    self.url.grab_focus();
                    return;
                }
                let parent = self.folder_ids.get(self.folder.selected() as usize).copied().flatten();
                bookmarks::add(parent, &self.title.text(), Some(&url));
                root.close();
            }
            BookmarkDialogMsg::Cancel => root.close(),
        }
    }
}
