use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use std::cell::RefCell;
use std::rc::Rc;
use vte::prelude::*;

mod archive;
mod command_library;
mod docker;
mod fsutil;
mod host;
mod key_deploy;
mod known_hosts;
mod port_forward;
mod s3;
mod settings;
mod sftp;
mod snippet;
mod socks_proxy;
mod ssh;
mod sync;
mod sync_crypto;
mod tailscale;
mod telnet;
mod wake_on_lan;
mod wireguard;

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
    let settings_store = Rc::new(RefCell::new(
        settings::AppSettingsStore::open(settings::AppSettingsStore::default_path())
            .expect("kunde inte öppna inställningsfilen"),
    ));
    let snippet_store = Rc::new(RefCell::new(
        snippet::SnippetStore::open(snippet::SnippetStore::default_path())
            .expect("kunde inte öppna snippet-databasen"),
    ));
    let wireguard_store = Rc::new(RefCell::new(
        wireguard::WireGuardProfileStore::open(wireguard::WireGuardProfileStore::default_path())
            .expect("kunde inte öppna wireguard-profildatabasen"),
    ));
    let s3_store = Rc::new(RefCell::new(
        s3::S3ConnectionStore::open(s3::S3ConnectionStore::default_path())
            .expect("kunde inte öppna s3-anslutningsdatabasen"),
    ));
    let sync_config = Rc::new(RefCell::new(sync::SyncConfig::load(
        &sync::SyncConfig::default_path(),
    )));

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

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();

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
        move |_| show_host_dialog(
            &app,
            &store,
            &list,
            &area,
            &settings_store,
            &snippet_store,
            None
        )
    ));

    let quick_connect_button = gtk::Button::from_icon_name("system-run-symbolic");
    quick_connect_button.set_tooltip_text(Some("Snabbanslutning"));
    quick_connect_button.connect_clicked(clone!(
        #[weak]
        app,
        #[strong]
        area,
        move |_| show_quick_connect_dialog(&app, &area)
    ));

    let telnet_button = gtk::Button::from_icon_name("utilities-terminal-symbolic");
    telnet_button.set_tooltip_text(Some("Telnet"));
    telnet_button.connect_clicked(clone!(
        #[weak]
        app,
        #[strong]
        area,
        move |_| show_telnet_connect_dialog(&app, &area)
    ));

    let tailscale_button = gtk::Button::from_icon_name("network-workgroup-symbolic");
    tailscale_button.set_tooltip_text(Some("Tailscale"));
    tailscale_button.connect_clicked(clone!(
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
        move |_| show_tailscale_discovery_dialog(
            &app,
            &store,
            &list,
            &area,
            &settings_store,
            &snippet_store
        )
    ));

    let wireguard_button = gtk::Button::from_icon_name("network-vpn-symbolic");
    wireguard_button.set_tooltip_text(Some("WireGuard-profiler"));
    wireguard_button.connect_clicked(clone!(
        #[weak]
        app,
        #[strong]
        wireguard_store,
        move |_| show_wireguard_profile_list(&app, &wireguard_store)
    ));

    let s3_button = gtk::Button::from_icon_name("folder-remote-symbolic");
    s3_button.set_tooltip_text(Some("S3-anslutningar"));
    s3_button.connect_clicked(clone!(
        #[weak]
        app,
        #[strong]
        area,
        #[strong]
        s3_store,
        move |_| show_s3_connection_list(&app, &area, &s3_store)
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
        #[strong]
        sync_config,
        move |_| show_settings_dialog(
            &app,
            &settings_store,
            &store,
            &list,
            &area,
            &snippet_store,
            &sync_config
        )
    ));

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.pack_end(&add_button);
    sidebar_header.pack_end(&quick_connect_button);
    sidebar_header.pack_end(&telnet_button);
    sidebar_header.pack_end(&tailscale_button);
    sidebar_header.pack_end(&wireguard_button);
    sidebar_header.pack_end(&s3_button);
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
            if let Some(host) = store
                .borrow()
                .all()
                .get(index as usize)
                .map(|h| (*h).clone())
            {
                open_session(&area, &store, host);
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
        let menu = gio_menu_for(&toggles, &h.mac_address);
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
                let host = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .map(|h| (*h).clone());
                if let Some(host) = host {
                    show_host_dialog(
                        &app,
                        &store,
                        &list,
                        &area,
                        &settings_store,
                        &snippet_store,
                        Some(host),
                    );
                }
            }
        ));
        let wake_action = gtk::gio::SimpleAction::new("wake", None);
        wake_action.connect_activate(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let mac = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .and_then(|h| h.mac_address.clone());
                let Some(mac) = mac else { return };
                let rx = wake_on_lan::spawn_send(mac, "255.255.255.255".to_string(), 9);
                glib::spawn_future_local(clone!(
                    #[strong]
                    area,
                    async move {
                        match rx.recv().await {
                            Ok(Ok(())) => show_message_dialog(
                                &area,
                                "Wake-on-LAN",
                                "Magic packet skickat.",
                            ),
                            Ok(Err(e)) => show_message_dialog(
                                &area,
                                "Wake-on-LAN",
                                &format!("Kunde inte skicka magic packet: {e}"),
                            ),
                            Err(_) => show_message_dialog(
                                &area,
                                "Wake-on-LAN",
                                "Kanalen stängdes oväntat",
                            ),
                        }
                    }
                ));
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
                // En `.expect` här kraschade tidigare HELA appen (och alla
                // öppna SSH-sessioner) om t.ex. disken var full eller
                // `~/.bastion` skrivskyddad (CodeRabbit-fynd) — en
                // återhämtningsbar I/O-miss ska inte vara ödesdiger.
                if let Err(e) = store.borrow_mut().delete(host_id) {
                    eprintln!("kunde inte ta bort värden: {e}");
                    return;
                }
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
                let host = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .map(|h| (*h).clone());
                if let Some(host) = host {
                    with_resolved_jump(&area, &store, host, clone!(
                        #[strong]
                        area,
                        move |host, jump| {
                            require_password(&area, host, move |area, host, password| {
                                open_docker_view(area, host, password, jump.clone())
                            });
                        }
                    ));
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
                let host = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .map(|h| (*h).clone());
                if let Some(host) = host {
                    let snippet_store = snippet_store.clone();
                    with_resolved_jump(&area, &store, host, clone!(
                        #[strong]
                        area,
                        move |host, jump| {
                            require_password(&area, host, move |area, host, password| {
                                open_command_library_view(
                                    area,
                                    host,
                                    password,
                                    &snippet_store,
                                    jump.clone(),
                                )
                            });
                        }
                    ));
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
                let host = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .map(|h| (*h).clone());
                if let Some(host) = host {
                    with_resolved_jump(&area, &store, host, clone!(
                        #[strong]
                        area,
                        move |host, jump| {
                            require_password(&area, host, move |area, host, password| {
                                open_sftp_view(area, host, password, jump.clone())
                            });
                        }
                    ));
                }
            }
        ));
        let forward_action = gtk::gio::SimpleAction::new("forward", None);
        forward_action.connect_activate(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .map(|h| (*h).clone());
                if let Some(host) = host {
                    with_resolved_jump(&area, &store, host, clone!(
                        #[strong]
                        area,
                        move |host, jump| {
                            require_password(&area, host, move |area, host, password| {
                                open_port_forward_view(area, host, password, jump.clone())
                            });
                        }
                    ));
                }
            }
        ));
        let key_deploy_action = gtk::gio::SimpleAction::new("key_deploy", None);
        key_deploy_action.connect_activate(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong(rename_to = host_id)]
            h.id,
            move |_, _| {
                let host = store
                    .borrow()
                    .all()
                    .iter()
                    .find(|x| x.id == host_id)
                    .map(|h| (*h).clone());
                if let Some(host) = host {
                    with_resolved_jump(&area, &store, host, clone!(
                        #[strong]
                        area,
                        #[strong]
                        store,
                        move |host, jump| {
                            require_password(
                                &area,
                                host,
                                clone!(
                                    #[strong]
                                    store,
                                    move |area, host, password| open_key_deploy_view(
                                        area,
                                        host,
                                        password,
                                        &store,
                                        jump.clone()
                                    )
                                ),
                            );
                        }
                    ));
                }
            }
        ));
        action_group.add_action(&edit_action);
        action_group.add_action(&wake_action);
        action_group.add_action(&delete_action);
        action_group.add_action(&docker_action);
        action_group.add_action(&commands_action);
        action_group.add_action(&sftp_action);
        action_group.add_action(&forward_action);
        action_group.add_action(&key_deploy_action);
        row.insert_action_group("host", Some(&action_group));

        list.append(&row);
    }
}

