use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use std::cell::RefCell;
use std::rc::Rc;
use vte::prelude::*;

mod command_library;
mod docker;
mod host;
mod known_hosts;
mod settings;
mod sftp;
mod snippet;
mod ssh;
#[allow(dead_code)] // synk-UI (välja/konfigurera en SyncProvider) är inte byggt än
mod sync;

use host::{Host, HostStore};
use ssh::SshEvent;

const APP_ID: &str = "se.denied.bastion";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let store = Rc::new(RefCell::new(
        HostStore::open(HostStore::default_path()).expect("kunde inte öppna host-databasen"),
    ));
    let settings_store = Rc::new(RefCell::new(settings::AppSettingsStore::open(
        settings::AppSettingsStore::default_path(),
    )));
    let snippet_store = Rc::new(RefCell::new(snippet::SnippetStore::open(snippet::SnippetStore::default_path())));

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let area = SessionArea::new();
    refresh_list(&list, &store, app, &area, &settings_store, &snippet_store);

    let scrolled = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();

    let add_button = gtk::Button::from_icon_name("list-add-symbolic");
    add_button.set_tooltip_text(Some("Lägg till värd"));
    add_button.connect_clicked(clone!(
        #[weak]
        app,
        #[strong]
        store,
        #[weak]
        list,
        #[strong]
        area,
        #[strong]
        settings_store,
        #[strong]
        snippet_store,
        move |_| show_host_dialog(&app, &store, &list, &area, &settings_store, &snippet_store, None)
    ));

    let settings_button = gtk::Button::from_icon_name("preferences-system-symbolic");
    settings_button.set_tooltip_text(Some("Funktioner"));
    settings_button.connect_clicked(clone!(
        #[weak]
        app,
        #[strong]
        settings_store,
        #[strong]
        store,
        #[weak]
        list,
        #[strong]
        area,
        #[strong]
        snippet_store,
        move |_| show_settings_dialog(&app, &settings_store, &store, &list, &area, &snippet_store)
    ));

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.pack_end(&add_button);
    sidebar_header.pack_end(&settings_button);

    let sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_content.append(&sidebar_header);
    sidebar_content.append(&scrolled);

    let sidebar_page = adw::NavigationPage::builder()
        .title("Värdar")
        .child(&sidebar_content)
        .build();

    let content_header = adw::HeaderBar::new();
    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.append(&content_header);
    content_box.append(&area.tab_bar);
    content_box.append(&area.overlay);
    let content_page = adw::NavigationPage::builder()
        .title("Bastion")
        .child(&content_box)
        .build();

    list.connect_row_activated(clone!(
        #[strong]
        store,
        #[strong]
        area,
        move |_, row| {
            let index = row.index();
            if let Some(host) = store.borrow().all().get(index as usize).map(|h| (*h).clone()) {
                open_session(&area, host);
            }
        }
    ));

    let split_view = adw::NavigationSplitView::builder()
        .sidebar(&sidebar_page)
        .content(&content_page)
        .min_sidebar_width(260.0)
        .max_sidebar_width(360.0)
        .build();

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Bastion")
        .default_width(1000)
        .default_height(700)
        .content(&split_view)
        .build();

    window.present();
}

/// Bygger om värdlistan från HostStore. Långtryck på en rad öppnar
/// redigera/ta-bort-menyn — touchscreen-vänligt (motsvarar iOS-menyn för
/// samma gest, se b30bec8).
fn refresh_list(
    list: &gtk::ListBox,
    store: &Rc<RefCell<HostStore>>,
    app: &adw::Application,
    area: &Rc<SessionArea>,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    let toggles = settings_store.borrow().current();
    for h in store.borrow().all() {
        let row = adw::ActionRow::builder()
            .title(&h.alias)
            .subtitle(format!("{}@{}:{}", h.user, h.host_name, h.port))
            .activatable(true)
            .build();

        let menu_button = gtk::MenuButton::builder()
            .icon_name("view-more-symbolic")
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .build();
        let menu = gio_menu_for(&toggles);
        menu_button.set_menu_model(Some(&menu));
        row.add_suffix(&menu_button);

        let action_group = gtk::gio::SimpleActionGroup::new();
        let edit_action = gtk::gio::SimpleAction::new("edit", None);
        edit_action.connect_activate(clone!(
            #[weak]
            app,
            #[strong]
            store,
            #[weak]
            list,
            #[strong]
            area,
            #[strong]
            settings_store,
            #[strong]
            snippet_store,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                if let Some(host) = host {
                    show_host_dialog(&app, &store, &list, &area, &settings_store, &snippet_store, Some(host));
                }
            }
        ));
        let delete_action = gtk::gio::SimpleAction::new("delete", None);
        delete_action.connect_activate(clone!(
            #[weak]
            app,
            #[strong]
            store,
            #[weak]
            list,
            #[strong]
            area,
            #[strong]
            settings_store,
            #[strong]
            snippet_store,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                store.borrow_mut().delete(host_id).expect("kunde inte ta bort värden");
                refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store);
            }
        ));
        let docker_action = gtk::gio::SimpleAction::new("docker", None);
        docker_action.connect_activate(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                if let Some(host) = host {
                    require_password(&area, host, open_docker_view);
                }
            }
        ));
        let commands_action = gtk::gio::SimpleAction::new("commands", None);
        commands_action.connect_activate(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong]
            snippet_store,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                if let Some(host) = host {
                    let snippet_store = snippet_store.clone();
                    require_password(&area, host, move |area, host, password| {
                        open_command_library_view(area, host, password, &snippet_store)
                    });
                }
            }
        ));
        let sftp_action = gtk::gio::SimpleAction::new("sftp", None);
        sftp_action.connect_activate(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                if let Some(host) = host {
                    require_password(&area, host, open_sftp_view);
                }
            }
        ));
        action_group.add_action(&edit_action);
        action_group.add_action(&delete_action);
        action_group.add_action(&docker_action);
        action_group.add_action(&commands_action);
        action_group.add_action(&sftp_action);
        row.insert_action_group("host", Some(&action_group));

        list.append(&row);
    }
}

