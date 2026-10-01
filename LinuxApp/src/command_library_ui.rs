//! Command-library and snippet presentation for the Linux application.
//!
//! State and command semantics stay in command_library, snippet, host
//! and the session/SSH layer. This module only builds and coordinates GTK UI.

use adw::prelude::*;
use gtk::glib::{self, clone};
use std::cell::RefCell;
use std::rc::Rc;

use crate::{
    SessionArea, command_library, host, session_form_dialog, snippet, start_command_session,
};

/// Öppnar Kommandobibliotek+Snippets-vyn för `host` i en ny flik: statiska
/// referenskommandon (`command_library.rs`) + användarens egna sparade
/// snippets (`snippet.rs`). Port av App/CommandLibraryView.swift.
pub(crate) fn open_command_library_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    jump: Option<host::Host>,
) {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();

    let add_button = gtk::Button::from_icon_name("list-add-symbolic");
    add_button.set_tooltip_text(Some("Ny snippet"));
    let toolbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    toolbar.append(
        &gtk::Label::builder()
            .label(format!("Kommandon: {}", host.alias))
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build(),
    );
    toolbar.append(&add_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    area.append_page(&content, &format!("Kommandon: {}", host.alias));

    add_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        snippet_store,
        #[weak]
        list,
        move |_| show_snippet_edit_dialog(
            &area,
            host.clone(),
            password.clone(),
            &snippet_store,
            &list,
            None,
            jump.clone()
        )
    ));

    refresh_command_library_list(area, &host, &password, snippet_store, &list, &jump);
}

fn refresh_command_library_list(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    list: &gtk::ListBox,
    jump: &Option<host::Host>,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }

    for s in snippet_store.borrow().all() {
        list.append(&build_snippet_row(
            area,
            host,
            password,
            snippet_store,
            s.clone(),
            list,
            jump,
        ));
    }
    for entry in command_library::all() {
        list.append(&build_library_entry_row(area, host, password, entry, jump));
    }
}

fn build_snippet_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    snippet: snippet::Snippet,
    list: &gtk::ListBox,
    jump: &Option<host::Host>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&snippet.name)
        .subtitle(&snippet.template)
        .build();
    let suffix = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();

    let run_button = gtk::Button::from_icon_name("media-playback-start-symbolic");
    run_button.set_tooltip_text(Some("Kör"));
    run_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        snippet,
        move |_| run_snippet(
            &area,
            host.clone(),
            password.clone(),
            snippet.clone(),
            jump.clone()
        )
    ));

    let edit_button = gtk::Button::from_icon_name("document-edit-symbolic");
    edit_button.set_tooltip_text(Some("Redigera"));
    edit_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        snippet_store,
        #[weak]
        list,
        #[strong]
        snippet,
        move |_| show_snippet_edit_dialog(
            &area,
            host.clone(),
            password.clone(),
            &snippet_store,
            &list,
            Some(snippet.clone()),
            jump.clone()
        )
    ));

    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some("Ta bort"));
    delete_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        snippet_store,
        #[weak]
        list,
        #[strong(rename_to = snippet_id)]
        snippet.id,
        move |_| {
            // `delete_synced`, inte `delete`: utan gravsten kommer
            // snippeten tillbaka vid nästa synk mot en enhet som
            // fortfarande har den, och användaren får radera om och om igen.
            let recorded = host::HostStore::default_path()
                .and_then(host::HostStore::open)
                .and_then(|mut hosts| {
                    snippet_store
                        .borrow_mut()
                        .delete_synced(snippet_id, &mut hosts)
                });
            if let Err(e) = recorded {
                eprintln!("kunde inte ta bort snippeten: {e}");
                return;
            }
            refresh_command_library_list(&area, &host, &password, &snippet_store, &list, &jump);
        }
    ));

    suffix.append(&run_button);
    suffix.append(&edit_button);
    suffix.append(&delete_button);
    row.add_suffix(&suffix);
    row
}