/// `mac_address`: bara den AKTUELLA raden vet om Wake-on-LAN är meningsfullt
/// — till skillnad från de andra posterna (styrda av globala
/// `FeatureToggles`) är "Väck"-posten en PER-VÄRD-egenskap, samma UX-regel
/// som App/HostListView.swift (`if host.macAddress != nil`).
fn gio_menu_for(toggles: &settings::FeatureToggles, mac_address: &Option<String>) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Redigera"), Some("host.edit"));
    if mac_address.is_some() {
        menu.append(Some("Väck (Wake-on-LAN)"), Some("host.wake"));
    }
    if toggles.show_docker {
        menu.append(Some("Docker"), Some("host.docker"));
    }
    if toggles.show_command_library {
        menu.append(Some("Kommandon"), Some("host.commands"));
    }
    if toggles.show_sftp_browser {
        menu.append(Some("Filer"), Some("host.sftp"));
    }
    if toggles.show_port_forward {
        menu.append(Some("Tunnel"), Some("host.forward"));
    }
    if toggles.show_key_deploy {
        menu.append(Some("Nyckel"), Some("host.key_deploy"));
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
    // Avgör vilket nyckeldistributionskommando `key_deploy::deploy_command_for_host`
    // bygger — Windows OpenSSH kräver helt andra kommandon/sökvägar än POSIX
    // (se key_deploy.rs). Samma tre alternativ som Swift-sidans `RemotePlatform`.
    let platform_row = adw::ComboRow::builder().title("Fjärrsystem").build();
    let platform_model = gtk::StringList::new(&["Linux/macOS", "Windows (adminkonto)", "Windows (standardkonto)"]);
    platform_row.set_model(Some(&platform_model));
    let mac_row = adw::EntryRow::builder()
        .title("MAC-adress (valfritt, Wake-on-LAN, t.ex. AA:BB:CC:DD:EE:FF)")
        .build();
    let mac_error_label = gtk::Label::builder()
        .label("Ogiltig MAC-adress")
        .margin_start(12)
        .margin_end(12)
        .halign(gtk::Align::Start)
        .css_classes(["error", "caption"])
        .visible(false)
        .build();

    if let Some(h) = &existing {
        alias_row.set_text(&h.alias);
        host_row.set_text(&h.host_name);
        user_row.set_text(&h.user);
        port_row.set_text(&h.port.to_string());
        platform_row.set_selected(match h.platform {
            host::RemotePlatform::Posix => 0,
            host::RemotePlatform::WindowsAdmin => 1,
            host::RemotePlatform::WindowsStandard => 2,
        });
        if let Some(mac) = &h.mac_address {
            mac_row.set_text(mac);
        }
    }

    let group = adw::PreferencesGroup::new();
    group.add(&alias_row);
    group.add(&host_row);
    group.add(&user_row);
    group.add(&port_row);
    group.add(&platform_row);
    group.add(&mac_row);

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
    content.append(&mac_error_label);

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
        #[strong]
        mac_row,
        #[strong]
        mac_error_label,
        move |_| {
            let alias = alias_row.text().to_string();
            let host_name = host_row.text().to_string();
            let user = user_row.text().to_string();
            let port: i64 = port_row.text().parse().unwrap_or(22);
            if alias.is_empty() || host_name.is_empty() || user.is_empty() {
                return; // formuläret kräver alias/värdnamn/användare
            }
            // Validerad HÄR, inte bara vid "Väck"-knappen — en trasig adress
            // sparad tyst skulle göra Wake-knappen deterministiskt trasig
            // senare, samma resonemang som App/HostEditView.swifts
            // `macValidationMessage`.
            let mac_text = mac_row.text().trim().to_string();
            let mac_address = if mac_text.is_empty() {
                None
            } else if wake_on_lan::parse_mac(&mac_text).is_ok() {
                Some(mac_text)
            } else {
                mac_error_label.set_visible(true);
                return;
            };
            mac_error_label.set_visible(false);
            let platform = match platform_row.selected() {
                1 => host::RemotePlatform::WindowsAdmin,
                2 => host::RemotePlatform::WindowsStandard,
                _ => host::RemotePlatform::Posix,
            };
            let host = if let Some(mut h) = existing.clone() {
                h.alias = alias;
                h.host_name = host_name;
                h.user = user;
                h.port = port;
                h.platform = platform;
                h.mac_address = mac_address;
                h
            } else {
                let mut h = Host::new(alias, host_name, user);
                h.port = port;
                h.platform = platform;
                h.mac_address = mac_address;
                h
            };
            // Se motiveringen vid `delete_action` ovan — en I/O-miss ska
            // inte krascha appen, bara lämna dialogen öppen så användaren
            // ser att sparandet inte gick igenom (fönstret stängs bara vid
            // lyckat utfall, nedan).
            if let Err(e) = store.borrow_mut().upsert(host) {
                eprintln!("kunde inte spara värden: {e}");
                return;
            }
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
    sync_config: &Rc<RefCell<sync::SyncConfig>>,
) {
    let current = settings_store.borrow().current();

    let docker_row = adw::SwitchRow::builder()
        .title("Docker")
        .subtitle("Visa Docker-knappen på värdar")
        .active(current.show_docker)
        .build();
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

    let sync_folder_row = adw::ActionRow::builder()
        .title("Synkmapp")
        .subtitle(
            sync_config
                .borrow()
                .folder_path
                .clone()
                .unwrap_or_else(|| "Ingen vald".to_string()),
        )
        .build();
    let choose_folder_button = gtk::Button::with_label("Välj mapp…");
    choose_folder_button.set_valign(gtk::Align::Center);
    sync_folder_row.add_suffix(&choose_folder_button);

    let encrypted_row = adw::SwitchRow::builder()
        .title("Kryptera (för molnmappar du inte litar på blint)")
        .subtitle("Dropbox/Google Drive/OneDrive — AES-256-GCM, lösenfras krävs vid varje synk")
        .active(sync_config.borrow().encrypted)
        .build();
    let passphrase_row = adw::PasswordEntryRow::builder()
        .title("Lösenfras")
        .visible(sync_config.borrow().encrypted)
        .build();
    encrypted_row.connect_active_notify(clone!(
        #[weak]
        passphrase_row,
        #[strong]
        sync_config,
        move |row| {
            passphrase_row.set_visible(row.is_active());
            let mut cfg = sync_config.borrow_mut();
            cfg.encrypted = row.is_active();
            if let Err(e) = cfg.save(&sync::SyncConfig::default_path()) {
                eprintln!("kunde inte spara synkinställningen: {e}");
            }
        }
    ));

    let sync_now_row = adw::ActionRow::builder()
        .title("Synka nu")
        .activatable(true)
        .build();
    let sync_status_label = gtk::Label::builder().opacity(0.7).build();
    sync_now_row.add_suffix(&sync_status_label);

    let sync_group = adw::PreferencesGroup::builder().title("Synk").description("Delar host-databasen mellan enheter via en mapp som redan synkas av något annat (Syncthing, en klonad Git-mapp) — eller en krypterad fil i en molnmapp (Dropbox/Drive/OneDrive). Se SYNC_PROTOCOL.md.").build();
    sync_group.add(&sync_folder_row);
    sync_group.add(&encrypted_row);
    sync_group.add(&passphrase_row);
    sync_group.add(&sync_now_row);

    let page = adw::PreferencesPage::new();
    page.add(&group);
    page.add(&sync_group);

    let close_button = gtk::Button::with_label("Klar");
    close_button.add_css_class("suggested-action");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
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
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
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
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
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
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store);
        }
    ));

    choose_folder_button.connect_clicked(clone!(
        #[strong]
        win,
        #[strong]
        sync_config,
        #[weak]
        sync_folder_row,
        move |_| {
            let dialog = gtk::FileDialog::builder().title("Välj synkmapp").build();
            dialog.select_folder(
                Some(&win),
                None::<&gtk::gio::Cancellable>,
                clone!(
                    #[strong]
                    sync_config,
                    #[weak]
                    sync_folder_row,
                    move |result| {
                        if let Ok(folder) = result {
                            if let Some(path) = folder.path() {
                                let path_str = path.to_string_lossy().to_string();
                                let mut cfg = sync_config.borrow_mut();
                                cfg.folder_path = Some(path_str.clone());
                                if let Err(e) = cfg.save(&sync::SyncConfig::default_path()) {
                                    eprintln!("kunde inte spara synkinställningen: {e}");
                                    return;
                                }
                                sync_folder_row.set_subtitle(&path_str);
                            }
                        }
                    }
                ),
            );
        }
    ));

    sync_now_row.connect_activated(clone!(
        #[strong]
        store,
        #[strong]
        sync_config,
        #[weak]
        sync_status_label,
        #[weak]
        passphrase_row,
        #[weak]
        sync_now_row,
        #[weak]
        list,
        #[strong]
        app,
        #[strong]
        area,
        #[strong]
        settings_store,
        #[strong]
        snippet_store,
        move |_| {
            let Some(folder) = sync_config.borrow().folder_path.clone() else {
                sync_status_label.set_text("Välj en mapp först");
                return;
            };
            let encrypted = sync_config.borrow().encrypted;

            // Både filens I/O (kan vara en molnsynkad mapp — Dropbox/Drive/
            // OneDrive — som stallar) och, för den krypterade varianten,
            // PBKDF2-nyckelhärledningen är för tunga för att köras rakt av
            // i klickhanteraren — det fryser hela GTK-huvudloopen för
            // varaktigheten (CodeRabbit-fynd). Görs istället på en egen
            // bakgrundstråd mot en FRISTÅENDE `HostStore::open` (samma fil,
            // men INTE den delade `Rc<RefCell<HostStore>>` — den är inte
            // `Send` och får aldrig korsa en trådgräns); resultatet läses
            // tillbaka till den riktiga butiken bara vid lyckat utfall.
            sync_status_label.set_text("Synkar…");
            sync_now_row.set_sensitive(false);
            let rx = if encrypted {
                let passphrase = passphrase_row.text().to_string();
                if passphrase.is_empty() {
                    sync_status_label.set_text("Ange en lösenfras först");
                    sync_now_row.set_sensitive(true);
                    return;
                }
                let provider = sync_crypto::EncryptedFolderSyncProvider::new(
                    std::path::PathBuf::from(folder).join("hosts.enc"),
                    passphrase,
                );
                spawn_background_sync_encrypted(provider)
            } else {
                let provider = sync::FolderSyncProvider::new(
                    std::path::PathBuf::from(folder).join("hosts.json"),
                );
                spawn_background_sync_plain(provider)
            };

            glib::spawn_future_local(clone!(
                #[strong]
                store,
                #[strong]
                sync_status_label,
                #[strong]
                sync_now_row,
                #[strong]
                list,
                #[strong]
                app,
                #[strong]
                area,
                #[strong]
                settings_store,
                #[strong]
                snippet_store,
                async move {
                    let result = rx
                        .recv()
                        .await
                        .unwrap_or_else(|_| Err("kanalen stängdes oväntat".to_string()));
                    match result {
                        Ok(()) => {
                            // Läs om från disk — bakgrundstråden skrev med
                            // sin EGEN `HostStore`-instans, den delade
                            // `Rc<RefCell<HostStore>>` här vet inget om det
                            // förrän den öppnas igen.
                            match host::HostStore::open(host::HostStore::default_path()) {
                                Ok(reloaded) => {
                                    *store.borrow_mut() = reloaded;
                                    sync_status_label.set_text("Synkad");
                                    refresh_list(
                                        &list,
                                        &store,
                                        &app,
                                        &area,
                                        &settings_store,
                                        &snippet_store,
                                    );
                                }
                                Err(e) => sync_status_label
                                    .set_text(&format!("Fel: synkad men kunde inte läsa om: {e}")),
                            }
                        }
                        Err(e) => sync_status_label.set_text(&format!("Fel: {e}")),
                    }
                    sync_now_row.set_sensitive(true);
                }
            ));
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

        let area = Rc::new(SessionArea {
            overlay,
            tab_view,
            tab_bar,
            placeholder,
        });
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
                if let Some(content_box) = page.child().downcast_ref::<gtk::Box>() {
                    // Tunnel-flikens innehåll (om den här sidan råkar vara
                    // en) bär ett `ActiveForward`-handtag — utan detta
                    // fortsatte en aktiv `-L`/`-R`/`-D`-vidarebefordran att
                    // köra i bakgrunden för alltid efter att fliken stängts
                    // (CodeRabbit-fynd), ingen väg kvar att stoppa den.
                    unsafe {
                        if let Some(handle) = content_box
                            .steal_data::<Rc<RefCell<Option<port_forward::ActiveForward>>>>("bastion-active-forward")
                        {
                            if let Some(forward) = handle.borrow_mut().take() {
                                forward.stop();
                            }
                        }
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
/// Visar ett kort felmeddelande i en modal dialog — bara för fel som
/// upptäcks INNAN någon anslutning ens försöks (just nu bara en trasig
/// jump-host-konfiguration, se `host::HostStore::resolve_jump`). Fel som
/// upptäcks EFTER att anslutningen startat visas i respektive vys egen
/// felyta (statusrad/röd text i terminalen) som redan fanns — den här
/// dialogen är bara för "kunde inte ens börja".
fn show_connect_error(area: &Rc<SessionArea>, message: &str) {
    show_message_dialog(area, "Kunde inte ansluta", message);
}

/// Samma modala dialog som `show_connect_error`, fast med valfri titel —
/// återanvänd av t.ex. Wake-on-LAN-resultat (inte ett anslutningsfel, men
/// samma "kort meddelande, en OK-knapp"-behov).
fn show_message_dialog(area: &Rc<SessionArea>, title: &str, message: &str) {
    let dialog = adw::AlertDialog::new(Some(title), Some(message));
    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.present(Some(&area.overlay));
}

/// Löser upp `host`s jump-host (om någon) och kör `then` med resultatet —
/// delad av alla vy-öppnare (terminal/Docker/SFTP/tunnlar/nyckeldistribution)
/// så att ETT ställe känner till `resolve_jump`s felkontrakt. En trasig
/// jump-host-konfiguration visar felet direkt och avbryter — ansluter
/// ALDRIG direkt mot target och hoppar tyst över en konfigurerad jump-host.
fn with_resolved_jump(
    area: &Rc<SessionArea>,
    store: &Rc<RefCell<HostStore>>,
    host: host::Host,
    then: impl FnOnce(host::Host, Option<host::Host>) + 'static,
) {
    match store.borrow().resolve_jump(&host) {
        Ok(jump) => then(host, jump),
        Err(e) => show_connect_error(area, &e),
    }
}

fn open_session(area: &Rc<SessionArea>, store: &Rc<RefCell<HostStore>>, host: host::Host) {
    with_resolved_jump(area, store, host, clone!(
        #[strong]
        area,
        move |host, jump| {
            if matches!(host.auth, host::HostAuth::AskPassword) {
                prompt_password_then(&area, host, move |area, host, password| {
                    start_session(area, host, Some(password), jump.clone())
                });
            } else {
                start_session(&area, host, None, jump);
            }
        }
    ));
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
    let entry = gtk::PasswordEntry::builder()
        .show_peek_icon(true)
        .hexpand(true)
        .build();
    let group = adw::PreferencesGroup::builder()
        .title(format!("Lösenord för {}@{}", host.user, host.host_name))
        .build();
    group.add(&entry);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let connect_button = gtk::Button::with_label("Anslut");
    connect_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&connect_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
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

fn start_session(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
) {
    let terminal = vte::Terminal::builder().vexpand(true).hexpand(true).build();

    let cols = 80u32;
    let rows = 24u32;
    let session = ssh::spawn_shell(host.clone(), password, cols, rows, jump);

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
                        terminal.feed(
                            format!("\r\n\x1b[31m[bastion] fel: {msg}\x1b[0m\r\n").as_bytes(),
                        );
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

/// Ansluter till en Telnet-värd (RFC 854, okrypterat, inget lösenord/
/// nyckelval — autentisering, om servern ens kräver någon, sker inuti
/// terminalsessionen via en login-prompt, inte som ett separat
/// handskakningssteg). Motsvarar `start_session` för SSH, men wirear
/// `telnet::spawn` istället för `ssh::spawn_shell` — samma
/// `terminal.set_data("bastion-ssh-input", …)`-nyckel återanvänds rakt av
/// så den redan existerande generiska close-page-städningen (`SessionArea::
/// new`) fungerar identiskt utan telnet-specifik kod där.
fn start_telnet_session(area: &Rc<SessionArea>, host: String, port: u16) {
    let terminal = vte::Terminal::builder().vexpand(true).hexpand(true).build();
    let handle = telnet::spawn(host.clone(), port);

    unsafe {
        terminal.set_data("bastion-ssh-input", handle.input.clone());
    }

    let page = area.tab_view.append(&terminal);
    page.set_title(&format!("{host}:{port} (telnet)"));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    terminal.connect_commit(clone!(
        #[strong(rename_to = input)]
        handle.input,
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
        handle.output,
        async move {
            while let Ok(event) = output.recv().await {
                match event {
                    telnet::TelnetEvent::Data(bytes) => terminal.feed(&bytes),
                    telnet::TelnetEvent::Error(msg) => {
                        terminal.feed(
                            format!("\r\n\x1b[31m[bastion] fel: {msg}\x1b[0m\r\n").as_bytes(),
                        );
                    }
                    telnet::TelnetEvent::Connected => {}
                    telnet::TelnetEvent::Closed => {
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

/// Ad-hoc anslutningsdialog för Telnet — inget sparande i värdlistan,
/// motsvarar `App/TelnetConnectView.swift` (bara värd+port, ingen auth).
fn show_telnet_connect_dialog(app: &adw::Application, area: &Rc<SessionArea>) {
    let host_row = adw::EntryRow::builder().title("Värd (t.ex. 10.0.0.5)").build();
    let port_row = adw::EntryRow::builder().title("Port").text("23").build();

    let group = adw::PreferencesGroup::new();
    group.add(&host_row);
    group.add(&port_row);
    let warning_label = gtk::Label::builder()
        .label("Telnet är okrypterat — använd bara på ett nätverk du litar på (t.ex. mot nätverksutrustning som saknar SSH-stöd).")
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .halign(gtk::Align::Start)
        .wrap(true)
        .css_classes(["dim-label", "caption"])
        .build();

    let page = adw::PreferencesPage::new();
    page.add(&group);

    let connect_button = gtk::Button::with_label("Anslut");
    connect_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&connect_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);
    content.append(&warning_label);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(380)
        .default_height(220)
        .title("Telnet")
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
        move |_| {
            let host = host_row.text().trim().to_string();
            let Ok(port) = port_row.text().parse::<u16>() else {
                return;
            };
            if host.is_empty() || port == 0 {
                return;
            }
            win.close();
            start_telnet_session(&area, host, port);
        }
    ));

    win.present();
}

/// Ansluter till en ad-hoc värd UTAN att spara den i värdlistan — Termius
/// kallar detta "Quick Connect". Bygger en `Host` bara i minnet (aldrig
/// skickad till `HostStore`) och öppnar samma `start_session`-flöde som en
/// sparad värd. Motsvarar `App/QuickConnectView.swift`. Går direkt till
/// `start_session` (inte `open_session`s omväg via `with_resolved_jump`/
/// `prompt_password_then`) — en ad-hoc-värd har per definition ingen
/// `jump_host_id`, och lösenordet (om något) matas in i SAMMA formulär,
/// inte en separat efterföljande dialog.
fn show_quick_connect_dialog(app: &adw::Application, area: &Rc<SessionArea>) {
    let host_row = adw::EntryRow::builder().title("Värd (t.ex. 10.0.0.5)").build();
    let user_row = adw::EntryRow::builder().title("Användare").build();
    let port_row = adw::EntryRow::builder().title("Port").text("22").build();
    let password_row = adw::PasswordEntryRow::builder()
        .title("Lösenord (tomt = agent/standardnyckel)")
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&host_row);
    group.add(&user_row);
    group.add(&port_row);
    group.add(&password_row);
    let info_label = gtk::Label::builder()
        .label("Den här värden sparas INTE i din värdlista — perfekt för en engångsanslutning. Lägg till den vanligt med + om du vill återansluta senare.")
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .halign(gtk::Align::Start)
        .wrap(true)
        .css_classes(["dim-label", "caption"])
        .build();

    let page = adw::PreferencesPage::new();
    page.add(&group);

    let connect_button = gtk::Button::with_label("Anslut");
    connect_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&connect_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);
    content.append(&info_label);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(380)
        .default_height(320)
        .title("Snabbanslutning")
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
        move |_| {
            let host_name = host_row.text().trim().to_string();
            let user = user_row.text().trim().to_string();
            let Ok(port) = port_row.text().parse::<i64>() else {
                return;
            };
            if host_name.is_empty() || user.is_empty() || !(1..=65_535).contains(&port) {
                return;
            }
            // Lösenordet skickas OBESKURET — trimning hade tyst korrumperat
            // ett giltigt lösenord med inlednings-/avslutande blanktecken,
            // samma cubic-fynd Swift-sidan (PR #173) redan vaktar mot.
            // `is_empty` (inte trimmat) avgör bara vilket auth-läge som väljs.
            let password = password_row.text().to_string();
            let mut host = host::Host::new(String::new(), host_name, user);
            host.port = port;
            let password = if password.is_empty() {
                host.auth = host::HostAuth::AgentDefault;
                None
            } else {
                host.auth = host::HostAuth::AskPassword;
                Some(password)
            };
            win.close();
            start_session(&area, host, password, None);
        }
    ));

    win.present();
}

/// Föreslå SSH-värdar ur ett tailnet — två källor att välja mellan
/// (användaren avgör vad som är bekvämast, inte appen): den här enheten
/// (`tailscale::fetch_local`) eller en redan sparad fjärrvärd
/// (`tailscale::fetch_remote`, samma jump-host-medvetna anslutningsväg som
/// alla andra engångskommandon). Motsvarar `App/TailscaleDiscoveryView.swift`.
/// "Lägg till" öppnar samma "Ny värd"-dialog som +-knappen, bara förifyllt
/// med tailnet-adressen — Tailscale känner inte till SSH-användarnamnet,
/// så det sista steget är alltid det vanliga redigeringsläget.
fn show_tailscale_discovery_dialog(
    app: &adw::Application,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
    area: &Rc<SessionArea>,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
) {
    let hosts = store.borrow().all().into_iter().cloned().collect::<Vec<_>>();
    let mut source_labels: Vec<String> = vec!["Denna enhet".to_string()];
    source_labels.extend(hosts.iter().map(|h| h.alias.clone()));
    let source_row = adw::ComboRow::builder().title("Källa").build();
    let source_labels_refs: Vec<&str> = source_labels.iter().map(String::as_str).collect();
    source_row.set_model(Some(&gtk::StringList::new(&source_labels_refs)));

    let group = adw::PreferencesGroup::new();
    group.add(&source_row);

    let fetch_button = gtk::Button::with_label("Hämta");
    fetch_button.add_css_class("suggested-action");
    let status_label = gtk::Label::builder()
        .label("Inte hämtat")
        .margin_start(12)
        .margin_end(12)
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();

    let results_list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(12)
        .build();
    let results_scrolled = gtk::ScrolledWindow::builder()
        .child(&results_list)
        .min_content_height(240)
        .vexpand(true)
        .build();

    let page = adw::PreferencesPage::new();
    page.add(&group);

    let close_button = gtk::Button::with_label("Klar");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&close_button);
    header.pack_end(&fetch_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);
    content.append(&status_label);
    content.append(&results_scrolled);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(460)
        .default_height(480)
        .title("Tailscale")
        .content(&content)
        .build();

    close_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));

    fetch_button.connect_clicked(clone!(
        #[strong]
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
        #[strong]
        hosts,
        #[weak]
        results_list,
        #[strong]
        status_label,
        #[strong]
        fetch_button,
        #[weak]
        win,
        move |_| {
            while let Some(row) = results_list.row_at_index(0) {
                results_list.remove(&row);
            }
            status_label.set_label("Hämtar…");
            fetch_button.set_sensitive(false);

            let selected = source_row.selected();
            let remote_host = if selected == 0 {
                None
            } else {
                hosts.get((selected - 1) as usize).cloned()
            };

            let finish = clone!(
                #[strong]
                app,
                #[strong]
                store,
                #[strong]
                list,
                #[strong]
                area,
                #[strong]
                settings_store,
                #[strong]
                snippet_store,
                #[strong]
                results_list,
                #[strong]
                status_label,
                #[strong]
                fetch_button,
                #[strong]
                win,
                move |result: Result<tailscale::TailscaleStatus, tailscale::TailscaleError>| {
                    fetch_button.set_sensitive(true);
                    match result {
                        Ok(status) => {
                            let suggested = status.suggested_hosts();
                            if suggested.is_empty() {
                                status_label.set_label("Inga peers online med en Tailscale-IP hittades.");
                            } else {
                                status_label.set_label(&format!("{} förslag", suggested.len()));
                            }
                            for (host_name, address) in suggested {
                                let row = adw::ActionRow::builder()
                                    .title(&host_name)
                                    .subtitle(&address)
                                    .build();
                                let add_button = gtk::Button::with_label("Lägg till");
                                add_button.connect_clicked(clone!(
                                    #[strong]
                                    app,
                                    #[strong]
                                    store,
                                    #[strong]
                                    list,
                                    #[strong]
                                    area,
                                    #[strong]
                                    settings_store,
                                    #[strong]
                                    snippet_store,
                                    #[strong]
                                    host_name,
                                    #[strong]
                                    address,
                                    #[weak]
                                    win,
                                    move |_| {
                                        let prefilled = Host::new(host_name.clone(), address.clone(), String::new());
                                        win.close();
                                        show_host_dialog(
                                            &app,
                                            &store,
                                            &list,
                                            &area,
                                            &settings_store,
                                            &snippet_store,
                                            Some(prefilled),
                                        );
                                    }
                                ));
                                row.add_suffix(&add_button);
                                results_list.append(&row);
                            }
                        }
                        Err(e) => status_label.set_label(&format!("Fel: {e}")),
                    }
                }
            );

            match remote_host {
                None => {
                    let rx = tailscale::spawn_fetch_local();
                    glib::spawn_future_local(async move {
                        let result = rx.recv().await.unwrap_or_else(|_| {
                            Err(tailscale::TailscaleError::Io("kanalen stängdes oväntat".to_string()))
                        });
                        finish(result);
                    });
                }
                Some(host) => {
                    with_resolved_jump(&area, &store, host, clone!(
                        #[strong]
                        area,
                        move |host, jump| {
                            require_password(&area, host, move |_area, host, password| {
                                let finish = finish.clone();
                                let jump = jump.clone();
                                glib::spawn_future_local(async move {
                                    let result = tailscale::fetch_remote(host, password, jump).await;
                                    finish(result);
                                });
                            });
                        }
                    ));
                }
            }
        }
    ));

    win.present();
}

/// Lista över sparade WireGuard-profiler — toppnivå, inte kopplat till en
/// specifik `Host` (en profil beskriver en VPN-anslutning, inte en
/// SSH-värd). Samma v1-avgränsning som App/-motsvarigheten: profilhantering
/// (klistra in/visa/redigera/ta bort `.conf`-text), INTE att faktiskt
/// upprätta tunneln — se `wireguard.rs`s doc-kommentar.
fn show_wireguard_profile_list(
    app: &adw::Application,
    wireguard_store: &Rc<RefCell<wireguard::WireGuardProfileStore>>,
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
    add_button.set_tooltip_text(Some("Ny profil"));
    let header = adw::HeaderBar::new();
    header.pack_end(&add_button);
    header.set_title_widget(Some(&adw::WindowTitle::new("WireGuard-profiler", "")));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&scrolled);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(420)
        .default_height(480)
        .content(&content)
        .build();

    refresh_wireguard_profile_list(app, wireguard_store, &list, &win);

    add_button.connect_clicked(clone!(
        #[strong]
        app,
        #[strong]
        wireguard_store,
        #[weak]
        list,
        #[weak]
        win,
        move |_| show_wireguard_profile_edit(
            &app,
            &wireguard_store,
            &list,
            &win,
            wireguard::WireGuardProfile::new(String::new(), wireguard::WireGuardConfig::default()),
        )
    ));

    win.present();
}

fn refresh_wireguard_profile_list(
    app: &adw::Application,
    wireguard_store: &Rc<RefCell<wireguard::WireGuardProfileStore>>,
    list: &gtk::ListBox,
    parent_win: &adw::Window,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    for profile in wireguard_store.borrow().all() {
        let address = profile
            .config
            .interface
            .address
            .first()
            .cloned()
            .unwrap_or_else(|| "ingen adress".to_string());
        let peer_word = if profile.config.peers.len() == 1 { "peer" } else { "peers" };
        let row = adw::ActionRow::builder()
            .title(&profile.name)
            .subtitle(format!("{address} · {} {peer_word}", profile.config.peers.len()))
            .activatable(true)
            .build();

        let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
        delete_button.set_tooltip_text(Some("Ta bort"));
        delete_button.add_css_class("flat");
        delete_button.connect_clicked(clone!(
            #[strong]
            app,
            #[strong]
            wireguard_store,
            #[weak]
            list,
            #[weak]
            parent_win,
            #[strong(rename_to = profile_id)]
            profile.id,
            move |_| {
                if let Err(e) = wireguard_store.borrow_mut().delete(profile_id) {
                    eprintln!("kunde inte ta bort wireguard-profilen: {e}");
                    return;
                }
                refresh_wireguard_profile_list(&app, &wireguard_store, &list, &parent_win);
            }
        ));
        row.add_suffix(&delete_button);

        row.connect_activated(clone!(
            #[strong]
            app,
            #[strong]
            wireguard_store,
            #[weak]
            list,
            #[weak]
            parent_win,
            #[strong]
            profile,
            move |_| show_wireguard_profile_edit(&app, &wireguard_store, &list, &parent_win, profile.clone())
        ));

        list.append(&row);
    }
}

/// Redigerar en `WireGuardProfile` som rå `.conf`-text — enklare och mer
/// direkt begripligt för en användare som redan har filen (från sin
/// VPN-leverantör/router) än ett fält-för-fält-formulär. `WireGuardConfig::
/// parse` är förlåtande (okända rader hoppas tyst över) så ogiltig text ger
/// bara en tom/ofullständig profil, inte en krasch.
fn show_wireguard_profile_edit(
    app: &adw::Application,
    wireguard_store: &Rc<RefCell<wireguard::WireGuardProfileStore>>,
    list: &gtk::ListBox,
    parent_win: &adw::Window,
    profile: wireguard::WireGuardProfile,
) {
    let name_row = adw::EntryRow::builder().title("Namn").text(&profile.name).build();
    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let text_view = gtk::TextView::builder().monospace(true).build();
    text_view.buffer().set_text(&profile.config.rendered());
    let text_scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(240)
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .vexpand(true)
        .build();

    let save_button = gtk::Button::with_label("Spara");
    save_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);
    content.append(&text_scrolled);

    let win = adw::Window::builder()
        .transient_for(parent_win)
        .modal(true)
        .default_width(460)
        .default_height(500)
        .title("WireGuard-profil")
        .content(&content)
        .build();

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    save_button.connect_clicked(clone!(
        #[strong]
        app,
        #[strong]
        wireguard_store,
        #[weak]
        list,
        #[strong]
        parent_win,
        #[weak]
        win,
        #[strong]
        profile,
        move |_| {
            let name = name_row.text().trim().to_string();
            if name.is_empty() {
                return;
            }
            let buffer = text_view.buffer();
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
            let mut updated = profile.clone();
            updated.name = name;
            updated.config = wireguard::WireGuardConfig::parse(&text);
            if let Err(e) = wireguard_store.borrow_mut().upsert(updated) {
                eprintln!("kunde inte spara wireguard-profilen: {e}");
                return;
            }
            refresh_wireguard_profile_list(&app, &wireguard_store, &list, &parent_win);
            win.close();
        }
    ));

    win.present();
}

/// Lista över sparade S3-anslutningar. "Bläddra" på en rad öppnar en
/// bucket-/objektbläddare i en ny flik (`open_s3_bucket_browser`); raden
/// själv öppnar redigeringsdialogen (namn/endpoint/region/nycklar).
fn show_s3_connection_list(
    app: &adw::Application,
    area: &Rc<SessionArea>,
    s3_store: &Rc<RefCell<s3::S3ConnectionStore>>,
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
    add_button.set_tooltip_text(Some("Ny anslutning"));
    let header = adw::HeaderBar::new();
    header.pack_end(&add_button);
    header.set_title_widget(Some(&adw::WindowTitle::new("S3-anslutningar", "")));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&scrolled);

    let win = adw::Window::builder()
        .transient_for(&app.active_window().expect("inget aktivt fönster"))
        .modal(true)
        .default_width(420)
        .default_height(480)
        .content(&content)
        .build();

    refresh_s3_connection_list(app, area, s3_store, &list, &win);

    add_button.connect_clicked(clone!(
        #[strong]
        app,
        #[strong]
        area,
        #[strong]
        s3_store,
        #[weak]
        list,
        #[weak]
        win,
        move |_| show_s3_connection_edit(
            &app,
            &area,
            &s3_store,
            &list,
            &win,
            s3::S3Connection::new(
                String::new(),
                String::new(),
                "us-east-1".to_string(),
                String::new(),
                String::new(),
            ),
        )
    ));

    win.present();
}

fn refresh_s3_connection_list(
    app: &adw::Application,
    area: &Rc<SessionArea>,
    s3_store: &Rc<RefCell<s3::S3ConnectionStore>>,
    list: &gtk::ListBox,
    parent_win: &adw::Window,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    for connection in s3_store.borrow().all() {
        let row = adw::ActionRow::builder()
            .title(&connection.name)
            .subtitle(format!("{} · {}", connection.endpoint, connection.region))
            .activatable(true)
            .build();

        let browse_button = gtk::Button::from_icon_name("folder-open-symbolic");
        browse_button.set_tooltip_text(Some("Bläddra"));
        browse_button.add_css_class("flat");
        browse_button.connect_clicked(clone!(
            #[strong]
            area,
            #[weak]
            parent_win,
            #[strong]
            connection,
            move |_| {
                open_s3_bucket_browser(&area, connection.clone());
                parent_win.close();
            }
        ));
        row.add_suffix(&browse_button);

        let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
        delete_button.set_tooltip_text(Some("Ta bort"));
        delete_button.add_css_class("flat");
        delete_button.connect_clicked(clone!(
            #[strong]
            app,
            #[strong]
            area,
            #[strong]
            s3_store,
            #[weak]
            list,
            #[weak]
            parent_win,
            #[strong(rename_to = connection_id)]
            connection.id,
            move |_| {
                if let Err(e) = s3_store.borrow_mut().delete(connection_id) {
                    eprintln!("kunde inte ta bort s3-anslutningen: {e}");
                    return;
                }
                refresh_s3_connection_list(&app, &area, &s3_store, &list, &parent_win);
            }
        ));
        row.add_suffix(&delete_button);

        row.connect_activated(clone!(
            #[strong]
            app,
            #[strong]
            area,
            #[strong]
            s3_store,
            #[weak]
            list,
            #[weak]
            parent_win,
            #[strong]
            connection,
            move |_| show_s3_connection_edit(&app, &area, &s3_store, &list, &parent_win, connection.clone())
        ));

        list.append(&row);
    }
}

fn show_s3_connection_edit(
    app: &adw::Application,
    area: &Rc<SessionArea>,
    s3_store: &Rc<RefCell<s3::S3ConnectionStore>>,
    list: &gtk::ListBox,
    parent_win: &adw::Window,
    connection: s3::S3Connection,
) {
    let name_row = adw::EntryRow::builder().title("Namn").text(&connection.name).build();
    let endpoint_row = adw::EntryRow::builder()
        .title("Endpoint (t.ex. https://s3.example.com)")
        .text(&connection.endpoint)
        .build();
    let region_row = adw::EntryRow::builder().title("Region").text(&connection.region).build();
    let access_key_row = adw::EntryRow::builder()
        .title("Access Key ID")
        .text(&connection.access_key_id)
        .build();
    let secret_key_row = adw::PasswordEntryRow::builder()
        .title("Secret Access Key")
        .text(&connection.secret_access_key)
        .build();

    let test_button = gtk::Button::with_label("Testa anslutning");
    let test_status_label = gtk::Label::builder()
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(8)
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();

    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    group.add(&endpoint_row);
    group.add(&region_row);
    group.add(&access_key_row);
    group.add(&secret_key_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let save_button = gtk::Button::with_label("Spara");
    save_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&cancel_button);
    header.pack_end(&save_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);
    content.append(&test_button);
    content.append(&test_status_label);

    let win = adw::Window::builder()
        .transient_for(parent_win)
        .modal(true)
        .default_width(420)
        .default_height(360)
        .title("S3-anslutning")
        .content(&content)
        .build();

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    // Bygger ett ANONYMT S3Connection av de aktuella fältvärdena (inte
    // nödvändigtvis sparade än) — "Testa" ska verifiera det som STÅR i
    // formuläret just nu, inte den senast sparade versionen, samma
    // resonemang som key_deploy's "verifiera innan lösenordet tas bort".
    test_button.connect_clicked(clone!(
        #[strong]
        connection,
        #[strong]
        endpoint_row,
        #[strong]
        region_row,
        #[strong]
        access_key_row,
        #[strong]
        secret_key_row,
        #[strong]
        test_status_label,
        #[strong]
        test_button,
        move |_| {
            let mut candidate = connection.clone();
            candidate.endpoint = endpoint_row.text().trim().to_string();
            candidate.region = region_row.text().trim().to_string();
            candidate.access_key_id = access_key_row.text().trim().to_string();
            candidate.secret_access_key = secret_key_row.text().to_string();

            test_status_label.set_label("Testar…");
            test_button.set_sensitive(false);
            let rx = s3::spawn_test_connection(candidate);
            glib::spawn_future_local(clone!(
                #[strong]
                test_status_label,
                #[strong]
                test_button,
                async move {
                    match rx.recv().await {
                        Ok(Ok(buckets)) => test_status_label.set_label(&format!(
                            "Anslutningen fungerar — {} bucket(s) hittades.",
                            buckets.len()
                        )),
                        Ok(Err(e)) => test_status_label.set_label(&format!("Fel: {e}")),
                        Err(_) => test_status_label.set_label("Kanalen stängdes oväntat"),
                    }
                    test_button.set_sensitive(true);
                }
            ));
        }
    ));
    save_button.connect_clicked(clone!(
        #[strong]
        app,
        #[strong]
        area,
        #[strong]
        s3_store,
        #[weak]
        list,
        #[strong]
        parent_win,
        #[weak]
        win,
        #[strong]
        connection,
        move |_| {
            let name = name_row.text().trim().to_string();
            let endpoint = endpoint_row.text().trim().to_string();
            if name.is_empty() || endpoint.is_empty() {
                return;
            }
            let mut updated = connection.clone();
            updated.name = name;
            updated.endpoint = endpoint;
            updated.region = region_row.text().trim().to_string();
            updated.access_key_id = access_key_row.text().trim().to_string();
            updated.secret_access_key = secret_key_row.text().to_string();
            if let Err(e) = s3_store.borrow_mut().upsert(updated) {
                eprintln!("kunde inte spara s3-anslutningen: {e}");
                return;
            }
            refresh_s3_connection_list(&app, &area, &s3_store, &list, &parent_win);
            win.close();
        }
    ));

    win.present();
}

/// Bucket-/objektbläddare för en S3-anslutning — öppnas i en ny flik
/// (samma "en flik per session/vy"-mönster som Docker/SFTP/Tunnel).
/// `current_bucket: None` = visar bucket-listan (rot); `Some(bucket)` =
/// visar objekten i den bucket:en. Bara EN nivå djup — S3:s `Key` är platt
/// (en nyckel som "mapp/fil.txt" är bara en vanlig nyckel, ingen riktig
/// katalog), samma enkla modell `s3::S3Client::list_objects` redan
/// exponerar (inget `prefix`-baserat undermapps-UI i v1).
fn open_s3_bucket_browser(area: &Rc<SessionArea>, connection: s3::S3Connection) {
    let current_bucket: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

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
    let new_bucket_button = gtk::Button::from_icon_name("folder-new-symbolic");
    new_bucket_button.set_tooltip_text(Some("Ny bucket"));
    let upload_button = gtk::Button::from_icon_name("document-send-symbolic");
    upload_button.set_tooltip_text(Some("Ladda upp (i en bucket)"));
    let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");

    let toolbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    toolbar.append(&up_button);
    toolbar.append(&path_label);
    toolbar.append(&new_bucket_button);
    toolbar.append(&upload_button);
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("S3: {}", connection.name));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    up_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            if current_bucket.borrow().is_some() {
                *current_bucket.borrow_mut() = None;
                refresh_s3_browser(&area, connection.clone(), current_bucket.clone(), &list, &path_label);
            }
        }
    ));

    new_bucket_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| prompt_new_bucket_name(&area, connection.clone(), current_bucket.clone(), &list, &path_label)
    ));

    upload_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            // Bara meningsfullt inuti en bucket — ett klick vid roten (ingen
            // vald bucket) gör ingenting, samma "guard, inte gömd knapp"-val
            // som förenklar UI-tillståndet (inget att synka mellan
            // knappsynlighet och `current_bucket` separat).
            let Some(bucket) = current_bucket.borrow().clone() else { return };
            let dialog = gtk::FileDialog::builder().title("Välj fil att ladda upp").build();
            let parent_window = area.overlay.root().and_downcast::<gtk::Window>();
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                connection,
                #[strong]
                current_bucket,
                #[strong]
                bucket,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    let Ok(file) = dialog.open_future(parent_window.as_ref()).await else {
                        return;
                    };
                    let Some(local_path) = file.path() else { return };
                    let Some(key) = local_path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                        return;
                    };
                    let data = match std::fs::read(&local_path) {
                        Ok(d) => d,
                        Err(e) => {
                            list.append(&error_row(&format!("kunde inte läsa {}: {e}", local_path.display())));
                            return;
                        }
                    };
                    let rx = s3::spawn_put_object(connection.clone(), bucket, key, data);
                    match rx.recv().await {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => list.append(&error_row(&e)),
                        Err(_) => list.append(&error_row("kanalen stängdes oväntat")),
                    }
                    refresh_s3_browser(&area, connection, current_bucket, &list, &path_label);
                }
            ));
        }
    ));

    refresh_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| refresh_s3_browser(&area, connection.clone(), current_bucket.clone(), &list, &path_label)
    ));

    refresh_s3_browser(area, connection, current_bucket, &list, &path_label);
}