fn gio_menu_for(toggles: &settings::FeatureToggles) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Redigera"), Some("host.edit"));
    if toggles.show_docker {
        menu.append(Some("Docker"), Some("host.docker"));
    }
    if toggles.show_command_library {
        menu.append(Some("Kommandon"), Some("host.commands"));
    }
    if toggles.show_sftp_browser {
        menu.append(Some("Filer"), Some("host.sftp"));
    }
    menu.append(Some("Ta bort"), Some("host.delete"));
    menu
}

/// Lägg till/redigera-dialogen. `existing = None` skapar en ny värd.
fn show_host_dialog(
    app: &adw::Application,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
    area: &Rc<SessionArea>,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    existing: Option<Host>,
) {
    let is_edit = existing.is_some();
    let alias_row = adw::EntryRow::builder().title("Alias").build();
    let host_row = adw::EntryRow::builder().title("Värdnamn/IP").build();
    let user_row = adw::EntryRow::builder().title("Användare").build();
    let port_row = adw::EntryRow::builder().title("Port").text("22").build();

    if let Some(h) = &existing {
        alias_row.set_text(&h.alias);
        host_row.set_text(&h.host_name);
        user_row.set_text(&h.user);
        port_row.set_text(&h.port.to_string());
    }

    let group = adw::PreferencesGroup::new();
    group.add(&alias_row);
    group.add(&host_row);
    group.add(&user_row);
    group.add(&port_row);

    let page = adw::PreferencesPage::new();
    page.add(&group);

    let save_button = gtk::Button::with_label(if is_edit { "Spara" } else { "Lägg till" });
    save_button.add_css_class("suggested-action");

    let cancel_button = gtk::Button::with_label("Avbryt");

    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(420)
        .default_height(360)
        .content(&content)
        .build();

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));

    save_button.connect_clicked(clone!(
        #[strong]
        store,
        #[weak]
        list,
        #[weak]
        app,
        #[strong]
        area,
        #[strong]
        settings_store,
        #[strong]
        snippet_store,
        #[weak]
        win,
        #[strong]
        existing,
        move |_| {
            let alias = alias_row.text().to_string();
            let host_name = host_row.text().to_string();
            let user = user_row.text().to_string();
            let port: i64 = port_row.text().parse().unwrap_or(22);
            if alias.is_empty() || host_name.is_empty() || user.is_empty() {
                return; // formuläret kräver alias/värdnamn/användare
            }
            let host = if let Some(mut h) = existing.clone() {
                h.alias = alias;
                h.host_name = host_name;
                h.user = user;
                h.port = port;
                h
            } else {
                let mut h = Host::new(alias, host_name, user);
                h.port = port;
                h
            };
            store.borrow_mut().upsert(host).expect("kunde inte spara värden");
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store);
            win.close();
        }
    ));

    win.present();
}