fn build_library_entry_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    entry: command_library::Entry,
    jump: &Option<host::Host>,
) -> adw::ActionRow {
    let mut subtitle = format!("[{}] {}", entry.category.label(), entry.summary);
    if let Some(example) = entry.example {
        subtitle.push_str(&format!(" — t.ex. {example}"));
    }
    let row = adw::ActionRow::builder()
        .title(entry.command)
        .subtitle(subtitle)
        .build();

    let suffix = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();

    if let Some(docs_url) = entry.docs_url {
        let docs_button = gtk::Button::from_icon_name("help-about-symbolic");
        docs_button.set_tooltip_text(Some("Dokumentation"));
        docs_button.connect_clicked(move |_| {
            gtk::gio::AppInfo::launch_default_for_uri(docs_url, gtk::gio::AppLaunchContext::NONE)
                .ok();
        });
        suffix.append(&docs_button);
    }

    let run_button = gtk::Button::from_icon_name("media-playback-start-symbolic");
    run_button.set_tooltip_text(Some("Kör"));
    run_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        move |_| {
            let snippet =
                snippet::Snippet::new(entry.summary.to_string(), entry.command.to_string());
            run_snippet(&area, host.clone(), password.clone(), snippet, jump.clone());
        }
    ));
    suffix.append(&run_button);
    row.add_suffix(&suffix);
    row
}

/// Kör en snippet: fyller i `{{variabler}}` via en dialog om det finns
/// några, annars öppnar direkt en ny terminalflik med det rendrade
/// kommandot som `startup_command` (samma mönster som Docker-shell).
fn run_snippet(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    snippet: snippet::Snippet,
    jump: Option<host::Host>,
) {
    if snippet.variable_names().is_empty() {
        launch_rendered_command(
            area,
            host,
            password,
            &snippet.name,
            snippet.rendered(&std::collections::HashMap::new()),
            jump,
        );
    } else {
        prompt_snippet_variables(area, host, password, snippet, jump);
    }
}

fn launch_rendered_command(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    title_suffix: &str,
    command: String,
    jump: Option<host::Host>,
) {
    let mut h = host;
    h.startup_command = Some(command);
    h.alias = format!("{}: {title_suffix}", h.alias);
    start_command_session(area, h, password, jump);
}

fn prompt_snippet_variables(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    snippet: snippet::Snippet,
    jump: Option<host::Host>,
) {
    let names = snippet.variable_names();
    let group = adw::PreferencesGroup::builder()
        .title(&snippet.name)
        .description(&snippet.template)
        .build();
    let entries: Vec<(String, adw::EntryRow)> = names
        .iter()
        .map(|name| {
            let entry_row = adw::EntryRow::builder().title(name.as_str()).build();
            group.add(&entry_row);
            (name.clone(), entry_row)
        })
        .collect();

    let page = adw::PreferencesPage::new();
    page.add(&group);

    let run_button = gtk::Button::with_label("Kör");
    run_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&run_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = session_form_dialog(area, "Fyll i kommandot", &content);

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    run_button.connect_clicked(clone!(
        #[weak]
        win,
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        snippet,
        move |_| {
            let values: std::collections::HashMap<String, String> = entries
                .iter()
                .map(|(name, row)| (name.clone(), row.text().to_string()))
                .collect();
            let rendered = snippet.rendered(&values);
            win.close();
            launch_rendered_command(
                &area,
                host.clone(),
                password.clone(),
                &snippet.name,
                rendered,
                jump.clone(),
            );
        }
    ));

    win.present();
}

fn show_snippet_edit_dialog(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    list: &gtk::ListBox,
    existing: Option<snippet::Snippet>,
    jump: Option<host::Host>,
) {
    let is_edit = existing.is_some();
    let name_row = adw::EntryRow::builder().title("Namn").build();
    let template_row = adw::EntryRow::builder()
        .title("Kommando (t.ex. docker restart {{service}})")
        .build();
    if let Some(s) = &existing {
        name_row.set_text(&s.name);
        template_row.set_text(&s.template);
    }

    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    group.add(&template_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let save_button = gtk::Button::with_label(if is_edit { "Spara" } else { "Lägg till" });
    save_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = session_form_dialog(area, "Snippet", &content);

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    save_button.connect_clicked(clone!(
        #[weak]
        win,
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        snippet_store,
        #[weak]
        list,
        #[strong]
        existing,
        move |_| {
            let name = name_row.text().to_string();
            let template = template_row.text().to_string();
            if name.is_empty() || template.is_empty() {
                return;
            }
            let snippet = if let Some(mut s) = existing.clone() {
                s.name = name;
                s.template = template;
                s
            } else {
                snippet::Snippet::new(name, template)
            };
            if let Err(e) = snippet_store.borrow_mut().upsert(snippet) {
                eprintln!("kunde inte spara snippeten: {e}");
                return;
            }
            refresh_command_library_list(&area, &host, &password, &snippet_store, &list, &jump);
            win.close();
        }
    ));

    win.present();
}