fn refresh_s3_browser(
    area: &Rc<SessionArea>,
    connection: s3::S3Connection,
    current_bucket: Rc<RefCell<Option<String>>>,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    let bucket = current_bucket.borrow().clone();
    path_label.set_label(bucket.as_deref().unwrap_or("Buckets"));

    match bucket {
        None => {
            let rx = s3::spawn_list_buckets(connection.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                connection,
                #[strong]
                current_bucket,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    match rx.recv().await {
                        Ok(Ok(buckets)) => {
                            for bucket in buckets {
                                list.append(&build_s3_bucket_row(&area, &connection, &current_bucket, &list, &path_label, bucket));
                            }
                        }
                        Ok(Err(e)) => list.append(&error_row(&e)),
                        Err(_) => list.append(&error_row("kanalen stängdes oväntat")),
                    }
                }
            ));
        }
        Some(bucket_name) => {
            let rx = s3::spawn_list_objects(connection.clone(), bucket_name.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                connection,
                #[strong]
                current_bucket,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    match rx.recv().await {
                        Ok(Ok(objects)) => {
                            for object in objects {
                                list.append(&build_s3_object_row(&area, &connection, &current_bucket, &list, &path_label, &bucket_name, object));
                            }
                        }
                        Ok(Err(e)) => list.append(&error_row(&e)),
                        Err(_) => list.append(&error_row("kanalen stängdes oväntat")),
                    }
                }
            ));
        }
    }
}