/// Funktioner-inställningar: just nu bara Docker-togglen (det uttryckligen
/// namngivna kravet) — Snippets/Kommandobibliotek/SFTP/portvidarebefordran/
/// SSH-nyckeldistribution har ingen vy att gömma i LinuxApp än (se
/// ROADMAP.md), så deras fält finns i `settings::FeatureToggles` (för att
/// inte tappa en delad settings.json-fils övriga värden) men saknar UI här.
fn show_settings_dialog(
    app: &adw::Application,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
    area: &Rc<SessionArea>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
) {
    let current = settings_store.borrow().current();

    let docker_row = adw::SwitchRow::builder().title("Docker").subtitle("Visa Docker-knappen på värdar").active(current.show_docker).build();
    let commands_row = adw::SwitchRow::builder()
        .title("Kommandobibliotek")
        .subtitle("Visa Kommandon-knappen på värdar")
        .active(current.show_command_library)
        .build();
    let sftp_row = adw::SwitchRow::builder()
        .title("Filer (SFTP)")
        .subtitle("Visa Filer-knappen på värdar")
        .active(current.show_sftp_browser)
        .build();

    let group = adw::PreferencesGroup::builder().title("Funktioner").build();
    group.add(&docker_row);
    group.add(&commands_row);
    group.add(&sftp_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let close_button = gtk::Button::with_label("Klar");
    close_button.add_css_class("suggested-action");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_end(&close_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(420)
        .default_height(260)
        .title("Inställningar")
        .content(&content)
        .build();

    docker_row.connect_active_notify(clone!(
        #[strong]
        settings_store,
        #[strong]
        store,
        #[weak]
        list,
        #[strong]
        app,
        #[strong]
        area,
        #[strong]
        snippet_store,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_docker = row.is_active();
            settings_store
                .borrow_mut()
                .update(toggles)
                .expect("kunde inte spara inställningarna");
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store);
        }
    ));

    commands_row.connect_active_notify(clone!(
        #[strong]
        settings_store,
        #[strong]
        store,
        #[weak]
        list,
        #[strong]
        app,
        #[strong]
        area,
        #[strong]
        snippet_store,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_command_library = row.is_active();
            settings_store
                .borrow_mut()
                .update(toggles)
                .expect("kunde inte spara inställningarna");
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store);
        }
    ));

    sftp_row.connect_active_notify(clone!(
        #[strong]
        settings_store,
        #[strong]
        store,
        #[weak]
        list,
        #[strong]
        app,
        #[strong]
        area,
        #[strong]
        snippet_store,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_sftp_browser = row.is_active();
            settings_store
                .borrow_mut()
                .update(toggles)
                .expect("kunde inte spara inställningarna");
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store);
        }
    ));

    close_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));

    win.present();
}

/// Samlar flikvyn (en session per flik) + platshållaren som visas när inga
/// flikar är öppna. Touchscreen-svep mellan flikar via en `GestureSwipe`
/// ovanpå hela ytan (samma UX-mål som iOS MultiSessionView, commit 5a82141 —
/// olöst för gamla SwiftCrossUI-Linux, se project-bastion-linuxapp-touchscreen-goal).
struct SessionArea {
    overlay: gtk::Overlay,
    tab_view: adw::TabView,
    tab_bar: adw::TabBar,
    placeholder: adw::StatusPage,
}

impl SessionArea {
    fn new() -> Rc<Self> {
        let tab_view = adw::TabView::new();
        let tab_bar = adw::TabBar::builder().autohide(false).build();
        tab_bar.set_view(Some(&tab_view));

        let placeholder = adw::StatusPage::builder()
            .title("Ingen session öppen")
            .description("Välj en värd i listan för att ansluta")
            .icon_name("network-server-symbolic")
            .vexpand(true)
            .hexpand(true)
            .build();

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&tab_view));
        overlay.add_overlay(&placeholder);

        let swipe = gtk::GestureSwipe::new();
        swipe.connect_swipe(clone!(
            #[weak]
            tab_view,
            move |_, vx, vy| {
                const THRESHOLD: f64 = 400.0; // px/s, kräver ett bestämt svep, inte en darrning
                if vx.abs() > THRESHOLD && vx.abs() > vy.abs() {
                    if vx < 0.0 {
                        tab_view.select_next_page();
                    } else {
                        tab_view.select_previous_page();
                    }
                }
            }
        ));
        overlay.add_controller(swipe);

        let area = Rc::new(SessionArea { overlay, tab_view, tab_bar, placeholder });
        area.update_placeholder();

        area.tab_view.connect_close_page(clone!(
            #[strong]
            area,
            move |_, page| {
                if let Some(terminal) = page.child().downcast_ref::<vte::Terminal>() {
                    // Att droppa sändaren stänger av bakgrundstrådens select-loop
                    // (input_rx.recv() → Err → run() returnerar), vilket i sin
                    // tur stänger SSH-anslutningen rent istället för att lämna
                    // den övergiven i bakgrunden efter att fliken stängts.
                    unsafe {
                        terminal.steal_data::<async_channel::Sender<Vec<u8>>>("bastion-ssh-input");
                    }
                }
                area.tab_view.close_page_finish(page, true);
                area.update_placeholder();
                glib::Propagation::Stop
            }
        ));

        area
    }

    fn update_placeholder(&self) {
        self.placeholder.set_visible(self.tab_view.n_pages() == 0);
    }
}

