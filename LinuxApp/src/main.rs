use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use std::cell::RefCell;
use std::rc::Rc;
use uuid::Uuid;
use vte::prelude::*;

mod host;
mod ssh;

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

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    refresh_list(&list, &store, app);

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
        move |_| show_host_dialog(&app, &store, &list, None)
    ));

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.pack_end(&add_button);

    let sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_content.append(&sidebar_header);
    sidebar_content.append(&scrolled);

    let sidebar_page = adw::NavigationPage::builder()
        .title("Värdar")
        .child(&sidebar_content)
        .build();

    let area = SessionArea::new();

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
fn refresh_list(list: &gtk::ListBox, store: &Rc<RefCell<HostStore>>, app: &adw::Application) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
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
        let menu = gio_menu_for(h.id);
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
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                if let Some(host) = host {
                    show_host_dialog(&app, &store, &list, Some(host));
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
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                store.borrow_mut().delete(host_id).expect("kunde inte ta bort värden");
                refresh_list(&list, &store, &app);
            }
        ));
        action_group.add_action(&edit_action);
        action_group.add_action(&delete_action);
        row.insert_action_group("host", Some(&action_group));

        list.append(&row);
    }
}

fn gio_menu_for(_host_id: Uuid) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Redigera"), Some("host.edit"));
    menu.append(Some("Ta bort"), Some("host.delete"));
    menu
}

/// Lägg till/redigera-dialogen. `existing = None` skapar en ny värd.
fn show_host_dialog(
    app: &adw::Application,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
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
            refresh_list(&list, &store, &app);
            win.close();
        }
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
        prompt_password(area, host);
    } else {
        start_session(area, host, None);
    }
}

fn prompt_password(area: &Rc<SessionArea>, host: host::Host) {
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
            start_session(&area, host.clone(), Some(password));
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