fn build_s3_bucket_row(
    area: &Rc<SessionArea>,
    connection: &s3::S3Connection,
    current_bucket: &Rc<RefCell<Option<String>>>,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
    bucket: s3::S3Bucket,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&bucket.name)
        .subtitle(bucket.creation_date.clone().unwrap_or_default())
        .activatable(true)
        .build();

    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some("Ta bort bucket"));
    delete_button.add_css_class("flat");
    delete_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        #[strong(rename_to = bucket_name)]
        bucket.name,
        move |_| {
            let rx = s3::spawn_delete_bucket(connection.clone(), bucket_name.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                connection,
                #[strong]
                current_bucket,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = rx.recv().await.unwrap_or_else(|_| Err("kanalen stängdes oväntat".to_string())) {
                        list.append(&error_row(&e));
                        return;
                    }
                    refresh_s3_browser(&area, connection, current_bucket, &list, &path_label);
                }
            ));
        }
    ));
    row.add_suffix(&delete_button);

    row.connect_activated(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        #[strong(rename_to = bucket_name)]
        bucket.name,
        move |_| {
            *current_bucket.borrow_mut() = Some(bucket_name.clone());
            refresh_s3_browser(&area, connection.clone(), current_bucket.clone(), &list, &path_label);
        }
    ));

    row
}