/// Öppnar en riktig SSH-session för `host` i en ny flik. Exit/Ctrl+D i
/// fjärrskalet stänger fliken automatiskt (samma UX-mål som iOS, se
/// commit 4e9270b).
fn open_session(area: &Rc<SessionArea>, host: host::Host) {
    if matches!(host.auth, host::HostAuth::AskPassword) {
        prompt_password_then(area, host, |area, host, password| {
            start_session(area, host, Some(password))
        });
    } else {
        start_session(area, host, None);
    }
}

/// Ger `on_password` antingen direkt (`None`, ingen prompt behövs) eller
/// efter att användaren skrivit in ett lösenord i en dialog — återanvänds av
/// både terminalsessioner och Docker-vyn, båda kan hamna på en
/// `AskPassword`-värd.
fn require_password(
    area: &Rc<SessionArea>,
    host: host::Host,
    on_password: impl Fn(&Rc<SessionArea>, host::Host, Option<String>) + 'static,
) {
    if matches!(host.auth, host::HostAuth::AskPassword) {
        prompt_password_then(area, host, move |area, host, password| {
            on_password(area, host, Some(password))
        });
    } else {
        on_password(area, host, None);
    }
}

fn prompt_password_then(
    area: &Rc<SessionArea>,
    host: host::Host,
    on_password: impl Fn(&Rc<SessionArea>, host::Host, String) + 'static,
) {
    let entry = gtk::PasswordEntry::builder().show_peek_icon(true).hexpand(true).build();
    let group = adw::PreferencesGroup::builder().title(format!("Lösenord för {}@{}", host.user, host.host_name)).build();
    group.add(&entry);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let connect_button = gtk::Button::with_label("Anslut");
    connect_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&connect_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(360)
        .default_height(180)
        .content(&content)
        .build();

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    connect_button.connect_clicked(clone!(
        #[weak]
        win,
        #[strong]
        area,
        #[strong]
        host,
        #[weak]
        entry,
        move |_| {
            let password = entry.text().to_string();
            win.close();
            on_password(&area, host.clone(), password);
        }
    ));

    win.present();
}

fn start_session(area: &Rc<SessionArea>, host: host::Host, password: Option<String>) {
    let terminal = vte::Terminal::builder().vexpand(true).hexpand(true).build();

    let cols = 80u32;
    let rows = 24u32;
    let session = ssh::spawn_shell(host.clone(), password, cols, rows);

    // Lagras på widgeten så close-page-hanteraren kan droppa sändaren och
    // därmed stänga SSH-anslutningen när fliken stängs manuellt.
    unsafe {
        terminal.set_data("bastion-ssh-input", session.input.clone());
    }

    let page = area.tab_view.append(&terminal);
    page.set_title(&host.alias);
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    terminal.connect_commit(clone!(
        #[strong(rename_to = input)]
        session.input,
        move |_, text, _| {
            let _ = input.try_send(text.as_bytes().to_vec());
        }
    ));

    glib::spawn_future_local(clone!(
        #[weak]
        terminal,
        #[strong]
        area,
        #[weak]
        page,
        #[strong(rename_to = output)]
        session.output,
        async move {
            while let Ok(event) = output.recv().await {
                match event {
                    SshEvent::Data(bytes) => terminal.feed(&bytes),
                    SshEvent::Error(msg) => {
                        terminal.feed(format!("\r\n\x1b[31m[bastion] fel: {msg}\x1b[0m\r\n").as_bytes());
                    }
                    SshEvent::Connected => {}
                    SshEvent::Closed => {
                        if area.tab_view.page_position(&page) >= 0 {
                            area.tab_view.close_page(&page);
                        }
                        break;
                    }
                }
            }
        }
    ));
}

/// Öppnar Docker-vyn för `host` i en ny flik: en containerlista med
/// start/stopp/omstart/loggar/shell per rad. Port av App/DockerView.swift
/// till en fristående SSH-engångskörning per anrop (`ssh::run_command`).
fn open_docker_view(area: &Rc<SessionArea>, host: host::Host, password: Option<String>) {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();

    let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some("Uppdatera"));
    let toolbar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).margin_start(12).margin_end(12).margin_top(8).build();
    toolbar.append(&gtk::Label::builder().label(format!("Docker: {}", host.alias)).hexpand(true).halign(gtk::Align::Start).build());
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Docker: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    refresh_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[weak]
        list,
        move |_| refresh_docker_list(&area, host.clone(), password.clone(), &list)
    ));

    refresh_docker_list(area, host, password, &list);
}