fn build_s3_object_row(
    area: &Rc<SessionArea>,
    connection: &s3::S3Connection,
    current_bucket: &Rc<RefCell<Option<String>>>,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
    bucket_name: &str,
    object: s3::S3Object,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&object.key)
        .subtitle(format!("{} bytes", object.size))
        .build();

    let download_button = gtk::Button::from_icon_name("document-save-symbolic");
    download_button.set_tooltip_text(Some("Ladda ner"));
    download_button.add_css_class("flat");
    download_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong(rename_to = bucket_name)]
        bucket_name.to_string(),
        #[strong]
        list,
        #[strong(rename_to = key)]
        object.key,
        move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("Spara som")
                .initial_name(&key)
                .build();
            let parent_window = area.overlay.root().and_downcast::<gtk::Window>();
            glib::spawn_future_local(clone!(
                #[strong]
                connection,
                #[strong]
                bucket_name,
                #[strong]
                list,
                #[strong]
                key,
                async move {
                    let Ok(file) = dialog.save_future(parent_window.as_ref()).await else {
                        return;
                    };
                    let Some(local_path) = file.path() else { return };
                    let rx = s3::spawn_get_object(connection, bucket_name, key);
                    match rx.recv().await {
                        Ok(Ok(data)) => {
                            if let Err(e) = std::fs::write(&local_path, data) {
                                list.append(&error_row(&format!("kunde inte skriva {}: {e}", local_path.display())));
                            }
                        }
                        Ok(Err(e)) => list.append(&error_row(&e)),
                        Err(_) => list.append(&error_row("kanalen stängdes oväntat")),
                    }
                }
            ));
        }
    ));
    row.add_suffix(&download_button);

    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some("Ta bort"));
    delete_button.add_css_class("flat");
    delete_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        connection,
        #[strong]
        current_bucket,
        #[weak]
        list,
        #[weak]
        path_label,
        #[strong(rename_to = bucket_name)]
        bucket_name.to_string(),
        #[strong(rename_to = key)]
        object.key,
        move |_| {
            let rx = s3::spawn_delete_object(connection.clone(), bucket_name.clone(), key.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                connection,
                #[strong]
                current_bucket,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = rx.recv().await.unwrap_or_else(|_| Err("kanalen stängdes oväntat".to_string())) {
                        list.append(&error_row(&e));
                        return;
                    }
                    refresh_s3_browser(&area, connection, current_bucket, &list, &path_label);
                }
            ));
        }
    ));
    row.add_suffix(&delete_button);

    row
}

fn prompt_new_bucket_name(
    area: &Rc<SessionArea>,
    connection: s3::S3Connection,
    current_bucket: Rc<RefCell<Option<String>>>,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    let name_row = adw::EntryRow::builder().title("Bucket-namn").build();
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
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
        .modal(true)
        .default_width(360)
        .default_height(160)
        .title("Ny bucket")
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
        connection,
        #[strong]
        current_bucket,
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
            let rx = s3::spawn_create_bucket(connection.clone(), name);
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                connection,
                #[strong]
                current_bucket,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = rx.recv().await.unwrap_or_else(|_| Err("kanalen stängdes oväntat".to_string())) {
                        list.append(&error_row(&e));
                        return;
                    }
                    refresh_s3_browser(&area, connection, current_bucket, &list, &path_label);
                }
            ));
        }
    ));

    win.present();
}

/// Öppnar Docker-vyn för `host` i en ny flik: en containerlista med
/// start/stopp/omstart/loggar/shell per rad. Port av App/DockerView.swift
/// till en fristående SSH-engångskörning per anrop (`ssh::run_command`).
fn open_docker_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
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

    let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some("Uppdatera"));
    let toolbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    toolbar.append(
        &gtk::Label::builder()
            .label(format!("Docker: {}", host.alias))
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build(),
    );
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
        #[strong]
        jump,
        #[weak]
        list,
        move |_| refresh_docker_list(&area, host.clone(), password.clone(), &list, jump.clone())
    ));

    refresh_docker_list(area, host, password, &list, jump);
}

/// Öppnar en "Tunnel"-flik för `host`: startar/stoppar en lokal
/// portvidarebefordran (motsvarar `App/PortForwardView.swift`, bara lokal
/// `-L` hittills — se `port_forward.rs`, fjärr/dynamisk kvarstår).
fn open_port_forward_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
) {
    // "Lokal" = `ssh -L` (vi lyssnar, servern kopplar mot målet). "Fjärr" =
    // `ssh -R` (servern lyssnar åt oss, vi kopplar mot målet) — samma
    // fältuppsättning som Lokal, bara vem som lyssnar skiljer. "Dynamisk" =
    // `ssh -D` (lokal SOCKS5-proxy) — målet väljs av SOCKS-klienten per
    // anslutning, så Målvärd/Målport är meningslösa och göms.
    let direction_row = adw::ComboRow::builder().title("Riktning").build();
    let direction_model =
        gtk::StringList::new(&["Lokal (-L)", "Fjärr (-R)", "Dynamisk (-D, SOCKS5)"]);
    direction_row.set_model(Some(&direction_model));

    let bind_port_row = adw::EntryRow::builder()
        .title("Bindport (0 = valfri)")
        .build();
    let target_host_row = adw::EntryRow::builder()
        .title("Målvärd")
        .text("127.0.0.1")
        .build();
    let target_port_row = adw::EntryRow::builder().title("Målport").build();

    let group = adw::PreferencesGroup::new();
    group.add(&direction_row);
    group.add(&bind_port_row);
    group.add(&target_host_row);
    group.add(&target_port_row);

    direction_row.connect_selected_notify(clone!(
        #[strong]
        target_host_row,
        #[strong]
        target_port_row,
        move |row| {
            let is_dynamic = row.selected() == 2;
            target_host_row.set_visible(!is_dynamic);
            target_port_row.set_visible(!is_dynamic);
        }
    ));

    let status_label = gtk::Label::builder()
        .label("Inte startad")
        .margin_start(12)
        .margin_end(12)
        .halign(gtk::Align::Start)
        .build();
    let start_button = gtk::Button::with_label("Starta");
    let stop_button = gtk::Button::with_label("Stoppa");
    stop_button.set_sensitive(false);

    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    button_box.append(&start_button);
    button_box.append(&stop_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(
        &gtk::Label::builder()
            .label(format!("Tunnel: {}", host.alias))
            .margin_top(12)
            .margin_bottom(4)
            .margin_start(12)
            .halign(gtk::Align::Start)
            .css_classes(["title-4"])
            .build(),
    );
    content.append(&group);
    content.append(&button_box);
    content.append(&status_label);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Tunnel: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    let forward_handle: Rc<RefCell<Option<port_forward::ActiveForward>>> =
        Rc::new(RefCell::new(None));
    // Läses av `connect_close_page` (`SessionArea::new`) för att stoppa en
    // fortfarande aktiv vidarebefordran om användaren stänger fliken utan
    // att först trycka "Stoppa" — se kommentaren där.
    unsafe {
        content.set_data("bastion-active-forward", forward_handle.clone());
    }

    start_button.connect_clicked(clone!(
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        direction_row,
        #[strong]
        bind_port_row,
        #[strong]
        target_host_row,
        #[strong]
        target_port_row,
        #[strong]
        status_label,
        #[strong]
        start_button,
        #[strong]
        stop_button,
        #[strong]
        forward_handle,
        move |_| {
            let direction = direction_row.selected();
            let is_dynamic = direction == 2;
            let bind_port_text = bind_port_row.text();
            let bind_port: u16 = if bind_port_text.is_empty() {
                0
            } else {
                match bind_port_text.parse() {
                    Ok(p) => p,
                    Err(_) => {
                        status_label.set_label("Ogiltig bindport");
                        return;
                    }
                }
            };
            let target_host_value = target_host_row.text().to_string();
            // Dynamisk (-D) behöver inget mål — SOCKS-klienten väljer det
            // per anslutning — så målporten valideras bara för -L/-R.
            let target_port: u16 = if is_dynamic {
                0
            } else {
                match target_port_row.text().parse() {
                    Ok(p) => p,
                    Err(_) => {
                        status_label.set_label("Ogiltig målport");
                        return;
                    }
                }
            };
            status_label.set_label("Startar…");
            start_button.set_sensitive(false);
            glib::spawn_future_local(clone!(
                #[strong]
                host,
                #[strong]
                password,
                #[strong]
                jump,
                #[strong]
                status_label,
                #[strong]
                start_button,
                #[strong]
                stop_button,
                #[strong]
                forward_handle,
                async move {
                    // Tre olika kanaltyper (`LocalPortForward`/
                    // `RemotePortForward`/`DynamicPortForward`) kan inte
                    // bindas till samma `let rx = ... else ...` — de slås
                    // ihop till `ActiveForward` HÄR, inte innan, så att
                    // resten av blocket kan hantera dem enhetligt.
                    let result: Result<port_forward::ActiveForward, String> = match direction {
                        1 => {
                            let rx = port_forward::spawn_remote_forward(
                                host,
                                password,
                                "0.0.0.0".to_string(),
                                bind_port,
                                target_host_value,
                                target_port,
                                jump,
                            );
                            match rx.recv().await {
                                Ok(r) => r.map(port_forward::ActiveForward::Remote),
                                Err(_) => Err("kanalen stängdes oväntat".to_string()),
                            }
                        }
                        2 => {
                            let rx = socks_proxy::spawn_dynamic_forward(
                                host,
                                password,
                                "127.0.0.1".to_string(),
                                bind_port,
                                jump,
                            );
                            match rx.recv().await {
                                Ok(r) => r.map(port_forward::ActiveForward::Dynamic),
                                Err(_) => Err("kanalen stängdes oväntat".to_string()),
                            }
                        }
                        _ => {
                            let rx = port_forward::spawn_local_forward(
                                host,
                                password,
                                "127.0.0.1".to_string(),
                                bind_port,
                                target_host_value,
                                target_port,
                                jump,
                            );
                            match rx.recv().await {
                                Ok(r) => r.map(port_forward::ActiveForward::Local),
                                Err(_) => Err("kanalen stängdes oväntat".to_string()),
                            }
                        }
                    };
                    match result {
                        Ok(forward) => {
                            let port = forward.actual_bind_port();
                            let label = match direction {
                                1 => format!("Servern vidarebefordrar sin port {port} till oss"),
                                2 => format!("SOCKS5-proxy lyssnar på port {port}"),
                                _ => format!("Vidarebefordrar lokal port {port}"),
                            };
                            status_label.set_label(&label);
                            stop_button.set_sensitive(true);
                            *forward_handle.borrow_mut() = Some(forward);
                        }
                        Err(e) => {
                            status_label.set_label(&format!("Fel: {e}"));
                            start_button.set_sensitive(true);
                        }
                    }
                }
            ));
        }
    ));

    stop_button.connect_clicked(clone!(
        #[strong]
        forward_handle,
        #[strong]
        status_label,
        #[strong]
        start_button,
        #[strong]
        stop_button,
        move |_| {
            if let Some(forward) = forward_handle.borrow_mut().take() {
                forward.stop();
            }
            status_label.set_label("Stoppad");
            start_button.set_sensitive(true);
            stop_button.set_sensitive(false);
        }
    ));
}

/// Genererar en ny Ed25519-nyckel, deployerar den mot värden (via den
/// BEFINTLIGA auth-metoden) och verifierar att den fungerar — motsvarar
/// `App/KeyDeployView.swift`. Vid lyckad verifiering erbjuds att byta
/// värdens lagrade auth-metod till den nya nyckeln, så ett gammalt
/// lösenord inte behöver sparas kvar.
fn open_key_deploy_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    store: &Rc<RefCell<HostStore>>,
    jump: Option<host::Host>,
) {
    let comment_row = adw::EntryRow::builder()
        .title("Kommentar (valfri)")
        .text(format!("bastion@{}", host.alias))
        .build();
    let group = adw::PreferencesGroup::new();
    group.add(&comment_row);

    // Klistra in en REDAN BEFINTLIG privatnyckel (OpenSSH-PEM) istället för
    // att generera en ny — motsvarar Swift-sidans
    // `KeyDeployModel.importExisting`. Bara okrypterade Ed25519-nycklar
    // stöds (`key_deploy::import_existing` ger ett tydligt fel annars).
    let paste_view = gtk::TextView::builder().monospace(true).build();
    let paste_scroller = gtk::ScrolledWindow::builder()
        .child(&paste_view)
        .min_content_height(80)
        .margin_start(12)
        .margin_end(12)
        .margin_top(4)
        .build();
    let paste_placeholder = gtk::Label::builder()
        .label("Klistra in en OpenSSH-privatnyckel (-----BEGIN OPENSSH PRIVATE KEY-----…) här för att importera den istället för att generera en ny.")
        .margin_start(12)
        .margin_end(12)
        .halign(gtk::Align::Start)
        .wrap(true)
        .css_classes(["dim-label", "caption"])
        .build();

    let public_key_view = gtk::TextView::builder()
        .editable(false)
        .wrap_mode(gtk::WrapMode::WordChar)
        .build();
    public_key_view.set_visible(false);
    let public_key_scroller = gtk::ScrolledWindow::builder()
        .child(&public_key_view)
        .min_content_height(60)
        .margin_start(12)
        .margin_end(12)
        .build();

    let status_label = gtk::Label::builder()
        .label("Inte genererad")
        .margin_start(12)
        .margin_end(12)
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();
    let generate_button = gtk::Button::with_label("Generera + deploya + verifiera");
    let import_button = gtk::Button::with_label("Importera + deploya + verifiera");
    let adopt_button = gtk::Button::with_label("Använd den nya nyckeln för den här värden");
    adopt_button.set_sensitive(false);

    let button_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .build();
    button_box.append(&generate_button);
    button_box.append(&import_button);
    button_box.append(&adopt_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(
        &gtk::Label::builder()
            .label(format!("Nyckel: {}", host.alias))
            .margin_top(12)
            .margin_bottom(4)
            .margin_start(12)
            .halign(gtk::Align::Start)
            .css_classes(["title-4"])
            .build(),
    );
    content.append(&group);
    content.append(&paste_placeholder);
    content.append(&paste_scroller);
    content.append(&button_box);
    content.append(&status_label);
    content.append(&public_key_scroller);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Nyckel: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    // Håller den nya nyckelns sökväg mellan "Generera"- och "Använd den
    // nya nyckeln"-klicken — bara `adopt_button` behöver den, och bara
    // efter en lyckad verifiering (`adopt_button` är osensitiv innan dess).
    let new_key_path: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));

    generate_button.connect_clicked(clone!(
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        comment_row,
        #[strong]
        public_key_view,
        #[strong]
        public_key_scroller,
        #[strong]
        status_label,
        #[strong]
        generate_button,
        #[strong]
        adopt_button,
        #[strong]
        new_key_path,
        move |_| {
            let pair = match key_deploy::generate_ed25519(&comment_row.text()) {
                Ok(p) => p,
                Err(e) => {
                    status_label.set_label(&format!("Fel: kunde inte generera nyckel: {e}"));
                    return;
                }
            };
            let key_path = match key_deploy::save_private_key(&pair.private_key_pem) {
                Ok(p) => p,
                Err(e) => {
                    status_label.set_label(&format!("Fel: kunde inte spara nyckeln: {e}"));
                    return;
                }
            };
            public_key_view.buffer().set_text(&pair.public_key_line);
            public_key_scroller.set_visible(true);
            public_key_view.set_visible(true);
            status_label.set_label("Deployerar och verifierar…");
            generate_button.set_sensitive(false);

            let rx = key_deploy::spawn_deploy_and_verify(
                host.clone(),
                password.clone(),
                pair.public_key_line.clone(),
                key_path.clone(),
                jump.clone(),
            );
            glib::spawn_future_local(clone!(
                #[strong]
                status_label,
                #[strong]
                generate_button,
                #[strong]
                adopt_button,
                #[strong]
                new_key_path,
                async move {
                    match rx.recv().await {
                        Ok(Ok(())) => {
                            status_label.set_label(
                                "Klart — nyckeln är deployad och verifierad att fungera.",
                            );
                            *new_key_path.borrow_mut() = Some(key_path);
                            adopt_button.set_sensitive(true);
                        }
                        Ok(Err(e)) => {
                            status_label.set_label(&format!("Fel: {e}"));
                        }
                        Err(_) => {
                            status_label.set_label("Fel: kanalen stängdes oväntat");
                        }
                    }
                    generate_button.set_sensitive(true);
                }
            ));
        }
    ));

    import_button.connect_clicked(clone!(
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        comment_row,
        #[strong]
        paste_view,
        #[strong]
        public_key_view,
        #[strong]
        public_key_scroller,
        #[strong]
        status_label,
        #[strong]
        import_button,
        #[strong]
        adopt_button,
        #[strong]
        new_key_path,
        move |_| {
            let buffer = paste_view.buffer();
            let pasted = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
            let pair = match key_deploy::import_existing(&pasted, &comment_row.text()) {
                Ok(p) => p,
                Err(e) => {
                    status_label.set_label(&format!("Fel: {e}"));
                    return;
                }
            };
            let key_path = match key_deploy::save_private_key(&pair.private_key_pem) {
                Ok(p) => p,
                Err(e) => {
                    status_label.set_label(&format!("Fel: kunde inte spara nyckeln: {e}"));
                    return;
                }
            };
            public_key_view.buffer().set_text(&pair.public_key_line);
            public_key_scroller.set_visible(true);
            public_key_view.set_visible(true);
            status_label.set_label("Deployerar och verifierar…");
            import_button.set_sensitive(false);

            let rx = key_deploy::spawn_deploy_and_verify(
                host.clone(),
                password.clone(),
                pair.public_key_line.clone(),
                key_path.clone(),
                jump.clone(),
            );
            glib::spawn_future_local(clone!(
                #[strong]
                status_label,
                #[strong]
                import_button,
                #[strong]
                adopt_button,
                #[strong]
                new_key_path,
                async move {
                    match rx.recv().await {
                        Ok(Ok(())) => {
                            status_label.set_label(
                                "Klart — den importerade nyckeln är deployad och verifierad att fungera.",
                            );
                            *new_key_path.borrow_mut() = Some(key_path);
                            adopt_button.set_sensitive(true);
                        }
                        Ok(Err(e)) => {
                            status_label.set_label(&format!("Fel: {e}"));
                        }
                        Err(_) => {
                            status_label.set_label("Fel: kanalen stängdes oväntat");
                        }
                    }
                    import_button.set_sensitive(true);
                }
            ));
        }
    ));

    adopt_button.connect_clicked(clone!(
        #[strong]
        host,
        #[strong]
        store,
        #[strong]
        status_label,
        #[strong]
        adopt_button,
        #[strong]
        new_key_path,
        move |_| {
            // Läses UTAN att tas bort — en misslyckad `upsert` (disk full,
            // rättighetsfel) ska inte förbruka sökvägen och blockera ett
            // omförsök vid nästa klick (CodeRabbit-fynd). Tas bort bara vid
            // lyckat utfall, nedan.
            let Some(key_path) = new_key_path.borrow().clone() else {
                return;
            };
            let mut updated = host.clone();
            updated.auth = host::HostAuth::KeyFile(key_path);
            match store.borrow_mut().upsert(updated) {
                Ok(()) => {
                    *new_key_path.borrow_mut() = None;
                    status_label.set_label("Värdens auth-metod är nu den nya nyckeln.");
                    adopt_button.set_sensitive(false);
                }
                Err(e) => status_label
                    .set_label(&format!("Fel: kunde inte spara den nya auth-metoden: {e}")),
            }
        }
    ));
}