fn refresh_docker_list(area: &Rc<SessionArea>, host: host::Host, password: Option<String>, list: &gtk::ListBox) {
    let rx = ssh::run_command(host.clone(), password.clone(), docker::list_command(true));
    glib::spawn_future_local(clone!(
        #[weak]
        list,
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        async move {
            while let Some(row) = list.row_at_index(0) {
                list.remove(&row);
            }
            match rx.recv().await {
                Ok(Ok(output)) => {
                    for container in docker::parse_list(&output) {
                        list.append(&build_container_row(&area, &host, &password, &list, container));
                    }
                }
                Ok(Err(e)) => list.append(&error_row(&e)),
                Err(_) => list.append(&error_row("SSH-anslutningen avbröts oväntat")),
            }
        }
    ));
}

fn error_row(message: &str) -> adw::ActionRow {
    adw::ActionRow::builder().title("Fel").subtitle(message).build()
}

fn build_container_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    list: &gtk::ListBox,
    container: docker::DockerContainer,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&container.name)
        .subtitle(format!("{} — {}", container.image, container.status))
        .build();

    let suffix = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(4).valign(gtk::Align::Center).build();

    let run_docker_action = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let list = list.clone();
        move |command: Result<String, String>| {
            let Ok(command) = command else { return };
            let rx = ssh::run_command(host.clone(), password.clone(), command);
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                host,
                #[strong]
                password,
                #[strong]
                list,
                async move {
                    let _ = rx.recv().await;
                    refresh_docker_list(&area, host, password, &list);
                }
            ));
        }
    };

    if container.is_running() {
        let stop_btn = gtk::Button::from_icon_name("media-playback-stop-symbolic");
        stop_btn.set_tooltip_text(Some("Stoppa"));
        stop_btn.connect_clicked(clone!(
            #[strong(rename_to = run)]
            run_docker_action,
            #[strong]
            container,
            move |_| run(docker::stop_command(&container.id))
        ));
        let restart_btn = gtk::Button::from_icon_name("view-refresh-symbolic");
        restart_btn.set_tooltip_text(Some("Starta om"));
        restart_btn.connect_clicked(clone!(
            #[strong(rename_to = run)]
            run_docker_action,
            #[strong]
            container,
            move |_| run(docker::restart_command(&container.id))
        ));
        let shell_btn = gtk::Button::from_icon_name("utilities-terminal-symbolic");
        shell_btn.set_tooltip_text(Some("Shell"));
        shell_btn.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            host,
            #[strong]
            password,
            #[strong]
            container,
            move |_| {
                if let Ok(cmd) = docker::exec_shell_command(&container.id) {
                    // `startup_command` skickas automatiskt in i shellen direkt
                    // efter att den öppnats (se ssh::run) — samma mekanism som
                    // Host.startupCommand i Swift, bara återanvänd här för
                    // `docker exec` istället för ett vanligt inloggningsskal.
                    let mut shell_host = host.clone();
                    shell_host.startup_command = Some(cmd);
                    shell_host.alias = format!("{}: {}", host.alias, container.name);
                    start_session(&area, shell_host, password.clone());
                }
            }
        ));
        suffix.append(&stop_btn);
        suffix.append(&restart_btn);
        suffix.append(&shell_btn);
    } else {
        let start_btn = gtk::Button::from_icon_name("media-playback-start-symbolic");
        start_btn.set_tooltip_text(Some("Starta"));
        start_btn.connect_clicked(clone!(
            #[strong(rename_to = run)]
            run_docker_action,
            #[strong]
            container,
            move |_| run(docker::start_command(&container.id))
        ));
        suffix.append(&start_btn);
    }

    let logs_btn = gtk::Button::from_icon_name("text-x-generic-symbolic");
    logs_btn.set_tooltip_text(Some("Loggar"));
    logs_btn.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        container,
        move |_| show_docker_logs(&area, &host, &password, &container)
    ));
    suffix.append(&logs_btn);

    row.add_suffix(&suffix);
    row
}

fn show_docker_logs(area: &Rc<SessionArea>, host: &host::Host, password: &Option<String>, container: &docker::DockerContainer) {
    let Ok(cmd) = docker::logs_command(&container.id, 200) else { return };
    let text_view = gtk::TextView::builder().editable(false).monospace(true).build();
    let scrolled = gtk::ScrolledWindow::builder().child(&text_view).vexpand(true).build();
    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(700)
        .default_height(500)
        .title(format!("Loggar: {}", container.name))
        .content(&scrolled)
        .build();
    win.present();

    let rx = ssh::run_command(host.clone(), password.clone(), cmd);
    glib::spawn_future_local(clone!(
        #[weak]
        text_view,
        async move {
            let text = match rx.recv().await {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => format!("Fel: {e}"),
                Err(_) => "SSH-anslutningen avbröts oväntat".to_string(),
            };
            text_view.buffer().set_text(&text);
        }
    ));
}

/// Öppnar Kommandobibliotek+Snippets-vyn för `host` i en ny flik: statiska
/// referenskommandon (`command_library.rs`) + användarens egna sparade
/// snippets (`snippet.rs`). Port av App/CommandLibraryView.swift.
fn open_command_library_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
) {
    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();

    let add_button = gtk::Button::from_icon_name("list-add-symbolic");
    add_button.set_tooltip_text(Some("Ny snippet"));
    let toolbar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).margin_start(12).margin_end(12).margin_top(8).build();
    toolbar.append(&gtk::Label::builder().label(format!("Kommandon: {}", host.alias)).hexpand(true).halign(gtk::Align::Start).build());
    toolbar.append(&add_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Kommandon: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    add_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        snippet_store,
        #[weak]
        list,
        move |_| show_snippet_edit_dialog(&area, host.clone(), password.clone(), &snippet_store, &list, None)
    ));

    refresh_command_library_list(area, &host, &password, snippet_store, &list);
}

fn refresh_command_library_list(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    list: &gtk::ListBox,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }

    for s in snippet_store.borrow().all() {
        list.append(&build_snippet_row(area, host, password, snippet_store, s.clone(), list));
    }
    for entry in command_library::all() {
        list.append(&build_library_entry_row(area, host, password, entry));
    }
}

fn build_snippet_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    snippet: snippet::Snippet,
    list: &gtk::ListBox,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(&snippet.name).subtitle(&snippet.template).build();
    let suffix = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(4).valign(gtk::Align::Center).build();

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
        snippet,
        move |_| run_snippet(&area, host.clone(), password.clone(), snippet.clone())
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
        snippet_store,
        #[weak]
        list,
        #[strong]
        snippet,
        move |_| show_snippet_edit_dialog(&area, host.clone(), password.clone(), &snippet_store, &list, Some(snippet.clone()))
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
        snippet_store,
        #[weak]
        list,
        #[strong(rename_to = snippet_id)]
        snippet.id,
        move |_| {
            snippet_store.borrow_mut().delete(snippet_id).expect("kunde inte ta bort snippeten");
            refresh_command_library_list(&area, &host, &password, &snippet_store, &list);
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
) -> adw::ActionRow {
    let mut subtitle = format!("[{}] {}", entry.category.label(), entry.summary);
    if let Some(example) = entry.example {
        subtitle.push_str(&format!(" — t.ex. {example}"));
    }
    let row = adw::ActionRow::builder().title(entry.command).subtitle(subtitle).build();

    let suffix = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(4).valign(gtk::Align::Center).build();

    if let Some(docs_url) = entry.docs_url {
        let docs_button = gtk::Button::from_icon_name("help-about-symbolic");
        docs_button.set_tooltip_text(Some("Dokumentation"));
        docs_button.connect_clicked(move |_| {
            gtk::gio::AppInfo::launch_default_for_uri(docs_url, gtk::gio::AppLaunchContext::NONE).ok();
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
        move |_| {
            let snippet = snippet::Snippet::new(entry.summary.to_string(), entry.command.to_string());
            run_snippet(&area, host.clone(), password.clone(), snippet);
        }
    ));
    suffix.append(&run_button);
    row.add_suffix(&suffix);
    row
}

/// Kör en snippet: fyller i `{{variabler}}` via en dialog om det finns
/// några, annars öppnar direkt en ny terminalflik med det rendrade
/// kommandot som `startup_command` (samma mönster som Docker-shell).
fn run_snippet(area: &Rc<SessionArea>, host: host::Host, password: Option<String>, snippet: snippet::Snippet) {
    if snippet.variable_names().is_empty() {
        launch_rendered_command(area, host, password, &snippet.name, snippet.rendered(&std::collections::HashMap::new()));
    } else {
        prompt_snippet_variables(area, host, password, snippet);
    }
}

fn launch_rendered_command(area: &Rc<SessionArea>, host: host::Host, password: Option<String>, title_suffix: &str, command: String) {
    let mut h = host;
    h.startup_command = Some(command);
    h.alias = format!("{}: {title_suffix}", h.alias);
    start_session(area, h, password);
}

fn prompt_snippet_variables(area: &Rc<SessionArea>, host: host::Host, password: Option<String>, snippet: snippet::Snippet) {
    let names = snippet.variable_names();
    let group = adw::PreferencesGroup::builder().title(&snippet.name).description(&snippet.template).build();
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
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&run_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(420)
        .default_height(320)
        .title("Fyll i kommandot")
        .content(&content)
        .build();

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
        snippet,
        move |_| {
            let values: std::collections::HashMap<String, String> =
                entries.iter().map(|(name, row)| (name.clone(), row.text().to_string())).collect();
            let rendered = snippet.rendered(&values);
            win.close();
            launch_rendered_command(&area, host.clone(), password.clone(), &snippet.name, rendered);
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
) {
    let is_edit = existing.is_some();
    let name_row = adw::EntryRow::builder().title("Namn").build();
    let template_row = adw::EntryRow::builder().title("Kommando (t.ex. docker restart {{service}})").build();
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
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(420)
        .default_height(240)
        .title("Snippet")
        .content(&content)
        .build();

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
            snippet_store.borrow_mut().upsert(snippet).expect("kunde inte spara snippeten");
            refresh_command_library_list(&area, &host, &password, &snippet_store, &list);
            win.close();
        }
    ));

    win.present();
}