/// Kör en full synkrunda (öppen-egen-`HostStore` → `sync` → stäng) på en
/// egen bakgrundstråd, motsvarande `spawn_background_sync_encrypted` men
/// för den okrypterade `FolderSyncProvider`. Två nästan identiska
/// funktioner istället för en generisk/`dyn`-baserad — `HostStore::sync`
/// tar redan `impl SyncProvider` (monomorfiserad), och bara två
/// anropsställen gör en abstraktion här till för tidig.
fn spawn_background_sync_plain(
    provider: sync::FolderSyncProvider,
) -> async_channel::Receiver<Result<(), String>> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let result = (|| -> std::io::Result<()> {
            let mut store = host::HostStore::open(host::HostStore::default_path())?;
            store.sync(&provider)
        })();
        let _ = tx.send_blocking(result.map_err(|e| e.to_string()));
    });
    rx
}

fn spawn_background_sync_encrypted(
    provider: sync_crypto::EncryptedFolderSyncProvider,
) -> async_channel::Receiver<Result<(), String>> {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || {
        let result = (|| -> std::io::Result<()> {
            let mut store = host::HostStore::open(host::HostStore::default_path())?;
            store.sync(&provider)
        })();
        let _ = tx.send_blocking(result.map_err(|e| e.to_string()));
    });
    rx
}

fn refresh_docker_list(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    list: &gtk::ListBox,
    jump: Option<host::Host>,
) {
    let rx = ssh::run_command(
        host.clone(),
        password.clone(),
        docker::list_command(true),
        jump.clone(),
    );
    glib::spawn_future_local(clone!(
        #[weak]
        list,
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        async move {
            while let Some(row) = list.row_at_index(0) {
                list.remove(&row);
            }
            match rx.recv().await {
                Ok(Ok(output)) => {
                    for container in docker::parse_list(&output) {
                        list.append(&build_container_row(
                            &area, &host, &password, &list, container, &jump,
                        ));
                    }
                }
                Ok(Err(e)) => list.append(&error_row(&e)),
                Err(_) => list.append(&error_row("SSH-anslutningen avbröts oväntat")),
            }
        }
    ));
}

fn error_row(message: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title("Fel")
        .subtitle(message)
        .build()
}

fn build_container_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    list: &gtk::ListBox,
    container: docker::DockerContainer,
    jump: &Option<host::Host>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&container.name)
        .subtitle(format!("{} — {}", container.image, container.status))
        .build();

    let suffix = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .valign(gtk::Align::Center)
        .build();

    let run_docker_action = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let list = list.clone();
        move |command: Result<String, String>| {
            let Ok(command) = command else { return };
            let rx = ssh::run_command(host.clone(), password.clone(), command, jump.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                host,
                #[strong]
                password,
                #[strong]
                jump,
                #[strong]
                list,
                async move {
                    let _ = rx.recv().await;
                    refresh_docker_list(&area, host, password, &list, jump);
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
            jump,
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
                    start_session(&area, shell_host, password.clone(), jump.clone());
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
        jump,
        #[strong]
        container,
        move |_| show_docker_logs(&area, &host, &password, &container, &jump)
    ));
    suffix.append(&logs_btn);

    row.add_suffix(&suffix);
    row
}

fn show_docker_logs(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    container: &docker::DockerContainer,
    jump: &Option<host::Host>,
) {
    let Ok(cmd) = docker::logs_command(&container.id, 200) else {
        return;
    };
    let text_view = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .build();
    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
        .modal(true)
        .default_width(700)
        .default_height(500)
        .title(format!("Loggar: {}", container.name))
        .content(&scrolled)
        .build();
    win.present();

    let rx = ssh::run_command(host.clone(), password.clone(), cmd, jump.clone());
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
        move |_| run_snippet(&area, host.clone(), password.clone(), snippet.clone(), jump.clone())
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
            if let Err(e) = snippet_store.borrow_mut().delete(snippet_id) {
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
    start_session(area, h, password, jump);
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

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
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

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
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

/// Öppnar SFTP-bläddraren för `host` i en ny flik. Port av
/// App/SFTPBrowserModel.swift (kärnfunktioner — se sftp.rs för vad som
/// medvetet är uppskjutet: chmod/chown/komprimera/packa upp).
/// Bunt av allt en SFTP-vy behöver för att köra en engångskommando över
/// den vanliga exec-kanalen (`ssh::run_command`) — komprimera/packa upp
/// har ingen egen SFTP-semantik (SFTP v3), så de shellar ut till tar/zip
/// precis som Docker-vyn shellar ut till `docker`.
#[derive(Clone)]
struct SftpContext {
    handle: sftp::SftpHandle,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
}

fn open_sftp_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
) {
    let handle = sftp::spawn(host.clone(), password.clone(), jump.clone());
    let ctx = SftpContext {
        handle,
        host,
        password,
        jump,
    };
    let current_path = Rc::new(RefCell::new(".".to_string()));

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

    let path_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    let up_button = gtk::Button::from_icon_name("go-up-symbolic");
    up_button.set_tooltip_text(Some("Upp en nivå"));
    let mkdir_button = gtk::Button::from_icon_name("folder-new-symbolic");
    mkdir_button.set_tooltip_text(Some("Ny mapp"));
    let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");

    let toolbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    toolbar.append(&up_button);
    toolbar.append(&path_label);
    toolbar.append(&mkdir_button);
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Filer: {}", ctx.host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    up_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
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
                refresh_sftp_list(
                    &area,
                    ctx.clone(),
                    current_path.clone(),
                    new_path,
                    &list,
                    &path_label,
                );
            }
        }
    ));

    mkdir_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| prompt_new_folder_name(
            &area,
            ctx.clone(),
            current_path.clone(),
            &list,
            &path_label
        )
    ));

    refresh_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
        #[strong]
        current_path,
        #[weak]
        list,
        #[weak]
        path_label,
        move |_| {
            let path = current_path.borrow().clone();
            refresh_sftp_list(
                &area,
                ctx.clone(),
                current_path.clone(),
                path,
                &list,
                &path_label,
            );
        }
    ));

    // Drag & drop-uppladdning: släpp filer/mappar från filhanteraren rakt
    // in i den aktuella katalogen — samma funktion som App/, tidigare
    // uteslutande dokumenterad som en LinuxApp-lucka (se ROADMAP.md,
    // motiveringen i sftp::upload_path_recursive).
    let drop_target = gtk::DropTarget::new(gtk::gdk::FileList::static_type(), gtk::gdk::DragAction::COPY);
    drop_target.connect_drop(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
        #[strong]
        current_path,
        #[strong]
        list,
        #[strong]
        path_label,
        move |_, value, _, _| {
            let Ok(file_list) = value.get::<gtk::gdk::FileList>() else { return false };
            let paths: Vec<std::path::PathBuf> = file_list.files().into_iter().filter_map(|f| f.path()).collect();
            if paths.is_empty() {
                return false;
            }
            let base_path = current_path.borrow().clone();
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                ctx,
                #[strong]
                current_path,
                #[strong]
                list,
                #[strong]
                path_label,
                async move {
                    for local_path in &paths {
                        let Some(name) = local_path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                            continue;
                        };
                        let remote_path = joined_path(&base_path, &name);
                        if let Err(e) = sftp::upload_path_recursive(&ctx.handle, local_path, &remote_path).await {
                            list.append(&error_row(&format!("Kunde inte ladda upp {name}: {e}")));
                        }
                    }
                    refresh_sftp_list(&area, ctx, current_path, base_path, &list, &path_label);
                }
            ));
            true
        }
    ));
    scrolled.add_controller(drop_target);

    let initial_path = current_path.borrow().clone();
    refresh_sftp_list(area, ctx, current_path, initial_path, &list, &path_label);
}