/// Öppnar SFTP-bläddraren för `host` i en ny flik. Port av
/// App/SFTPBrowserModel.swift (kärnfunktioner — se sftp.rs för vad som
/// medvetet är uppskjutet: chmod/chown/komprimera/packa upp).
fn open_sftp_view(area: &Rc<SessionArea>, host: host::Host, password: Option<String>) {
    let handle = sftp::spawn(host.clone(), password);
    let current_path = Rc::new(RefCell::new(".".to_string()));

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder().child(&list).vexpand(true).build();

    let path_label = gtk::Label::builder().halign(gtk::Align::Start).hexpand(true).build();
    let up_button = gtk::Button::from_icon_name("go-up-symbolic");
    up_button.set_tooltip_text(Some("Upp en nivå"));
    let mkdir_button = gtk::Button::from_icon_name("folder-new-symbolic");
    mkdir_button.set_tooltip_text(Some("Ny mapp"));
    let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");

    let toolbar = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(4).margin_start(12).margin_end(12).margin_top(8).build();
    toolbar.append(&up_button);
    toolbar.append(&path_label);
    toolbar.append(&mkdir_button);
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Filer: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    up_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            let mut path = current_path.borrow_mut();
            if *path != "." {
                *path = match path.rfind('/') {
                    Some(slash) => path[..slash].to_string(),
                    None => ".".to_string(),
                };
                let new_path = path.clone();
                drop(path);
                refresh_sftp_list(&area, handle.clone(), current_path.clone(), new_path, &list, &path_label);
            }
        }
    ));

    mkdir_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| prompt_new_folder_name(&area, handle.clone(), current_path.clone(), &list, &path_label)
    ));

    refresh_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            let path = current_path.borrow().clone();
            refresh_sftp_list(&area, handle.clone(), current_path.clone(), path, &list, &path_label);
        }
    ));

    let initial_path = current_path.borrow().clone();
    refresh_sftp_list(area, handle, current_path, initial_path, &list, &path_label);
}

fn refresh_sftp_list(
    area: &Rc<SessionArea>,
    handle: sftp::SftpHandle,
    current_path: Rc<RefCell<String>>,
    path: String,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    path_label.set_text(&path);
    glib::spawn_future_local(clone!(
        #[strong]
        area,
        #[weak]
        list,
        #[weak]
        path_label,
        async move {
            while let Some(row) = list.row_at_index(0) {
                list.remove(&row);
            }
            match handle.list(path.clone()).await {
                Ok(entries) => {
                    for entry in entries {
                        list.append(&build_sftp_entry_row(&area, handle.clone(), current_path.clone(), path.clone(), entry, &list, &path_label));
                    }
                }
                Err(e) => list.append(&error_row(&e)),
            }
        }
    ));
}

fn joined_path(base: &str, name: &str) -> String {
    if base == "." {
        name.to_string()
    } else {
        format!("{base}/{name}")
    }
}

fn build_sftp_entry_row(
    area: &Rc<SessionArea>,
    handle: sftp::SftpHandle,
    current_path: Rc<RefCell<String>>,
    path: String,
    entry: sftp::Entry,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) -> adw::ActionRow {
    let subtitle = if entry.is_dir { "Mapp".to_string() } else { format!("{} bytes", entry.size) };
    let row = adw::ActionRow::builder().title(&entry.name).subtitle(subtitle).activatable(true).build();
    let icon = gtk::Image::from_icon_name(if entry.is_dir { "folder-symbolic" } else { "text-x-generic-symbolic" });
    row.add_prefix(&icon);

    row.connect_activated(clone!(
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        #[strong]
        entry,
        #[strong]
        path,
        move |_| {
            let full_path = joined_path(&path, &entry.name);
            if entry.is_dir {
                *current_path.borrow_mut() = full_path.clone();
                refresh_sftp_list(&area, handle.clone(), current_path.clone(), full_path, &list, &path_label);
            } else {
                open_sftp_file_editor(&area, handle.clone(), full_path);
            }
        }
    ));

    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some("Ta bort"));
    delete_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        #[strong]
        entry,
        #[strong]
        path,
        move |_| {
            let full_path = joined_path(&path, &entry.name);
            let is_dir = entry.is_dir;
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                handle,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                #[strong]
                path,
                async move {
                    let result = if is_dir { handle.remove_dir(full_path).await } else { handle.remove_file(full_path).await };
                    if let Err(e) = result {
                        list.append(&error_row(&e));
                        return;
                    }
                    refresh_sftp_list(&area, handle.clone(), current_path, path, &list, &path_label);
                }
            ));
        }
    ));

    let rename_button = gtk::Button::from_icon_name("document-edit-symbolic");
    rename_button.set_tooltip_text(Some("Döp om"));
    rename_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        #[strong]
        entry,
        #[strong]
        path,
        move |_| prompt_rename(&area, handle.clone(), current_path.clone(), path.clone(), entry.clone(), &list, &path_label)
    ));

    row.add_suffix(&rename_button);
    row.add_suffix(&delete_button);
    row
}