fn refresh_sftp_list(
    area: &Rc<SessionArea>,
    ctx: SftpContext,
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
            match ctx.handle.list(path.clone()).await {
                Ok(entries) => {
                    for entry in entries {
                        list.append(&build_sftp_entry_row(
                            &area,
                            ctx.clone(),
                            current_path.clone(),
                            path.clone(),
                            entry,
                            &list,
                            &path_label,
                        ));
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
    ctx: SftpContext,
    current_path: Rc<RefCell<String>>,
    path: String,
    entry: sftp::Entry,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) -> adw::ActionRow {
    let subtitle = if entry.is_dir {
        "Mapp".to_string()
    } else {
        format!("{} bytes", entry.size)
    };
    let row = adw::ActionRow::builder()
        .title(&entry.name)
        .subtitle(subtitle)
        .activatable(true)
        .build();
    let icon = gtk::Image::from_icon_name(if entry.is_dir {
        "folder-symbolic"
    } else {
        "text-x-generic-symbolic"
    });
    row.add_prefix(&icon);

    row.connect_activated(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
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
                refresh_sftp_list(
                    &area,
                    ctx.clone(),
                    current_path.clone(),
                    full_path,
                    &list,
                    &path_label,
                );
            } else {
                open_sftp_file_editor(&area, ctx.handle.clone(), full_path);
            }
        }
    ));

    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.set_tooltip_text(Some("Ta bort"));
    delete_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
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
                ctx,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                #[strong]
                path,
                async move {
                    let result = if is_dir {
                        ctx.handle.remove_dir(full_path).await
                    } else {
                        ctx.handle.remove_file(full_path).await
                    };
                    if let Err(e) = result {
                        list.append(&error_row(&e));
                        return;
                    }
                    refresh_sftp_list(&area, ctx.clone(), current_path, path, &list, &path_label);
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
        ctx,
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
        move |_| prompt_rename(
            &area,
            ctx.clone(),
            current_path.clone(),
            path.clone(),
            entry.clone(),
            &list,
            &path_label
        )
    ));

    let permissions_button = gtk::Button::from_icon_name("changes-allow-symbolic");
    permissions_button.set_tooltip_text(Some("Rättigheter/ägare"));
    permissions_button.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        ctx,
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
        move |_| prompt_permissions(
            &area,
            ctx.clone(),
            current_path.clone(),
            path.clone(),
            entry.clone(),
            &list,
            &path_label
        )
    ));

    row.add_suffix(&permissions_button);
    row.add_suffix(&rename_button);
    row.add_suffix(&delete_button);

    if entry.is_dir {
        let compress_button = gtk::Button::from_icon_name("package-x-generic-symbolic");
        compress_button.set_tooltip_text(Some("Komprimera (tar.gz)"));
        compress_button.connect_clicked(clone!(
            #[strong]
            ctx,
            #[weak]
            list,
            #[strong]
            entry,
            #[strong]
            path,
            move |_| {
                // Komprimerar mappens INNEHÅLL (`.` inifrån mappen själv),
                // arkivet hamnar bredvid mappen (i `path`, inte inuti den).
                let full_dir = joined_path(&path, &entry.name);
                let archive_name = format!("../{}.tar.gz", entry.name);
                let command =
                    archive::create_tar_gz_command(&[".".to_string()], &archive_name, &full_dir);
                let rx = ssh::run_command(ctx.host.clone(), ctx.password.clone(), command, ctx.jump.clone());
                glib::spawn_future_local(async move {
                    if let Ok(Err(e)) = rx.recv().await {
                        list.append(&error_row(&e));
                    }
                });
            }
        ));
        row.add_suffix(&compress_button);
    } else if entry.name.ends_with(".tar.gz")
        || entry.name.ends_with(".tgz")
        || entry.name.ends_with(".zip")
    {
        let extract_button = gtk::Button::from_icon_name("package-x-generic-symbolic");
        extract_button.set_tooltip_text(Some("Packa upp"));
        extract_button.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            ctx,
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
                let command = if entry.name.ends_with(".zip") {
                    archive::extract_zip_command(&entry.name, &path)
                } else {
                    archive::extract_tar_gz_command(&entry.name, &path)
                };
                let rx = ssh::run_command(ctx.host.clone(), ctx.password.clone(), command, ctx.jump.clone());
                glib::spawn_future_local(clone!(
                    #[strong]
                    area,
                    #[strong]
                    ctx,
                    #[strong]
                    current_path,
                    #[weak]
                    list,
                    #[weak]
                    path_label,
                    #[strong]
                    path,
                    async move {
                        if let Ok(Err(e)) = rx.recv().await {
                            list.append(&error_row(&e));
                            return;
                        }
                        refresh_sftp_list(&area, ctx, current_path, path, &list, &path_label);
                    }
                ));
            }
        ));
        row.add_suffix(&extract_button);
    }

    row
}

fn prompt_new_folder_name(
    area: &Rc<SessionArea>,
    ctx: SftpContext,
    current_path: Rc<RefCell<String>>,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    let name_row = adw::EntryRow::builder().title("Mappnamn").build();
    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let create_button = gtk::Button::with_label("Skapa");
    create_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&create_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
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
        ctx,
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
                ctx,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = ctx.handle.mkdir(full_path).await {
                        list.append(&error_row(&e));
                        return;
                    }
                    let path = current_path.borrow().clone();
                    refresh_sftp_list(&area, ctx.clone(), current_path, path, &list, &path_label);
                }
            ));
        }
    ));

    win.present();
}

fn prompt_rename(
    area: &Rc<SessionArea>,
    ctx: SftpContext,
    current_path: Rc<RefCell<String>>,
    path: String,
    entry: sftp::Entry,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    let name_row = adw::EntryRow::builder()
        .title("Nytt namn")
        .text(&entry.name)
        .build();
    let group = adw::PreferencesGroup::new();
    group.add(&name_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let save_button = gtk::Button::with_label("Döp om");
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

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
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
        ctx,
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
                ctx,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                async move {
                    if let Err(e) = ctx.handle.rename(from, to).await {
                        list.append(&error_row(&e));
                        return;
                    }
                    let path = current_path.borrow().clone();
                    refresh_sftp_list(&area, ctx.clone(), current_path, path, &list, &path_label);
                }
            ));
        }
    ));

    win.present();
}

/// Rättigheter (oktalt läge, t.ex. 755) + ägare (numeriskt UID/GID) — port
/// av Swiftsidans SFTPClient.setPermissions/chown, samma dialogmönster
/// som döp om/ny mapp.
fn prompt_permissions(
    area: &Rc<SessionArea>,
    ctx: SftpContext,
    current_path: Rc<RefCell<String>>,
    path: String,
    entry: sftp::Entry,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    let mode_row = adw::EntryRow::builder()
        .title("Rättigheter (oktalt, t.ex. 755)")
        .build();
    let uid_row = adw::EntryRow::builder()
        .title("UID (lämna tomt för att inte ändra)")
        .build();
    let gid_row = adw::EntryRow::builder()
        .title("GID (lämna tomt för att inte ändra)")
        .build();

    let group = adw::PreferencesGroup::builder().title(&entry.name).build();
    group.add(&mode_row);
    group.add(&uid_row);
    group.add(&gid_row);
    let page = adw::PreferencesPage::new();
    page.add(&group);

    let apply_button = gtk::Button::with_label("Verkställ");
    apply_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&apply_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
        .modal(true)
        .default_width(420)
        .default_height(280)
        .title("Rättigheter/ägare")
        .content(&content)
        .build();

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    apply_button.connect_clicked(clone!(
        #[weak]
        win,
        #[strong]
        area,
        #[strong]
        ctx,
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
            win.close();
            let full_path = joined_path(&path, &entry.name);
            let mode = u32::from_str_radix(mode_row.text().trim(), 8).ok();
            let uid: Option<u32> = uid_row.text().trim().parse().ok();
            let gid: Option<u32> = gid_row.text().trim().parse().ok();

            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                ctx,
                #[strong]
                current_path,
                #[weak]
                list,
                #[weak]
                path_label,
                #[strong]
                path,
                async move {
                    if let Some(mode) = mode {
                        if let Err(e) = ctx.handle.chmod(full_path.clone(), mode).await {
                            list.append(&error_row(&e));
                            return;
                        }
                    }
                    if let (Some(uid), Some(gid)) = (uid, gid) {
                        if let Err(e) = ctx.handle.chown(full_path, uid, gid).await {
                            list.append(&error_row(&e));
                            return;
                        }
                    }
                    refresh_sftp_list(&area, ctx, current_path, path, &list, &path_label);
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
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .build();

    let save_button = gtk::Button::with_label("Spara");
    save_button.add_css_class("suggested-action");
    save_button.set_sensitive(false);
    let close_button = gtk::Button::with_label("Stäng");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .title_widget(&gtk::Label::new(Some(&path)))
        .build();
    header.pack_start(&close_button);
    header.pack_end(&save_button);

    let save_status_label = gtk::Label::builder()
        .halign(gtk::Align::Start)
        .margin_start(12)
        .margin_top(4)
        .visible(false)
        .css_classes(["error"])
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&save_status_label);
    content.append(&scrolled);

    let win = adw::Window::builder()
        .transient_for(
            &area
                .overlay
                .root()
                .and_downcast::<gtk::Window>()
                .expect("inget fönster"),
        )
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
        #[strong]
        save_status_label,
        move |_| {
            let buffer = text_view.buffer();
            let text = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            save_status_label.set_visible(false);
            glib::spawn_future_local(clone!(
                #[strong]
                handle,
                #[strong]
                path,
                #[strong]
                save_status_label,
                async move {
                    // Ett `let _ =` här dolde tidigare rättighets-/diskfulla
                    // fel helt — filen såg "sparad" ut för användaren trots
                    // att skrivningen avvisades (CodeRabbit-fynd).
                    if let Err(e) = handle.write(path, text.into_bytes()).await {
                        save_status_label.set_text(&format!("Kunde inte spara: {e}"));
                        save_status_label.set_visible(true);
                    }
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