fn prompt_new_folder_name(area: &Rc<SessionArea>, handle: sftp::SftpHandle, current_path: Rc<RefCell<String>>, list: &gtk::ListBox, path_label: &gtk::Label) {
    let name_row = adw::EntryRow::builder().title("Mappnamn").build();
    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let create_button = gtk::Button::with_label("Skapa");
    create_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&create_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(360)
        .default_height(160)
        .title("Ny mapp")
        .content(&content)
        .build();

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    create_button.connect_clicked(clone!(
        #[weak]
        win,
        #[strong]
        area,
        #[strong]
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            let name = name_row.text().to_string();
            if name.is_empty() {
                return;
            }
            win.close();
            let base = current_path.borrow().clone();
            let full_path = joined_path(&base, &name);
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                handle,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = handle.mkdir(full_path).await {
                        list.append(&error_row(&e));
                        return;
                    }
                    let path = current_path.borrow().clone();
                    refresh_sftp_list(&area, handle.clone(), current_path, path, &list, &path_label);
                }
            ));
        }
    ));

    win.present();
}

fn prompt_rename(
    area: &Rc<SessionArea>,
    handle: sftp::SftpHandle,
    current_path: Rc<RefCell<String>>,
    path: String,
    entry: sftp::Entry,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    let name_row = adw::EntryRow::builder().title("Nytt namn").text(&entry.name).build();
    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let save_button = gtk::Button::with_label("Döp om");
    save_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(360)
        .default_height(160)
        .title("Döp om")
        .content(&content)
        .build();

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
        handle,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            let new_name = name_row.text().to_string();
            if new_name.is_empty() {
                return;
            }
            win.close();
            let from = joined_path(&path, &entry.name);
            let to = joined_path(&path, &new_name);
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                handle,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = handle.rename(from, to).await {
                        list.append(&error_row(&e));
                        return;
                    }
                    let path = current_path.borrow().clone();
                    refresh_sftp_list(&area, handle.clone(), current_path, path, &list, &path_label);
                }
            ));
        }
    ));

    win.present();
}

/// Läser filen och visar den redigerbar om innehållet är giltig UTF-8 —
/// annars en tydlig platshållartext (samma "spara MÅSTE vara avstängt för
/// binärt innehåll"-lärdom som Swiftsidans `EditingFile.isBinary`).
fn open_sftp_file_editor(area: &Rc<SessionArea>, handle: sftp::SftpHandle, path: String) {
    let text_view = gtk::TextView::builder().monospace(true).build();
    let scrolled = gtk::ScrolledWindow::builder().child(&text_view).vexpand(true).build();

    let save_button = gtk::Button::with_label("Spara");
    save_button.add_css_class("suggested-action");
    save_button.set_sensitive(false);
    let close_button = gtk::Button::with_label("Stäng");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).title_widget(&gtk::Label::new(Some(&path))).build();
    header.pack_start(&close_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&scrolled);

    let win = adw::Window::builder()
        .transient_for(&area.overlay.root().and_downcast::<gtk::Window>().expect("inget fönster"))
        .modal(true)
        .default_width(700)
        .default_height(500)
        .content(&content)
        .build();
    win.present();

    close_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    save_button.connect_clicked(clone!(
        #[strong]
        handle,
        #[strong]
        path,
        #[weak]
        text_view,
        move |_| {
            let buffer = text_view.buffer();
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
            glib::spawn_future_local(clone!(
                #[strong]
                handle,
                #[strong]
                path,
                async move {
                    let _ = handle.write(path, text.into_bytes()).await;
                }
            ));
        }
    ));

    glib::spawn_future_local(clone!(
        #[weak]
        text_view,
        #[weak]
        save_button,
        async move {
            match handle.read(path).await {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(text) => {
                        text_view.buffer().set_text(&text);
                        save_button.set_sensitive(true);
                    }
                    Err(e) => {
                        text_view.buffer().set_text(&format!(
                            "(binärt innehåll, {} bytes — kan inte visas eller redigeras som text)",
                            e.into_bytes().len()
                        ));
                        text_view.set_editable(false);
                    }
                },
                Err(e) => text_view.buffer().set_text(&format!("Fel: {e}")),
            }
        }
    ));
}
