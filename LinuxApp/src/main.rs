use adw::prelude::*;
use gtk::glib;
use gtk::glib::clone;
use std::cell::RefCell;
use std::rc::Rc;
use vte::prelude::*;

mod archive;
mod bitwarden;
mod bookmarks;
mod command_library;
mod dashboard;
mod docker;
mod external_binary_fetcher;
mod fsutil;
mod fuzzy;
mod host;
mod host_grouping;
mod key_deploy;
mod known_hosts;
mod kubernetes;
mod oauth;
mod palette_actions;
mod port_forward;
mod s3;
mod serial;
mod settings;
mod sftp;
mod snippet;
mod socks_proxy;
mod split;
mod ssh;
mod tab_title;
#[cfg(test)]
mod test_support;
mod ssh_config;
mod sync;
mod sync_crypto;
mod tailscale;
mod telnet;
mod terminal_theme;
mod vault;
mod wake_on_lan;
mod wireguard;

use host::{Host, HostStore};
use ssh::SshEvent;

const APP_ID: &str = "se.denied.bastion";

/// De sju namngivna färgerna för `Host::color_tag` (`host.rs`) — fältet har
/// funnits i datamodellen sedan starten (wire-kompatibelt med Swift-sidans
/// `colorTag`, se `Host.swift`), men hade INGEN UI-koppling alls i
/// `LinuxApp` innan detta: varken en väljare i värdredigeringsdialogen
/// eller en visuell markering i värdlistan. Samma slags "fältet finns,
/// ingen använder det"-gap som certifikatautentiseringen. Hex-värdena är
/// GNOME/Adwaitas egna namngivna accentfärger (inte gissade), samma
/// paletturval (röd/orange/gul/grön/blå/lila/grå) som `App/HostColor.swift`s
/// `HostColorPalette`.
const HOST_COLORS: &[(&str, &str)] = &[
    ("red", "#e01b24"),
    ("orange", "#ff7800"),
    ("yellow", "#f6d32d"),
    ("green", "#2ec27e"),
    ("blue", "#3584e4"),
    ("purple", "#9141ac"),
    ("gray", "#9a9996"),
];

/// Laddar de sju `.host-color-<namn>`-CSS-klasserna EN gång, globalt för
/// hela displayen. GTK4 har (till skillnad från GTK3) ingen per-widget
/// `style_context()` längre — en delad `CssProvider` med fasta klasser är
/// rätt väg för ett litet, fast antal namngivna färger som denna.
fn load_host_color_css() {
    let css = HOST_COLORS
        .iter()
        .map(|(name, hex)| format!(".host-color-{name} {{ background-color: {hex}; border-radius: 9999px; }}"))
        .collect::<Vec<_>>()
        .join("\n");
    let provider = gtk::CssProvider::new();
    provider.load_from_string(&css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("ingen display tillgänglig"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Motsvarar `HostRow`s prefix-cirkel i `App/HostListView.swift`. `None`
/// (ospecificerat/okänt namn) ger ingen widget alls, inte en tom/grå
/// cirkel — samma tystnad som Swift-sidans `if let color = ...`.
fn host_color_dot(color_tag: &Option<String>) -> Option<gtk::Widget> {
    let name = color_tag.as_deref()?;
    if !HOST_COLORS.iter().any(|(n, _)| *n == name) {
        return None;
    }
    let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    dot.set_size_request(10, 10);
    dot.set_valign(gtk::Align::Center);
    dot.add_css_class(&format!("host-color-{name}"));
    Some(dot.upcast())
}

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    load_host_color_css();
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

    // Delat sökfilter-tillstånd — `refresh_list` läser det själv i stället
    // för att söktexten skulle behöva trädas igenom varje enskild
    // anropsplats separat (samma mönster som `settings_store`/
    // `snippet_store`s delade `Rc<RefCell<...>>`-tillstånd).
    let search_query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let area = SessionArea::new();
    refresh_list(&list, &store, app, &area, &settings_store, &snippet_store, &search_query);

    let search_entry = gtk::SearchEntry::builder()
        .placeholder_text("Sök (alias, värdnamn, användare, taggar)")
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    search_entry.connect_search_changed(clone!(
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
        #[strong]
        search_query,
        move |entry| {
            *search_query.borrow_mut() = entry.text().to_string();
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
        }
    ));

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();

    // ── Valvet ────────────────────────────────────────────────────────
    //
    // Allt appen har SPARAT, i en och samma sidopanel: värdar, WireGuard-
    // profiler, S3-anslutningar och kända värdnycklar. Se `vault.rs` för
    // varför (kort: WireGuard och S3 låg i var sitt dialogfönster bakom
    // primärmenyn, och kända värdar hade ingen yta alls). Sessionerna
    // ligger kvar till höger — det är just den uppdelningen valvet
    // handlar om.
    let vault_stack = gtk::Stack::new();

    let hosts_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hosts_page.append(&search_entry);
    hosts_page.append(&scrolled);
    vault_stack.add_named(&hosts_page, Some("hosts"));

    let wireguard_list = vault_list_box();
    vault_stack.add_named(&vault_page(&wireguard_list), Some("wireguard"));
    refresh_wireguard_profile_list(app, &wireguard_store, &wireguard_list);

    let s3_list = vault_list_box();
    vault_stack.add_named(&vault_page(&s3_list), Some("s3"));
    refresh_s3_connection_list(app, &area, &s3_store, &s3_list);

    let known_hosts_list = vault_list_box();
    vault_stack.add_named(&vault_page(&known_hosts_list), Some("known-hosts"));
    refresh_known_hosts_list(&known_hosts_list);

    let category_dropdown = gtk::DropDown::builder()
        .model(&gtk::StringList::new(&vault::labels()))
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .tooltip_text("Vad sidopanelen visar")
        .build();

    // Åtgärderna ligger på APPEN, inte i en grupp på en widget. Skälet är
    // tangentbordet: `set_accels_for_action` når bara `app.`- och
    // `win.`-prefixade åtgärder, inte en egen grupp inlagd på en behållare.
    // Med appen som hem delar meny, knapp och kortkommando exakt samma
    // definition — och GTK ritar dessutom ut kortkommandot bredvid
    // menyposten automatiskt, utan att texten skrivs in för hand.
    let new_host_action = gtk::gio::SimpleAction::new("new-host", None);
    new_host_action.connect_activate(clone!(
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
        #[strong]
        search_query,
        #[weak]
        category_dropdown,
        move |_, _| {
            // Samma skäl som `focus-search`: den nya värden ska hamna i
            // en lista som faktiskt syns.
            select_vault_category(&category_dropdown, "hosts");
            show_host_dialog(
                &app,
                &store,
                &list,
                &area,
                &settings_store,
                &snippet_store,
                &search_query,
                None,
            );
        }
    ));
    app.add_action(&new_host_action);

    let add_button = gtk::Button::from_icon_name("list-add-symbolic");

    // Kategorin styr både vad sidopanelen visar och vad `+` betyder.
    // Knappen får en ny åtgärd i stället för en förgrening i sin
    // klickhanterare — då fortsätter menyn, paletten och kortkommandot
    // att dela definition med den (se kommentaren om `app.`-åtgärder
    // ovan).
    let apply_vault_category = clone!(
        #[weak]
        vault_stack,
        #[weak]
        add_button,
        move |index: u32| {
            let Some(category) = vault::at(index as usize) else {
                return;
            };
            vault_stack.set_visible_child_name(category.id);
            match &category.add {
                Some(add) => {
                    add_button.set_visible(true);
                    add_button.set_tooltip_text(Some(add.tooltip));
                    add_button.set_action_name(Some(add.action));
                }
                // Kända värdar: raderna hamnar där genom att man
                // ansluter. En `+` hade lovat något som inte finns.
                None => add_button.set_visible(false),
            }
        }
    );
    apply_vault_category(category_dropdown.selected());
    category_dropdown.connect_selected_notify(move |dropdown| {
        apply_vault_category(dropdown.selected());
    });

    let quick_connect_action = gtk::gio::SimpleAction::new("new-connection", None);
    quick_connect_action.connect_activate(clone!(
        #[weak]
        app,
        #[strong]
        area,
        move |_, _| show_quick_connect_dialog(&app, &area)
    ));
    app.add_action(&quick_connect_action);

    let quick_connect_button = gtk::Button::from_icon_name("system-run-symbolic");
    quick_connect_button.set_tooltip_text(Some("Snabbanslutning"));
    quick_connect_button.set_action_name(Some("app.new-connection"));

    // Nio ikonknappar fick inte plats i sidopanelens header: rubriken
    // "Värdar" klipptes till "Vär…" (hittat genom att faktiskt köra appen,
    // se ROADMAP-posten om ikonbuggen). Bara de två vanligaste åtgärderna
    // — lägg till värd och snabbanslutning — står kvar som egna knappar;
    // resten flyttas in i en primärmeny, vilket dessutom är GNOME:s eget
    // mönster så fort en header behöver mer än ett par åtgärder.
    // Menyposterna körs som `SimpleAction`-poster på appen (se längre upp),
    // så samma definition driver meny, knapp och kortkommando.
    let import_action = gtk::gio::SimpleAction::new("import_ssh_config", None);
    import_action.connect_activate(clone!(
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
        #[strong]
        search_query,
        move |_, _| show_ssh_config_import_dialog(&app, &store, &list, &area, &settings_store, &snippet_store, &search_query)
    ));
    app.add_action(&import_action);

    let telnet_action = gtk::gio::SimpleAction::new("telnet", None);
    telnet_action.connect_activate(clone!(
        #[weak]
        app,
        #[strong]
        area,
        move |_, _| show_telnet_connect_dialog(&app, &area)
    ));
    app.add_action(&telnet_action);

    let serial_action = gtk::gio::SimpleAction::new("serial", None);
    serial_action.connect_activate(clone!(
        #[weak]
        app,
        #[strong]
        area,
        move |_, _| show_serial_connect_dialog(&app, &area)
    ));
    app.add_action(&serial_action);

    let tailscale_action = gtk::gio::SimpleAction::new("tailscale", None);
    tailscale_action.connect_activate(clone!(
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
        #[strong]
        search_query,
        move |_, _| show_tailscale_discovery_dialog(
            &app,
            &store,
            &list,
            &area,
            &settings_store,
            &snippet_store,
            &search_query
        )
    ));
    app.add_action(&tailscale_action);

    // De två åtgärderna öppnade förr var sitt dialogfönster. Nu VÄLJER de
    // kategori i valvet i stället — samma menypost och samma palettrad
    // leder till samma sak, bara utan ett fönster till att stänga.
    let wireguard_action = gtk::gio::SimpleAction::new("wireguard", None);
    wireguard_action.connect_activate(clone!(
        #[weak]
        category_dropdown,
        move |_, _| select_vault_category(&category_dropdown, "wireguard")
    ));
    app.add_action(&wireguard_action);

    let s3_action = gtk::gio::SimpleAction::new("s3", None);
    s3_action.connect_activate(clone!(
        #[weak]
        category_dropdown,
        move |_, _| select_vault_category(&category_dropdown, "s3")
    ));
    app.add_action(&s3_action);

    let known_hosts_action = gtk::gio::SimpleAction::new("known-hosts", None);
    known_hosts_action.connect_activate(clone!(
        #[weak]
        category_dropdown,
        #[weak]
        known_hosts_list,
        move |_, _| {
            // Listan läses om vid varje besök: sessioner lär in nya
            // värdnycklar medan appen kör, och en lista som visar läget
            // vid uppstart hade varit fel utan att synas vara det.
            refresh_known_hosts_list(&known_hosts_list);
            select_vault_category(&category_dropdown, "known-hosts");
        }
    ));
    app.add_action(&known_hosts_action);

    // `+`-knappens innebörd följer kategorin. Egna åtgärder i stället för
    // en förgrening i knappens klickhanterare, så att de också går att nå
    // från paletten och menyn.
    let new_wireguard_action = gtk::gio::SimpleAction::new("new-wireguard", None);
    new_wireguard_action.connect_activate(clone!(
        #[weak]
        app,
        #[strong]
        wireguard_store,
        #[weak]
        wireguard_list,
        move |_, _| show_wireguard_profile_edit(
            &app,
            &wireguard_store,
            &wireguard_list,
            wireguard::WireGuardProfile::new(String::new(), wireguard::WireGuardConfig::default()),
        )
    ));
    app.add_action(&new_wireguard_action);

    let new_s3_action = gtk::gio::SimpleAction::new("new-s3", None);
    new_s3_action.connect_activate(clone!(
        #[weak]
        app,
        #[strong]
        area,
        #[strong]
        s3_store,
        #[weak]
        s3_list,
        move |_, _| show_s3_connection_edit(
            &app,
            &area,
            &s3_store,
            &s3_list,
            s3::S3Connection::new(
                String::new(),
                String::new(),
                "us-east-1".to_string(),
                String::new(),
                String::new(),
            ),
        )
    ));
    app.add_action(&new_s3_action);

    let settings_action = gtk::gio::SimpleAction::new("settings", None);
    settings_action.connect_activate(clone!(
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
        #[strong]
        search_query,
        move |_, _| show_settings_dialog(
            &app,
            &settings_store,
            &store,
            &list,
            &area,
            &snippet_store,
            &sync_config,
            &search_query
        )
    ));
    app.add_action(&settings_action);

    let palette_action = gtk::gio::SimpleAction::new("palette", None);
    palette_action.connect_activate(clone!(
        #[weak]
        app,
        #[strong]
        store,
        #[strong]
        area,
        move |_, _| show_command_palette(&app, &store, &area)
    ));
    app.add_action(&palette_action);

    let close_tab_action = gtk::gio::SimpleAction::new("close-tab", None);
    close_tab_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, _| {
            // `n_pages` kollas FÖRST: `selected_page` kan hinna svara med
            // en sida som redan är på väg ut om kommandot trycks två
            // gånger snabbt, och `close_page` på en sida som inte längre
            // hör till vyn ger en Adwaita-CRITICAL.
            if area.tab_view.n_pages() == 0 {
                return;
            }
            if let Some(page) = area.tab_view.selected_page() {
                area.tab_view.close_page(&page);
            }
        }
    ));
    app.add_action(&close_tab_action);

    let focus_search_action = gtk::gio::SimpleAction::new("focus-search", None);
    focus_search_action.connect_activate(clone!(
        #[weak]
        search_entry,
        #[weak]
        category_dropdown,
        move |_, _| {
            // Sökrutan bor i valvets värdkategori. Står panelen på en
            // annan kategori är rutan inte bara ofokuserad utan DOLD, och
            // `grab_focus` på en dold widget misslyckas tyst — kommandot
            // hade sett trasigt ut. Byt kategori först.
            select_vault_category(&category_dropdown, "hosts");
            search_entry.grab_focus();
        }
    ));
    app.add_action(&focus_search_action);

    // Delad vy. Riktningen ligger i åtgärdsnamnet och inte som parameter,
    // eftersom de två är olika saker i menyn och har var sitt
    // kortkommando — en parametriserad åtgärd hade bara flyttat samma två
    // rader någon annanstans.
    let split_right_action = gtk::gio::SimpleAction::new("split-right", None);
    split_right_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, _| split_focused_pane(&area, gtk::Orientation::Horizontal)
    ));
    app.add_action(&split_right_action);

    let split_down_action = gtk::gio::SimpleAction::new("split-down", None);
    split_down_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, _| split_focused_pane(&area, gtk::Orientation::Vertical)
    ));
    app.add_action(&split_down_action);

    let close_pane_action = gtk::gio::SimpleAction::new("close-pane", None);
    close_pane_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, _| {
            if area.tab_view.n_pages() == 0 {
                return; // samma skydd som `app.close-tab`, se ovan
            }
            close_focused_pane(&area);
        }
    ));
    app.add_action(&close_pane_action);

    // Log-bokmärken. Två åtgärder och inte en: att SÄTTA ett bokmärke ska
    // gå utan att något öppnas — man trycker mitt i en körning man tittar
    // på — medan att hitta tillbaka kräver listan.
    let bookmark_add_action = gtk::gio::SimpleAction::new("bookmark-add", None);
    bookmark_add_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, _| bookmark_focused_pane(&area)
    ));
    app.add_action(&bookmark_add_action);

    let bookmark_list_action = gtk::gio::SimpleAction::new("bookmark-list", None);
    bookmark_list_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, _| show_bookmarks_dialog(&area)
    ));
    app.add_action(&bookmark_list_action);

    // Rutåtgärderna betyder ingenting utan en öppen session. I stället
    // för att tyst inte göra något gråas de ut i menyn, vilket GTK sköter
    // självt så fort åtgärden är avstängd.
    let session_actions = vec![
        split_right_action,
        split_down_action,
        close_pane_action,
        bookmark_add_action,
        bookmark_list_action,
    ];
    let set_session_actions_enabled = move |enabled: bool| {
        for action in &session_actions {
            action.set_enabled(enabled);
        }
    };
    set_session_actions_enabled(area.tab_view.n_pages() > 0);
    area.tab_view
        .connect_n_pages_notify(move |view| set_session_actions_enabled(view.n_pages() > 0));

    // En enda parametriserad åtgärd i stället för nio nästan identiska:
    // `app.select-tab(3)` är samma sak för menyn, tangentbordet och en
    // framtida kommandopalett.
    let select_tab_action =
        gtk::gio::SimpleAction::new("select-tab", Some(glib::VariantTy::INT32));
    select_tab_action.connect_activate(clone!(
        #[strong]
        area,
        move |_, parameter| {
            let Some(number) = parameter.and_then(|p| p.get::<i32>()) else {
                return;
            };
            // `AdwTabView` har ingen `nth_page` — sidorna nås via
            // `pages()`, som är en `gio::ListModel`.
            //
            // Indexet måste kontrolleras SJÄLV. `ListModel::item` brukar
            // svara `None` utanför intervallet, men den här modellen går
            // via `adw_tab_view_get_nth_page`, som i stället loggar en
            // Adwaita-CRITICAL. Upptäckt genom att trycka Alt+2 med bara
            // en flik öppen och läsa appens logg — inget syntes i UI:t.
            let index = (number - 1).max(0) as u32;
            let pages = area.tab_view.pages();
            if index >= pages.n_items() {
                return; // färre flikar öppna än siffran som trycktes
            }
            let Some(page) = pages.item(index).and_downcast::<adw::TabPage>() else {
                return;
            };
            area.tab_view.set_selected_page(&page);
        }
    ));
    app.add_action(&select_tab_action);

    // Kortkommandon.
    //
    // MEDVETET `Ctrl+Shift` och `Alt+siffra`, inte `Ctrl`+bokstav. Appen
    // är en terminal: `Ctrl+T`, `Ctrl+W` och `Ctrl+K` är readlines egna
    // (transponera tecken, radera ord bakåt, klipp rad) och tar appen dem
    // försvinner de ur skalet på andra sidan. Det är samma val som
    // GNOME Terminal och Ptyxis gör. Termius använder `Ctrl+T`/`Ctrl+J`
    // — här är det avsiktligt INTE härmat.
    app.set_accels_for_action("app.palette", &["<Ctrl><Shift>p"]);
    app.set_accels_for_action("app.new-host", &["<Ctrl><Shift>n"]);
    app.set_accels_for_action("app.new-connection", &["<Ctrl><Shift>t"]);
    app.set_accels_for_action("app.close-tab", &["<Ctrl><Shift>w"]);
    app.set_accels_for_action("app.focus-search", &["<Ctrl><Shift>f"]);
    // `E` och `O` för delning är Terminators, Tilix och GNOME Terminals
    // gemensamma val — den som redan delar rutor på Linux har dem i
    // fingrarna. Deras egna namn ("vertikalt"/"horisontellt") syftar på
    // AVDELAREN och betyder motsatsen till vad de flesta gissar, så
    // menyposterna här heter efter RIKTNINGEN i stället.
    app.set_accels_for_action("app.split-right", &["<Ctrl><Shift>e"]);
    app.set_accels_for_action("app.split-down", &["<Ctrl><Shift>o"]);
    app.set_accels_for_action("app.close-pane", &["<Ctrl><Shift>x"]);
    // `D` för att sätta ett bokmärke och `B` för att lista dem. `Ctrl+B`
    // utan Shift är tmux prefixtangent och `Ctrl+D` skickar EOF — båda
    // hade tagits från skalet, samma resonemang som ovan.
    app.set_accels_for_action("app.bookmark-add", &["<Ctrl><Shift>d"]);
    app.set_accels_for_action("app.bookmark-list", &["<Ctrl><Shift>b"]);
    app.set_accels_for_action("app.settings", &["<Ctrl>comma"]);
    for number in 1..=9 {
        app.set_accels_for_action(
            &format!("app.select-tab({number})"),
            &[&format!("<Alt>{number}")],
        );
    }

    let sidebar_menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .tooltip_text("Huvudmeny")
        .menu_model(&sidebar_menu())
        .build();

    let sidebar_header = adw::HeaderBar::new();
    sidebar_header.pack_end(&sidebar_menu_button);
    sidebar_header.pack_end(&add_button);
    sidebar_header.pack_end(&quick_connect_button);

    let sidebar_content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    sidebar_content.append(&sidebar_header);
    sidebar_content.append(&category_dropdown);
    sidebar_content.append(&vault_stack);

    let sidebar_page = adw::NavigationPage::builder()
        .title("Valv")
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

    // (Radaktivering kopplas PER RAD i `refresh_list`, med värdens `id`
    // fångat — se kommentaren där om varför ett index-baserat uppslag mot
    // `store.all()` slutade fungera när listan blev sektionerad.)

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
    search_query: &Rc<RefCell<String>>,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    let toggles = settings_store.borrow().current();
    let all_hosts: Vec<Host> = store.borrow().all().into_iter().cloned().collect();
    let query = search_query.borrow().clone();
    let groups = host_grouping::filter_groups(host_grouping::grouped_hosts(&all_hosts), &query);
    for (section_title, section_hosts) in &groups {
        let header = gtk::Label::builder()
            .label(section_title)
            .halign(gtk::Align::Start)
            .margin_start(4)
            .margin_top(if list.row_at_index(0).is_some() { 12 } else { 0 })
            .css_classes(["heading", "dim-label"])
            .build();
        let header_row = gtk::ListBoxRow::builder().activatable(false).selectable(false).child(&header).build();
        list.append(&header_row);
        for h in section_hosts {
        let subtitle = if h.tags.is_empty() {
            format!("{}@{}:{}", h.user, h.host_name, h.port)
        } else {
            format!("{}@{}:{} · {}", h.user, h.host_name, h.port, h.tags.join(", "))
        };
        let row = adw::ActionRow::builder()
            .title(&h.alias)
            .subtitle(subtitle)
            .activatable(true)
            .build();

        if let Some(dot) = host_color_dot(&h.color_tag) {
            row.add_prefix(&dot);
        }

        // Favorit-stjärna: sparas direkt vid klick, samma "Favorit"/"Ta
        // bort favorit"-växling som App/HostListView.swifts context-meny
        // (ikonnamnen är GNOME/Adwaitas standardnamn för fyllt/ofyllt stjärn-
        // märke, samma som Nautilus/GNOME Webs bokmärkesknappar använder).
        let favorite_button = gtk::ToggleButton::builder()
            .icon_name(if h.is_favorite { "starred-symbolic" } else { "non-starred-symbolic" })
            .valign(gtk::Align::Center)
            .css_classes(["flat"])
            .active(h.is_favorite)
            .build();
        favorite_button.connect_toggled(clone!(
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
            #[strong]
            search_query,
            #[strong(rename_to = host_id)]
            h.id,
            move |btn| {
                let is_favorite = btn.is_active();
                let existing = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                let Some(mut host) = existing else { return };
                // Ingen skillnad mot det sparade värdet → gör ingenting.
                // Skyddar mot en omtoggling som bara speglar en redan
                // sparad ändring (och därmed mot att `refresh_list`
                // nedan skulle kunna trigga sig själv i en loop).
                if host.is_favorite == is_favorite {
                    return;
                }
                host.is_favorite = is_favorite;
                if let Err(e) = store.borrow_mut().upsert(host) {
                    eprintln!("kunde inte spara favorit-status: {e}");
                    return;
                }
                // Bygg om listan så värden faktiskt flyttas in i/ut ur
                // "★ Favoriter"-sektionen direkt — samma beteende som
                // Swift-sidans `toggleFavorite` → `save` → `reload()`.
                // UPPSKJUTET till nästa huvudloopsvarv (`idle_add_local_once`)
                // i stället för att köras rakt av: `refresh_list` river
                // ut ALLA rader, inklusive den rad vars knapp just nu står
                // mitt i sin egen signalhantering. GTK håller visserligen
                // en referens under signalemissionen (ingen use-after-free),
                // men att låta hanteraren returnera FÖRST och rebuilda
                // efteråt undviker hela frågan.
                glib::idle_add_local_once(clone!(
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
                    #[strong]
                    search_query,
                    move || {
                        refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
                    }
                ));
            }
        ));
        row.add_suffix(&favorite_button);

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
            #[strong]
            search_query,
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
                        &search_query,
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
            #[strong]
            search_query,
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
                refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
            }
        ));
        let dashboard_action = gtk::gio::SimpleAction::new("dashboard", None);
        dashboard_action.connect_activate(clone!(
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
                                open_dashboard_view(area, host, password, jump.clone())
                            });
                        }
                    ));
                }
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
        let kubernetes_action = gtk::gio::SimpleAction::new("kubernetes", None);
        kubernetes_action.connect_activate(clone!(
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
                                open_kubernetes_view(area, host, password, jump.clone())
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
        action_group.add_action(&dashboard_action);
        action_group.add_action(&docker_action);
        action_group.add_action(&kubernetes_action);
        action_group.add_action(&commands_action);
        action_group.add_action(&sftp_action);
        action_group.add_action(&forward_action);
        action_group.add_action(&key_deploy_action);
        row.insert_action_group("host", Some(&action_group));

        // Anslut PER RAD, med värdens `id` fångat direkt — INTE via en
        // `ListBox::connect_row_activated` som slår upp `store.all()`
        // på radens INDEX. Det senare fungerade bara så länge listan var
        // platt och i exakt samma ordning som `all()`: efter
        // sektioneringen (rubrikrader, alias-sortering inom varje
        // sektion, favoriter urplockade ur sina taggsektioner, en
        // multi-taggad värd som förekommer i FLERA sektioner) — och
        // dessutom när sökfältet filtrerar bort rader — pekar index inte
        // längre på rätt värd, så ett klick öppnade en session mot FEL
        // värd. `id`-uppslag är dessutom robust mot att listan hinner
        // ändras mellan att raden byggs och att den klickas.
        row.connect_activated(clone!(
            #[strong]
            store,
            #[strong]
            area,
            #[strong(rename_to = host_id)]
            h.id,
            move |_| {
                let host = store.borrow().all().iter().find(|x| x.id == host_id).map(|h| (*h).clone());
                if let Some(host) = host {
                    open_session(&area, &store, host);
                }
            }
        ));

        list.append(&row);
        }
    }
}

/// `mac_address`: bara den AKTUELLA raden vet om Wake-on-LAN är meningsfullt
/// — till skillnad från de andra posterna (styrda av globala
/// `FeatureToggles`) är "Väck"-posten en PER-VÄRD-egenskap, samma UX-regel
/// som App/HostListView.swift (`if host.macAddress != nil`).
/// Hur ett dialogfönster tar plats — en AVSIKT, inte en mätning.
///
/// Appen har ett tjugotal dialoger. Tidigare byggde var och en sitt eget
/// `adw::Window` med ett handplockat `default_width`/`default_height`-par,
/// vilket betyder att varje justering av hur dialoger ser ut är tjugo
/// ändringar — och att sex av dem hann glida ifrån och sakna titel helt
/// (de skyltade "bastion-linuxapp" i sin header). Här ligger pixlarna på
/// ETT ställe, och `dialog_window` är enda vägen in.
///
/// Formulärvarianterna sätter avsiktligt INGEN höjd: `adw::PreferencesPage`
/// rapporterar sin naturliga höjd, så fönstret växer med innehållet. Det
/// gör att en ny rad i ett formulär syns direkt utan att någon behöver
/// leta rätt på en siffra att höja — vilket var precis vad som hänt med
/// "Lägg till värd", där fem av tretton rader låg utanför fönstret.
#[derive(Clone, Copy)]
enum DialogSize {
    /// Kort formulär: lösenordsprompt, "Ny mapp", enkla anslutningsformulär.
    Compact,
    /// Vanligt formulär med flera rader.
    Form,
    /// Lista eller annat skrollbart innehåll. Behöver en starthöjd — en
    /// `gtk::ScrolledWindow` har ingen naturlig höjd att växa efter.
    List,
    /// Stor innehållsvy: loggar, filredigerare.
    Viewer,
}

impl DialogSize {
    fn width(self) -> i32 {
        match self {
            DialogSize::Compact => 400,
            DialogSize::Form => 480,
            DialogSize::List => 480,
            DialogSize::Viewer => 760,
        }
    }

    /// `None` = låt innehållet bestämma (se `content_height`).
    fn height(self) -> Option<i32> {
        match self {
            DialogSize::Compact | DialogSize::Form => None,
            DialogSize::List => Some(520),
            DialogSize::Viewer => Some(560),
        }
    }
}

/// Innehållets egen naturliga höjd, men aldrig högre än vad skärmen
/// rymmer.
///
/// Det här är poängen med formulärvarianterna: höjden mäts på widgeten i
/// stället för att skrivas som en siffra i koden, så ett formulär som får
/// en rad till växer av sig självt. Taket behövs för att "naturlig höjd"
/// annars är obegränsad — `Inställningar` blev längre än skärmen när
/// mätningen infördes utan det. Taket räknas ur den faktiska skärmens
/// arbetsyta i stället för att vara ett fast tal, så samma kod ger en
/// rimlig dialog på en liten laptop och på en 4K-skärm. `adw::Window`
/// lägger sitt eget innehåll i en skrollbar vy, så det som inte får plats
/// blir skrollbart — inte oåtkomligt.
fn content_height(content: &impl IsA<gtk::Widget>, width: i32) -> i32 {
    let (_, natural, _, _) = content.measure(gtk::Orientation::Vertical, width);

    let available = gtk::gdk::Display::default()
        .and_then(|display: gtk::gdk::Display| display.monitors().item(0))
        .and_downcast::<gtk::gdk::Monitor>()
        .map(|monitor| monitor.geometry().height())
        .unwrap_or(0);
    // Utan känd skärm (t.ex. udda headless-uppsättningar) faller vi
    // tillbaka på ett tak som får plats även på en liten laptopskärm.
    let cap = if available > 0 { available * 4 / 5 } else { 640 };

    natural.min(cap).max(1)
}

/// Bygger ett modalt dialogfönster. Titeln är ett obligatoriskt argument,
/// inte en valfri byggarmetod — det är det som gör "dialog utan titel"
/// omöjligt att råka ut för.
fn dialog_window(
    parent: &impl IsA<gtk::Window>,
    title: &str,
    size: DialogSize,
    content: &impl IsA<gtk::Widget>,
) -> adw::Window {
    let builder = adw::Window::builder()
        .transient_for(parent)
        .modal(true)
        .title(title)
        .default_width(size.width())
        .content(content);
    let height = size
        .height()
        .unwrap_or_else(|| content_height(content, size.width()));
    let window = builder.default_height(height).build();

    // Escape stänger dialogen. GTK4 gör INTE det av sig självt — det var
    // `GtkDialog` som hade beteendet, och den är avvecklad. Utan det här
    // gick ingen av appens tjugo dialoger att avbryta från tangentbordet;
    // man var tvungen att sikta på "Avbryt" med musen. Att det räcker med
    // ett ställe är hela poängen med att `dialog_window` blev enda vägen
    // in. `window.close` är GTK:s egen inbyggda åtgärd, så knappen och
    // tangenten gör bokstavligen samma sak.
    let escape = gtk::ShortcutController::new();
    escape.add_shortcut(gtk::Shortcut::new(
        gtk::ShortcutTrigger::parse_string("Escape"),
        Some(gtk::NamedAction::new("window.close")),
    ));
    window.add_controller(escape);

    window
}

/// Föräldrafönstret för en dialog som öppnas från sidopanelen.
fn app_window(app: &adw::Application) -> gtk::Window {
    app.active_window().expect("inget aktivt fönster")
}

/// Föräldrafönstret för en dialog som öppnas inifrån en session.
fn session_window(area: &Rc<SessionArea>) -> gtk::Window {
    area.overlay
        .root()
        .and_downcast::<gtk::Window>()
        .expect("inget fönster")
}

/// En rad i kommandopaletten.
struct PaletteEntry {
    /// Det som visas.
    label: String,
    /// Undertiteln — vad raden är och var den leder.
    detail: String,
    /// Det som söks i. Bredare än etiketten: en värd ska gå att hitta på
    /// sin IP eller sin tagg, inte bara på sitt alias.
    haystack: String,
    action: PaletteAction,
}

enum PaletteAction {
    /// Byt till en redan öppen flik.
    Select(adw::TabPage),
    /// Anslut till en sparad värd.
    Connect(Box<host::Host>),
    /// Kör en av appens egna åtgärder, `app.`-prefixet inkluderat.
    Command(&'static str),
}

/// Kortkommandot för en åtgärd, skrivet som GTK skriver det i menyerna
/// ("Ctrl+Skift+N"). Hämtas från appen i stället för att skrivas in för
/// hand, så att paletten inte kan lära ut ett kortkommando som ändrats.
fn accel_label(app: &adw::Application, action: &str) -> Option<String> {
    let accel = app.accels_for_action(action).into_iter().next()?;
    let (key, mods) = gtk::accelerator_parse(&accel)?;
    let label = gtk::accelerator_get_label(key, mods);
    (!label.is_empty()).then(|| label.to_string())
}

/// Öppna sessioner FÖRST, sedan sparade värdar, sedan appens åtgärder.
/// Ordningen spelar roll: vid lika poäng behåller `fuzzy::rank` inbördes
/// ordning, så en session man redan har uppe vinner över att öppna en
/// till mot samma värd — och det man vill nå oftast, en maskin, hamnar
/// aldrig under en åtgärd.
fn palette_entries(
    app: &adw::Application,
    area: &Rc<SessionArea>,
    store: &Rc<RefCell<HostStore>>,
) -> Vec<PaletteEntry> {
    let mut entries = Vec::new();

    let pages = area.tab_view.pages();
    for index in 0..pages.n_items() {
        let Some(page) = pages.item(index).and_downcast::<adw::TabPage>() else {
            continue;
        };
        let title = page.title().to_string();
        entries.push(PaletteEntry {
            haystack: title.clone(),
            label: title,
            detail: format!("Öppen session · flik {}", index + 1),
            action: PaletteAction::Select(page),
        });
    }

    let hosts: Vec<host::Host> = store.borrow().all().into_iter().cloned().collect();
    for host in hosts {
        let label = if host.alias.trim().is_empty() {
            host.host_name.clone()
        } else {
            host.alias.clone()
        };
        entries.push(PaletteEntry {
            haystack: format!(
                "{} {}@{} {}",
                host.alias,
                host.user,
                host.host_name,
                host.tags.join(" ")
            ),
            label,
            detail: format!("Anslut · {}@{}:{}", host.user, host.host_name, host.port),
            action: PaletteAction::Connect(Box::new(host)),
        });
    }

    let has_session = area.tab_view.n_pages() > 0;
    for command in palette_actions::available(has_session) {
        // Paletten är också det stället man LÄR SIG kortkommandona —
        // samma sak som gör att GTK skriver ut dem i menyerna.
        let detail = match accel_label(app, command.action) {
            Some(accel) => format!("Åtgärd · {accel}"),
            None => "Åtgärd".to_string(),
        };
        entries.push(PaletteEntry {
            haystack: palette_actions::haystack(command),
            label: command.label.to_string(),
            detail,
            action: PaletteAction::Command(command.action),
        });
    }

    entries
}

/// Ritar om listan efter sökningen och kommer ihåg vilka poster raderna
/// pekar på — radens index i listan är INTE samma sak som postens index i
/// `entries` så snart något filtrerats bort.
fn fill_palette_list(
    list: &gtk::ListBox,
    entries: &[PaletteEntry],
    query: &str,
    visible: &Rc<RefCell<Vec<usize>>>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let candidates: Vec<(&str, usize)> = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| (entry.haystack.as_str(), index))
        .collect();

    let ranked = fuzzy::rank(&candidates, query);
    let mut order = Vec::with_capacity(ranked.len());
    for (_, index) in ranked {
        let entry = &entries[index];
        let row = adw::ActionRow::builder()
            .title(&entry.label)
            .subtitle(&entry.detail)
            .activatable(true)
            .build();
        list.append(&row);
        order.push(index);
    }
    *visible.borrow_mut() = order;

    if let Some(first) = list.row_at_index(0) {
        list.select_row(Some(&first));
    }
}

/// Kommandopaletten: en sökruta över allt man rimligen vill nå snabbt —
/// de öppna sessionerna och de sparade värdarna. Termius har en
/// motsvarighet; det här är den delen av deras form som är värd att ta
/// efter (till skillnad från deras tangentval, se `set_accels_for_action`).
fn show_command_palette(
    app: &adw::Application,
    store: &Rc<RefCell<HostStore>>,
    area: &Rc<SessionArea>,
) {
    let search = gtk::SearchEntry::builder()
        .placeholder_text("Sök värd, tagg eller öppen session")
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(6)
        .build();

    let list = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::Single)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_bottom(12)
        .build();

    let scrolled = gtk::ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .build();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&search);
    content.append(&scrolled);

    // Ingen `adw::HeaderBar` här, till skillnad från appens övriga
    // dialoger: en palett ska vara sökrutan, inget annat. Fönstertiteln
    // sätts ändå av `dialog_window`, för fönsterhanterarens skull.
    let win = dialog_window(&app_window(app), "Kommandopalett", DialogSize::List, &content);

    let entries = Rc::new(palette_entries(app, area, store));
    let visible: Rc<RefCell<Vec<usize>>> = Rc::new(RefCell::new(Vec::new()));
    fill_palette_list(&list, &entries, "", &visible);

    search.connect_search_changed(clone!(
        #[weak]
        list,
        #[strong]
        entries,
        #[strong]
        visible,
        move |entry| {
            fill_palette_list(&list, &entries, &entry.text(), &visible);
        }
    ));

    let activate = clone!(
        #[strong]
        entries,
        #[strong]
        visible,
        #[strong]
        area,
        #[strong]
        store,
        #[weak]
        app,
        #[weak]
        win,
        move |row_index: i32| {
            let Some(entry_index) = visible.borrow().get(row_index.max(0) as usize).copied() else {
                return;
            };
            win.close();
            match &entries[entry_index].action {
                PaletteAction::Select(page) => {
                    area.tab_view.set_selected_page(page);
                }
                PaletteAction::Connect(host) => {
                    open_session(&area, &store, (**host).clone());
                }
                PaletteAction::Command(name) => {
                    // `activate_action` på appen vill ha namnet UTAN
                    // `app.`-prefixet — prefixet hör till widget- och
                    // menysidan av åtgärdssystemet, inte till gruppen
                    // åtgärden faktiskt bor i.
                    let bare = name.strip_prefix("app.").unwrap_or(name);
                    app.activate_action(bare, None);
                }
            }
        }
    );

    list.connect_row_activated(clone!(
        #[strong]
        activate,
        move |_, row| activate(row.index())
    ));

    // Enter i sökrutan tar den markerade raden — man ska aldrig behöva
    // flytta handen till musen eller ens till listan.
    search.connect_activate(clone!(
        #[weak]
        list,
        #[strong]
        activate,
        move |_| {
            let index = list.selected_row().map(|row| row.index()).unwrap_or(0);
            activate(index);
        }
    ));

    // Upp/ner flyttar markeringen UTAN att flytta tangentbordsfokus från
    // sökrutan — annars kan man inte fortsätta skriva efter att ha pilat.
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(clone!(
        #[weak]
        list,
        #[strong]
        visible,
        #[upgrade_or]
        glib::Propagation::Proceed,
        move |_, key, _, _| {
            let step = match key {
                gtk::gdk::Key::Down => 1,
                gtk::gdk::Key::Up => -1,
                _ => return glib::Propagation::Proceed,
            };
            let count = visible.borrow().len() as i32;
            if count == 0 {
                return glib::Propagation::Stop;
            }
            let current = list.selected_row().map(|row| row.index()).unwrap_or(0);
            let next = (current + step).clamp(0, count - 1);
            if let Some(row) = list.row_at_index(next) {
                list.select_row(Some(&row));
            }
            glib::Propagation::Stop
        }
    ));
    search.add_controller(keys);

    win.present();
    search.grab_focus();
}

/// Sidopanelens primärmeny. Sektionerna grupperar posterna efter vad de
/// gör — andra anslutningstyper, nätverk/lagring, och appens egna
/// inställningar — och ritas som avdelade block med skiljelinje.
fn sidebar_menu() -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();

    let palette = gtk::gio::Menu::new();
    palette.append(Some("Kommandopalett"), Some("app.palette"));
    menu.append_section(None, &palette);

    // Riktningen, inte avdelarens orientering — se kommentaren vid
    // `set_accels_for_action` om varför "vertikalt"/"horisontellt" är
    // sämre namn än de ser ut att vara.
    let panes = gtk::gio::Menu::new();
    panes.append(Some("Dela åt höger"), Some("app.split-right"));
    panes.append(Some("Dela nedåt"), Some("app.split-down"));
    panes.append(Some("Stäng ruta"), Some("app.close-pane"));
    menu.append_section(None, &panes);

    let marks = gtk::gio::Menu::new();
    marks.append(Some("Sätt bokmärke"), Some("app.bookmark-add"));
    marks.append(Some("Bokmärken i loggen"), Some("app.bookmark-list"));
    menu.append_section(None, &marks);

    let connections = gtk::gio::Menu::new();
    connections.append(Some("Telnet"), Some("app.telnet"));
    connections.append(Some("Seriell/USB"), Some("app.serial"));
    menu.append_section(None, &connections);

    // Valvets kategorier. Posterna öppnar inte längre var sitt fönster —
    // de byter vad sidopanelen visar (se `vault.rs`). Tailscale står kvar
    // bland dem för att den hör hemma i samma tanke ("var finns mina
    // maskiner"), även om den är en upptäcktsdialog och inte sparad data.
    let network = gtk::gio::Menu::new();
    network.append(Some("Tailscale"), Some("app.tailscale"));
    network.append(Some("WireGuard-profiler"), Some("app.wireguard"));
    network.append(Some("S3-anslutningar"), Some("app.s3"));
    network.append(Some("Kända värdar"), Some("app.known-hosts"));
    menu.append_section(None, &network);

    let app_menu = gtk::gio::Menu::new();
    app_menu.append(
        Some("Importera ssh-config"),
        Some("app.import_ssh_config"),
    );
    app_menu.append(Some("Funktioner"), Some("app.settings"));
    menu.append_section(None, &app_menu);

    menu
}

fn gio_menu_for(toggles: &settings::FeatureToggles, mac_address: &Option<String>) -> gtk::gio::Menu {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Redigera"), Some("host.edit"));
    // Ingen `FeatureToggles`-gate — precis som App/DashboardView.swift har
    // ingen egen "visa/dölj"-inställning i `AppSettings.swift` (till
    // skillnad från Docker/Snippets/SFTP/Tunnel/Nyckel, som alla är
    // valfria sidofunktioner), så systemöversikten är alltid tillgänglig.
    menu.append(Some("Systemöversikt"), Some("host.dashboard"));
    if mac_address.is_some() {
        menu.append(Some("Väck (Wake-on-LAN)"), Some("host.wake"));
    }
    if toggles.show_docker {
        menu.append(Some("Docker"), Some("host.docker"));
        menu.append(Some("Kubernetes"), Some("host.kubernetes"));
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

/// Importera värdar från en `~/.ssh/config` — klistra in innehållet
/// (ingen filväljare i v1, till skillnad från `App/ImportConfigView.swift`
/// som har en dokumentväljare — LinuxApp-användare kan lika gärna
/// `cat ~/.ssh/config`/`xclip` in det). Alias som redan finns i
/// värdlistan (skiftlägesokänsligt) hoppas tyst över, så ett omimport av
/// samma fil inte skapar dubbletter — se `HostStore::import_ssh_config`.
fn show_ssh_config_import_dialog(
    app: &adw::Application,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
    area: &Rc<SessionArea>,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    search_query: &Rc<RefCell<String>>,
) {
    let text_view = gtk::TextView::builder().monospace(true).build();
    let text_scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .min_content_height(280)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .vexpand(true)
        .build();
    let status_label = gtk::Label::builder()
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .margin_bottom(8)
        .halign(gtk::Align::Start)
        .wrap(true)
        .build();

    let import_button = gtk::Button::with_label("Importera");
    import_button.add_css_class("suggested-action");
    let cancel_button = gtk::Button::with_label("Avbryt");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_start(&cancel_button);
    header.pack_end(&import_button);
    header.set_title_widget(Some(&adw::WindowTitle::new(
        "Importera ssh-config",
        "Klistra in innehållet från din ~/.ssh/config",
    )));

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&text_scrolled);
    content.append(&status_label);

    let win = dialog_window(
        &app_window(app),
        "Importera ssh-config",
        DialogSize::List,
        &content,
    );

    cancel_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));
    import_button.connect_clicked(clone!(
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
        search_query,
        #[strong]
        status_label,
        #[weak]
        win,
        move |_| {
            let buffer = text_view.buffer();
            let text = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string();
            if text.trim().is_empty() {
                return;
            }
            match store.borrow_mut().import_ssh_config(&text) {
                Ok(n) => {
                    refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
                    if n > 0 {
                        win.close();
                    } else {
                        status_label.set_label("Inga nya värdar hittades (redan importerade, eller inget alias hade en användare angiven).");
                    }
                }
                Err(e) => status_label.set_label(&format!("Fel: {e}")),
            }
        }
    ));

    win.present();
}

/// Lägg till/redigera-dialogen. `existing = None` skapar en ny värd.
/// Åtta parametrar — alla delat, per-fönster GTK-tillstånd (store/list/area/
/// inställningar/sökfilter) som funktionen genuint behöver för att kunna
/// spara och trigga en `refresh_list`, inte en signatur som går att slå
/// ihop utan att införa en egen struct bara för det.
#[allow(clippy::too_many_arguments)]
fn show_host_dialog(
    app: &adw::Application,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
    area: &Rc<SessionArea>,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    search_query: &Rc<RefCell<String>>,
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

    // Jump-host (ProxyJump). Motorn har kunnat det här sedan `ssh::connect_via_jump`,
    // och `Host::jump_host_id` har funnits i datamodellen hela tiden — men inget
    // i Linux-GUI:t kunde SÄTTA fältet, så funktionen gick bara att få via synk
    // från App/ eller genom att handredigera hosts.json.
    //
    // Urvalet kommer från `HostStore::jump_host_candidates`, som delar regler med
    // `resolve_jump`: aldrig sig själv, och aldrig en värd som själv har en jump
    // (bara ett hopp stöds). `jump_ids[i]` hör ihop med rad `i` i modellen.
    let jump_row = adw::ComboRow::builder()
        .title("Anslut via (jump-host)")
        .subtitle("ProxyJump — ett hopp")
        .build();
    let mut jump_ids: Vec<Option<uuid::Uuid>> = vec![None];
    let mut jump_labels: Vec<String> = vec!["Ingen".to_string()];
    {
        let store_ref = store.borrow();
        for candidate in store_ref.jump_host_candidates(existing.as_ref().map(|h| h.id)) {
            jump_labels.push(format!("{} ({}@{})", candidate.alias, candidate.user, candidate.host_name));
            jump_ids.push(Some(candidate.id));
        }
        // En redan sparad jump-host som INTE är en giltig kandidat (borttagen,
        // eller har hunnit få en egen jump via synk) får inte tyst nollställas
        // bara för att dialogen öppnas. Den visas i stället som ett eget,
        // förvalt alternativ med orsaken utskriven — användaren väljer själv
        // om den ska bytas.
        if let Some(saved) = existing.as_ref().and_then(|h| h.jump_host_id)
            && !jump_ids.contains(&Some(saved))
        {
            let label = match store_ref.all().into_iter().find(|h| h.id == saved) {
                Some(h) => format!("{} — ogiltig: har själv en jump-host", h.alias),
                None => "Okänd värd — borttagen eller inte synkad än".to_string(),
            };
            jump_labels.push(label);
            jump_ids.push(Some(saved));
        }
    }
    let jump_label_refs: Vec<&str> = jump_labels.iter().map(|s| s.as_str()).collect();
    jump_row.set_model(Some(&gtk::StringList::new(&jump_label_refs)));
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

    // Favorit + taggar: precis som `color_tag`, fält som funnits i
    // datamodellen sedan starten men saknade UI-koppling i den här
    // dialogen. Motsvarar `Toggle("Favorit", ...)` +
    // `tagsText`-fältet i `App/HostEditView.swift`.
    let favorite_row = adw::SwitchRow::builder().title("Favorit").build();
    let tags_row = adw::EntryRow::builder().title("Taggar (kommaseparerat)").build();

    // Färgmärkning: `host.color_tag` fanns i datamodellen sedan starten men
    // saknade helt en väljare i den här dialogen (och en visuell markering
    // i listan, se `host_color_dot`) — motsvarar `HostColorPicker` i
    // `App/HostColor.swift`. Manuell exklusiv-val-logik (inte
    // `ToggleButton::set_group`) eftersom Swift-sidans beteende — trycka på
    // den REDAN valda färgen igen tar bort valet helt — inte är vanlig
    // radioknapp-semantik.
    let color_selection: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let color_row = adw::ActionRow::builder().title("Färgmärkning").build();
    let color_box = gtk::Box::builder().orientation(gtk::Orientation::Horizontal).spacing(8).valign(gtk::Align::Center).build();
    let mut color_buttons: Vec<(String, gtk::ToggleButton)> = Vec::new();
    for (name, _hex) in HOST_COLORS {
        let btn = gtk::ToggleButton::builder().width_request(24).height_request(24).build();
        btn.add_css_class(&format!("host-color-{name}"));
        btn.add_css_class("circular");
        color_box.append(&btn);
        color_buttons.push(((*name).to_string(), btn));
    }
    color_row.add_suffix(&color_box);
    for (name, btn) in &color_buttons {
        btn.connect_toggled(clone!(
            #[strong]
            color_selection,
            #[strong(rename_to = my_name)]
            name,
            #[strong(rename_to = all_buttons)]
            color_buttons,
            move |btn| {
                if btn.is_active() {
                    *color_selection.borrow_mut() = Some(my_name.clone());
                    for (other_name, other_btn) in &all_buttons {
                        if other_name != &my_name {
                            other_btn.set_active(false);
                        }
                    }
                } else if color_selection.borrow().as_deref() == Some(my_name.as_str()) {
                    *color_selection.borrow_mut() = None;
                }
            }
        ));
    }

    // Auth-metod: fem av `HostAuth`s sex varianter går att välja här.
    // `KeychainKey` är genuint Apple Keychain-specifik och saknas — men
    // `BitwardenItem` hör HEMMA här: Linux är faktiskt den ENDA plattformen
    // där `bw`-integrationen fungerar (Apple-sidans `resolveAuth`
    // returnerar alltid `nil` för den, se `bitwarden.rs`s modulkommentar),
    // så den hör inte ihop med `KeychainKey` trots att de historiskt
    // grupperats ihop som "saknar Linux-stöd". Motsvarar
    // App/HostEditView.swifts auth-väljare.
    let auth_row = adw::ComboRow::builder().title("Autentisering").build();
    let auth_model = gtk::StringList::new(&[
        "SSH-agent (standard)",
        "Lösenord vid anslutning",
        "Nyckelfil",
        "OpenSSH-certifikat",
        "Bitwarden",
    ]);
    auth_row.set_model(Some(&auth_model));
    let key_file_row = adw::EntryRow::builder().title("Sökväg till privat nyckel").build();
    let key_file_browse = gtk::Button::from_icon_name("document-open-symbolic");
    key_file_browse.add_css_class("flat");
    key_file_browse.set_valign(gtk::Align::Center);
    key_file_row.add_suffix(&key_file_browse);
    let cert_file_row = adw::EntryRow::builder()
        .title("Sökväg till certifikatfil (…-cert.pub)")
        .build();
    let cert_file_browse = gtk::Button::from_icon_name("document-open-symbolic");
    cert_file_browse.add_css_class("flat");
    cert_file_browse.set_valign(gtk::Align::Center);
    cert_file_row.add_suffix(&cert_file_browse);
    let bitwarden_row = adw::EntryRow::builder().title("Bitwarden item-id eller namn").build();
    let auth_error_label = gtk::Label::builder()
        .label("Ange sökväg(ar)/item-id för den valda auth-metoden")
        .margin_start(12)
        .margin_end(12)
        .halign(gtk::Align::Start)
        .css_classes(["error", "caption"])
        .visible(false)
        .build();

    let update_auth_rows_visibility = clone!(
        #[weak]
        key_file_row,
        #[weak]
        cert_file_row,
        #[weak]
        bitwarden_row,
        move |selected: u32| {
            key_file_row.set_visible(selected == 2 || selected == 3);
            cert_file_row.set_visible(selected == 3);
            bitwarden_row.set_visible(selected == 4);
        }
    );
    update_auth_rows_visibility(0);
    auth_row.connect_selected_notify(clone!(
        #[strong]
        update_auth_rows_visibility,
        move |row| update_auth_rows_visibility(row.selected())
    ));

    // Bevaras uttryckligen om värden synkats in med Keychain-auth från en
    // Apple-enhet — Linux har ingen motsvarighet, så dialogen ska varken
    // kunna VÄLJA den eller av misstag SKRIVA ÖVER den bara för att
    // användaren redigerade alias/värdnamn (samma opt-in-princip som
    // nyckeldistributionens lösenordsborttagning, se `key_deploy.rs`).
    let preserve_apple_only_auth = existing.as_ref().is_some_and(|h| matches!(h.auth, host::HostAuth::KeychainKey(_)));

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
        if let Some(saved_jump) = h.jump_host_id
            && let Some(idx) = jump_ids.iter().position(|id| *id == Some(saved_jump))
        {
            jump_row.set_selected(idx as u32);
        }
        favorite_row.set_active(h.is_favorite);
        if !h.tags.is_empty() {
            tags_row.set_text(&h.tags.join(", "));
        }
        if let Some(color) = &h.color_tag {
            *color_selection.borrow_mut() = Some(color.clone());
            if let Some((_, btn)) = color_buttons.iter().find(|(name, _)| name == color) {
                btn.set_active(true);
            }
        }
        match &h.auth {
            host::HostAuth::AgentDefault => auth_row.set_selected(0),
            host::HostAuth::AskPassword => auth_row.set_selected(1),
            host::HostAuth::KeyFile(path) => {
                auth_row.set_selected(2);
                key_file_row.set_text(path);
            }
            host::HostAuth::CertificateFile { key_path, cert_path } => {
                auth_row.set_selected(3);
                key_file_row.set_text(key_path);
                cert_file_row.set_text(cert_path);
            }
            host::HostAuth::BitwardenItem(item_id) => {
                auth_row.set_selected(4);
                bitwarden_row.set_text(item_id);
            }
            // Apple-bara — visas som agent-standard men rörs inte vid
            // spara, se `preserve_apple_only_auth` ovan.
            host::HostAuth::KeychainKey(_) => {
                auth_row.set_selected(0);
            }
        }
        update_auth_rows_visibility(auth_row.selected());
    }

    key_file_browse.connect_clicked(clone!(
        #[weak]
        app,
        #[weak]
        key_file_row,
        move |_| {
            let dialog = gtk::FileDialog::builder().title("Välj privat nyckel").build();
            let parent_window = app.active_window();
            glib::spawn_future_local(clone!(
                #[weak]
                key_file_row,
                async move {
                    if let Ok(file) = dialog.open_future(parent_window.as_ref()).await
                        && let Some(path) = file.path()
                    {
                        key_file_row.set_text(&path.to_string_lossy());
                    }
                }
            ));
        }
    ));
    cert_file_browse.connect_clicked(clone!(
        #[weak]
        app,
        #[weak]
        cert_file_row,
        move |_| {
            let dialog = gtk::FileDialog::builder().title("Välj certifikatfil").build();
            let parent_window = app.active_window();
            glib::spawn_future_local(clone!(
                #[weak]
                cert_file_row,
                async move {
                    if let Ok(file) = dialog.open_future(parent_window.as_ref()).await
                        && let Some(path) = file.path()
                    {
                        cert_file_row.set_text(&path.to_string_lossy());
                    }
                }
            ));
        }
    ));

    let group = adw::PreferencesGroup::new();
    group.add(&alias_row);
    group.add(&host_row);
    group.add(&user_row);
    group.add(&port_row);
    group.add(&jump_row);
    group.add(&platform_row);
    group.add(&mac_row);
    group.add(&favorite_row);
    group.add(&tags_row);
    group.add(&color_row);
    group.add(&auth_row);
    group.add(&key_file_row);
    group.add(&cert_file_row);
    group.add(&bitwarden_row);

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
    content.append(&auth_error_label);

    let win = dialog_window(
        &app_window(app),
        if is_edit { "Redigera värd" } else { "Lägg till värd" },
        DialogSize::Form,
        &content,
    );

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
        #[strong]
        auth_row,
        #[strong]
        key_file_row,
        #[strong]
        cert_file_row,
        #[strong]
        bitwarden_row,
        #[strong]
        auth_error_label,
        #[strong]
        color_selection,
        #[strong]
        favorite_row,
        #[strong]
        tags_row,
        #[strong]
        search_query,
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
            // Sökvägarna trimmas men rörs INTE i övrigt — de kan peka på
            // vilken plats som helst, inte bara `~/.ssh` (samma
            // resonemang som `ssh_config.rs`s `IdentityFile`-hantering).
            let key_file = key_file_row.text().trim().to_string();
            let cert_file = cert_file_row.text().trim().to_string();
            let bitwarden_item = bitwarden_row.text().trim().to_string();
            let new_auth = match auth_row.selected() {
                1 => Ok(host::HostAuth::AskPassword),
                2 if !key_file.is_empty() => Ok(host::HostAuth::KeyFile(key_file)),
                3 if !key_file.is_empty() && !cert_file.is_empty() => {
                    Ok(host::HostAuth::CertificateFile {
                        key_path: key_file,
                        cert_path: cert_file,
                    })
                }
                4 if !bitwarden_item.is_empty() => Ok(host::HostAuth::BitwardenItem(bitwarden_item)),
                2..=4 => Err(()),
                _ => Ok(host::HostAuth::AgentDefault),
            };
            let new_auth = match new_auth {
                Ok(a) => a,
                Err(()) => {
                    auth_error_label.set_visible(true);
                    return;
                }
            };
            auth_error_label.set_visible(false);
            let color_tag = color_selection.borrow().clone();
            // `jump_ids` byggdes parallellt med modellen ovan, så index är
            // alltid giltigt — men `get` i stället för indexering, så en
            // framtida ändring av modellen inte kan panika här.
            let jump_host_id = jump_ids.get(jump_row.selected() as usize).copied().flatten();
            let is_favorite = favorite_row.is_active();
            // Samma tolkning som Swift-sidans `save()`: dela på komma,
            // trimma, kasta bort tomma segment (t.ex. ett kvarglömt
            // avslutande komma).
            let tags: Vec<String> = tags_row
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let host = if let Some(mut h) = existing.clone() {
                h.alias = alias;
                h.host_name = host_name;
                h.user = user;
                h.port = port;
                h.platform = platform;
                h.mac_address = mac_address;
                h.color_tag = color_tag;
                h.is_favorite = is_favorite;
                h.tags = tags;
                h.jump_host_id = jump_host_id;
                if !preserve_apple_only_auth {
                    h.auth = new_auth;
                }
                h
            } else {
                let mut h = Host::new(alias, host_name, user);
                h.port = port;
                h.platform = platform;
                h.mac_address = mac_address;
                h.color_tag = color_tag;
                h.is_favorite = is_favorite;
                h.tags = tags;
                h.jump_host_id = jump_host_id;
                h.auth = new_auth;
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
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
            win.close();
        }
    ));

    win.present();
}

/// Kör HELA den interaktiva OAuth-inloggningen: startar loopback-
/// lyssnaren, öppnar systemets webbläsare mot auktoriseringsURL:en
/// (`gtk::UriLauncher`, portal-baserad — fungerar även paketerat/
/// sandboxat, till skillnad från att skala ut till `xdg-open`), väntar
/// in redirecten och byter koden mot ett token. Se `oauth.rs`s
/// `LoginSession`-dokumentation för varför resten av flödet ligger där
/// i stället för här (testbart utan GTK).
async fn run_oauth_login(app: &adw::Application, provider: &oauth::OAuthProviderConfig) -> Result<oauth::StoredOAuthToken, String> {
    let session = oauth::start_login(provider).await?;
    let parent_window = app.active_window();
    let launcher = gtk::UriLauncher::new(session.authorize_url.as_str());
    launcher.launch_future(parent_window.as_ref()).await.map_err(|e| format!("kunde inte öppna webbläsaren: {e}"))?;
    let client = reqwest::Client::new();
    oauth::finish_login(session, &client, provider).await
}

/// Funktioner-inställningar: alla sex av `settings::FeatureToggles`s fält
/// utom Snippets (`show_snippets` — sidopanelens Snippets-vy finns inte i
/// LinuxApp än, se ROADMAP.md; fältet läses/skrivs ändå för att inte tappa
/// en delad `settings.json`-fils övriga värden) har ett eget reglage här.
/// Motsvarar `App/FeatureSettingsView.swift`. Åtta parametrar av samma
/// skäl som `show_host_dialog` ovan.
#[allow(clippy::too_many_arguments)]
fn show_settings_dialog(
    app: &adw::Application,
    settings_store: &Rc<RefCell<settings::AppSettingsStore>>,
    store: &Rc<RefCell<HostStore>>,
    list: &gtk::ListBox,
    area: &Rc<SessionArea>,
    snippet_store: &Rc<RefCell<snippet::SnippetStore>>,
    sync_config: &Rc<RefCell<sync::SyncConfig>>,
    search_query: &Rc<RefCell<String>>,
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
    // De två sista av `FeatureToggles`s sex fält saknade reglage här — lästes
    // redan (`gio_menu_for` döljer "Tunnel"/"Nyckel"-menyposterna korrekt)
    // men gick bara att stänga av genom att redigera `settings.json` för
    // hand. Motsvarar `App/FeatureSettingsView.swift`s "Värdmenyn"-sektion.
    let forward_row = adw::SwitchRow::builder()
        .title("Portvidarebefordran")
        .subtitle("Visa Tunnel-knappen på värdar")
        .active(current.show_port_forward)
        .build();
    let key_deploy_row = adw::SwitchRow::builder()
        .title("SSH-nyckel")
        .subtitle("Visa Nyckel-knappen på värdar")
        .active(current.show_key_deploy)
        .build();

    let group = adw::PreferencesGroup::builder().title("Funktioner").build();
    group.add(&docker_row);
    group.add(&commands_row);
    group.add(&sftp_row);
    group.add(&forward_row);
    group.add(&key_deploy_row);

    // Ren lokal preferens (INTE synkad, se `terminal_theme.rs`s
    // modulkommentar) — motsvarar App/TerminalThemeSettingsView.swift.
    let theme_store = terminal_theme::TerminalThemeStore::open(terminal_theme::TerminalThemeStore::default_path());
    let themes = terminal_theme::all();
    let theme_names: Vec<&str> = themes.iter().map(|t| t.name).collect();
    let theme_row = adw::ComboRow::builder().title("Terminalfärgtema").build();
    let theme_model = gtk::StringList::new(&theme_names);
    theme_row.set_model(Some(&theme_model));
    let current_theme_id = theme_store.selected_id();
    let current_theme = terminal_theme::theme(current_theme_id.as_deref());
    if let Some(pos) = themes.iter().position(|t| t.id == current_theme.id) {
        theme_row.set_selected(pos as u32);
    }
    let terminal_group = adw::PreferencesGroup::builder().title("Terminal").build();
    terminal_group.add(&theme_row);

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

    // Kontosynk (OAuth2/PKCE): motsvarar App/SyncSettingsView.swifts
    // `accountRow` per leverantör — bara inloggnings-/utloggningsläget här,
    // INTE en fullständig egen sync-transport (att faktiskt ladda upp/ner
    // den krypterade synkfilen via Dropbox/Drive/OneDrive-API:et är ett
    // eget, större steg, se ROADMAP.md "Kvar"). Login-flödet är en lokal
    // loopback-HTTP-lyssnare (RFC 8252, `oauth::start_login`/
    // `finish_login`) — `gtk::UriLauncher` öppnar systemets webbläsare via
    // portalen, fungerar även paketerat/sandboxat.
    let account_group = adw::PreferencesGroup::builder()
        .title("Kontosynk")
        .description("Logga in för att koppla ett molnkonto — själva synken via kontot är inte byggd än, bara inloggningen.")
        .build();
    let oauth_token_store = Rc::new(oauth::OAuthTokenStore::open(oauth::OAuthTokenStore::default_path()));
    for provider in oauth::all_providers() {
        let row = adw::ActionRow::builder().title(provider.display_name).build();
        if !provider.is_configured() {
            row.set_subtitle("Inte konfigurerad än — se README \"Kontointegration\"");
        } else {
            let logged_in = oauth_token_store.is_logged_in(&provider);
            row.set_subtitle(if logged_in { "Inloggad" } else { "Inte inloggad" });
            let button = gtk::Button::with_label(if logged_in { "Logga ut" } else { "Logga in" });
            button.set_valign(gtk::Align::Center);
            button.connect_clicked(clone!(
                #[weak]
                app,
                #[strong]
                oauth_token_store,
                #[weak]
                row,
                #[strong(rename_to = provider)]
                provider,
                move |btn| {
                    if oauth_token_store.is_logged_in(&provider) {
                        if let Err(e) = oauth_token_store.logout(&provider) {
                            eprintln!("kunde inte logga ut: {e}");
                            return;
                        }
                        row.set_subtitle("Inte inloggad");
                        btn.set_label("Logga in");
                        return;
                    }
                    btn.set_sensitive(false);
                    glib::spawn_future_local(clone!(
                        #[weak]
                        app,
                        #[strong]
                        oauth_token_store,
                        #[weak]
                        row,
                        #[weak]
                        btn,
                        #[strong(rename_to = provider)]
                        provider,
                        async move {
                            match run_oauth_login(&app, &provider).await {
                                Ok(token) => {
                                    if let Err(e) = oauth_token_store.save(&provider, &token) {
                                        row.set_subtitle(&format!("Kunde inte spara token: {e}"));
                                    } else {
                                        row.set_subtitle("Inloggad");
                                        btn.set_label("Logga ut");
                                    }
                                }
                                Err(e) => row.set_subtitle(&format!("Inloggning misslyckades: {e}")),
                            }
                            btn.set_sensitive(true);
                        }
                    ));
                }
            ));
            row.add_suffix(&button);
        }
        account_group.add(&row);
    }

    let page = adw::PreferencesPage::new();
    page.add(&group);
    page.add(&terminal_group);
    page.add(&sync_group);
    page.add(&account_group);

    let close_button = gtk::Button::with_label("Klar");
    close_button.add_css_class("suggested-action");
    let header = adw::HeaderBar::builder()
        .show_end_title_buttons(false)
        .build();
    header.pack_end(&close_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&page);

    let win = dialog_window(&app_window(app), "Inställningar", DialogSize::Form, &content);

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
        #[strong]
        search_query,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_docker = row.is_active();
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
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
        #[strong]
        search_query,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_command_library = row.is_active();
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
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
        #[strong]
        search_query,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_sftp_browser = row.is_active();
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
        }
    ));

    forward_row.connect_active_notify(clone!(
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
        #[strong]
        search_query,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_port_forward = row.is_active();
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
        }
    ));

    key_deploy_row.connect_active_notify(clone!(
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
        #[strong]
        search_query,
        move |row| {
            let mut toggles = settings_store.borrow().current();
            toggles.show_key_deploy = row.is_active();
            if let Err(e) = settings_store.borrow_mut().update(toggles) {
                eprintln!("kunde inte spara inställningarna: {e}");
            }
            refresh_list(&list, &store, &app, &area, &settings_store, &snippet_store, &search_query);
        }
    ));

    theme_row.connect_selected_notify(clone!(
        #[strong]
        area,
        move |row| {
            let themes = terminal_theme::all();
            let Some(t) = themes.get(row.selected() as usize) else { return };
            if let Err(e) = theme_store.set_selected_id(t.id) {
                eprintln!("kunde inte spara terminaltemat: {e}");
                return;
            }
            // Bara REDAN ÖPPNA flikar behöver uppdateras här — nästa
            // session skapas alltid med `new_themed_terminal()`, som läser
            // det nyss sparade valet direkt. `AdwTabView` har ingen
            // `nth_page`; `pages()` (ett `gio::ListModel`) är den
            // dokumenterade vägen att räkna upp alla flikar.
            let theme = terminal_theme::theme(Some(t.id));
            let pages = area.tab_view.pages();
            for i in 0..pages.n_items() {
                let Some(page) = pages.item(i).and_downcast::<adw::TabPage>() else { continue };
                // Varje ruta i fliken — en delad flik har flera terminaler
                // och alla ska byta tema, inte bara den första.
                for terminal in split::terminals_in(&page.child()) {
                    terminal_theme::apply(&terminal, theme);
                }
            }
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
        #[strong]
        search_query,
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
                #[strong]
                search_query,
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
                                        &search_query,
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
    /// Kortlivad återkoppling ovanpå sessionerna. En modal dialog vore
    /// fel verktyg för "bokmärke satt": den avbryter precis den körning
    /// man tittade på när man tryckte.
    toasts: adw::ToastOverlay,
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

        // Toast-lagret ligger MELLAN överlägget och flikvyn: `overlay`
        // förblir det dialoger presenteras på och det platshållaren
        // ritas över, medan toasts hamnar ovanpå sessionsinnehållet.
        let toasts = adw::ToastOverlay::new();
        toasts.set_child(Some(&tab_view));

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&toasts));
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
            toasts,
            tab_view,
            tab_bar,
            placeholder,
        });
        area.update_placeholder();

        area.tab_view.connect_close_page(clone!(
            #[strong]
            area,
            move |_, page| {
                // VARJE ruta i fliken, inte bara `page.child()`: med delad
                // vy (`split.rs`) är sidans barn en behållare med ett träd
                // av terminaler i, och en flik som stängs ska stänga alla
                // sina anslutningar — inte bara den första.
                for terminal in split::terminals_in(&page.child()) {
                    close_session_channel(&terminal);
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

/// Förklarar att RSA är avstängt och länkar vidare. Egen funktion i stället
/// för `show_message_dialog` eftersom den här behöver markup — poängen är
/// just att länken går att klicka på, inte att läsa upp en URL ur en
/// terminalrad. `body-use-markup` gör `<a href>` klickbar i AdwAlertDialog.
fn show_rsa_disabled_dialog(area: &Rc<SessionArea>) {
    let dialog = adw::AlertDialog::new(Some("RSA-nycklar är inaktiverade"), None);
    dialog.set_body_use_markup(true);
    dialog.set_body(&format!(
        "Anslutningen avbröts eftersom värden är konfigurerad med en \
         RSA-nyckel.\n\n\
         RSA är tillfälligt avstängt i Linux-appen på grund av \
         RUSTSEC-2023-0071 (Marvin-attacken), en sårbarhet i crate:n \
         <tt>rsa</tt> som saknar rättad version. Stödet slås på igen så \
         snart den finns.\n\n\
         Använd en Ed25519-nyckel under tiden.\n\n\
         <a href=\"{url}\">Läs mer om varför</a>",
        url = ssh::RSA_DISABLED_DOC_URL
    ));
    dialog.add_response("ok", "OK");
    dialog.set_default_response(Some("ok"));
    dialog.present(Some(&area.overlay));
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
                    start_session(area, host, Some(password), jump.clone(), SessionTarget::NewTab)
                });
            } else {
                start_session(&area, host, None, jump, SessionTarget::NewTab);
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

    let win = dialog_window(&session_window(area), "Lösenord", DialogSize::Compact, &content);

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

/// Bygger en ny terminalwidget med det sparade terminalfärgtemat applicerat
/// (`terminal_theme::apply`) — anropas från alla tre sessionsstartare
/// (SSH/Telnet/Seriell) i stället för att var och en läser
/// `TerminalThemeStore` och sätter färger separat.
fn new_themed_terminal() -> vte::Terminal {
    let terminal = vte::Terminal::builder()
        .vexpand(true)
        .hexpand(true)
        // VTE:s förval är 512 rader. Det räcker inte för det appen är
        // till för — en `apt upgrade` eller ett bygge skrollar förbi det
        // långt innan man hunnit läsa, och log-bokmärken pekar in i
        // just den här bufferten (se `bookmarks`), så taket är också
        // gränsen för hur långt tillbaka ett bokmärke kan peka.
        .scrollback_lines(SCROLLBACK_LINES)
        .build();
    let store = terminal_theme::TerminalThemeStore::open(terminal_theme::TerminalThemeStore::default_path());
    terminal_theme::apply(&terminal, terminal_theme::theme(store.selected_id().as_deref()));
    terminal
}

/// Flikar mot SAMMA värd (öppna en till session medan en redan är igång)
/// ser annars identiska ut i flikraden — numrerar dem "(2)", "(3)" osv.,
/// samma UX-regel som `App/MultiSessionView.swifts` `displayLabel`. Räknar
/// befintliga flikar med titeln `alias` eller `alias (N)` via
/// `AdwTabView::pages()` (inget `nth_page` i libadwaita-API:t, se
/// `terminal_theme`-portens motsvarande kommentar).
/// Hämtar de öppna flikarnas namn och lämnar över till `tab_title`, som
/// är GTK-fri och därför testbar.
fn unique_session_tab_title(area: &Rc<SessionArea>, base: &str) -> String {
    let pages = area.tab_view.pages();
    let existing: Vec<String> = (0..pages.n_items())
        .filter_map(|i| pages.item(i).and_downcast::<adw::TabPage>())
        .map(|page| page.title().to_string())
        .collect();
    tab_title::unique_title(base, &existing)
}

/// Hör sidan fortfarande till flikvyn?
///
/// `AdwTabView::page_position` DUGER INTE som den frågan: för en sida som
/// inte hör till vyn loggar den en Adwaita-CRITICAL i stället för att
/// svara. Sex ställen i appen ställde ändå frågan så, och alla sex
/// skrev ut en CRITICAL varje gång en flik stängdes innan dess
/// anslutning hann ge upp. Reproducerat med enbart mus: öppna en session
/// mot en oanträffbar adress, stäng fliken, vänta tills försöket ger upp.
fn tab_view_contains(area: &Rc<SessionArea>, page: &adw::TabPage) -> bool {
    let pages = area.tab_view.pages();
    (0..pages.n_items())
        .filter_map(|i| pages.item(i).and_downcast::<adw::TabPage>())
        .any(|open| &open == page)
}

/// Var en ny terminalsession ska hamna.
#[derive(Clone)]
enum SessionTarget {
    /// En egen flik, som allt gjorde innan delad vy fanns.
    NewTab,
    /// Delad vy: `pane` (en redan öppen ruta) delas och sessionen hamnar i
    /// den nya halvan av SAMMA flik. Fliken byter inte titel — den
    /// beskriver fortfarande sessionen den öppnades för.
    Split {
        pane: vte::Terminal,
        orientation: gtk::Orientation,
    },
}

/// Vad en ruta är ansluten till, sparat på terminalwidgeten så att "dela
/// vyn" kan öppna EN TILL session mot samma sak utan att fråga om det som
/// redan är känt.
///
/// Lösenordet sparas medvetet INTE här. En `AskPassword`-värd frågar igen
/// vid delning, precis som den gjorde när sessionen öppnades — ett
/// lösenord som ligger kvar på en widget är en hemlighet som lever längre
/// än den behöver.
///
/// Seriella sessioner har med flit ingen variant: en fysisk port kan bara
/// öppnas av en process i taget, så "en till mot samma sak" finns inte.
/// Delningsåtgärderna gör därför ingenting i en seriell ruta.
#[derive(Clone)]
enum PaneSource {
    Ssh {
        host: Box<host::Host>,
        jump: Option<Box<host::Host>>,
    },
    Telnet {
        host: String,
        port: u16,
    },
}

/// Hur många rader terminalens skrollbuffert håller. Se
/// `new_themed_terminal` och `bookmarks`-modulens kommentar om glidning.
const SCROLLBACK_LINES: u32 = 100_000;

const PANE_SOURCE_KEY: &str = "bastion-pane-source";

const BOOKMARKS_KEY: &str = "bastion-bookmarks";

/// Rutans bokmärkeslista, skapad vid första anropet.
///
/// Listan hänger på widgeten och inte i en central tabell av samma skäl
/// som `PaneSource`: en ruta som stängs tar med sig sina bokmärken, utan
/// att något måste komma ihåg att städa. Se `bookmarks`-modulen för
/// varför de inte sparas till disk.
fn pane_bookmarks(terminal: &vte::Terminal) -> Rc<RefCell<bookmarks::BookmarkList>> {
    unsafe {
        if let Some(ptr) = terminal.data::<Rc<RefCell<bookmarks::BookmarkList>>>(BOOKMARKS_KEY) {
            return ptr.as_ref().clone();
        }
        let list = Rc::new(RefCell::new(bookmarks::BookmarkList::new()));
        terminal.set_data(BOOKMARKS_KEY, list.clone());
        list
    }
}

fn set_pane_source(terminal: &vte::Terminal, source: PaneSource) {
    unsafe {
        terminal.set_data(PANE_SOURCE_KEY, source);
    }
}

fn pane_source(terminal: &vte::Terminal) -> Option<PaneSource> {
    unsafe {
        terminal
            .data::<PaneSource>(PANE_SOURCE_KEY)
            .map(|ptr| ptr.as_ref().clone())
    }
}

/// Fliken som `widget` sitter i, om någon.
fn page_containing(area: &Rc<SessionArea>, widget: &impl IsA<gtk::Widget>) -> Option<adw::TabPage> {
    let widget = widget.as_ref();
    let pages = area.tab_view.pages();
    (0..pages.n_items())
        .filter_map(|i| pages.item(i).and_downcast::<adw::TabPage>())
        .find(|page| {
            let child = page.child();
            child == *widget || widget.is_ancestor(&child)
        })
}

/// Sätter in en nybyggd terminal där `target` säger och ger tillbaka
/// fliken den hamnade i. Delad av alla tre sessionsstartare (SSH/Telnet/
/// Seriell) så att ETT ställe känner till både flik- och ruttfallet.
///
/// `title` gäller bara `NewTab` — en delad ruta ärver flikens titel.
fn place_terminal(
    area: &Rc<SessionArea>,
    terminal: &vte::Terminal,
    target: SessionTarget,
    title: &str,
) -> adw::TabPage {
    if let SessionTarget::Split { pane, orientation } = target {
        // Rutan kan ha hunnit stängas mellan att åtgärden aktiverades och
        // att lösenordsdialogen besvarades. Sessionen är redan startad då
        // — en ny flik är rätt svar, inte att tappa bort den.
        if let Some(page) = page_containing(area, &pane) {
            split::split(&pane, orientation, terminal);
            terminal.grab_focus();
            return page;
        }
    }

    let root = split::pane_root();
    root.append(terminal);
    let page = area.tab_view.append(&root);
    page.set_title(title);
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();
    terminal.grab_focus();
    page
}

/// Stryper anslutningen bakom en terminal — SSH, Telnet och Seriell delar
/// mekanism (`async_channel` in, händelser ut) och därför den här vägen ut.
///
/// Kanalen STÄNGS, den droppas inte. Skillnaden är hela poängen:
/// `steal_data` tog bara bort EN klon av sändaren, och den räckte inte,
/// för det finns alltid en till — `terminal.connect_commit` håller en egen
/// så länge widgeten lever. Och widgeten levde: `glib::spawn_future_local`
/// får sin terminal genom `clone!(#[weak] terminal, async move …)`, och
/// för ett ASYNC-BLOCK uppgraderar makrot den svaga referensen till en
/// STARK direkt och håller den så länge framtiden kör (verifierat i
/// `glib-macros`-källan, inte antaget). Det slöt en cykel: framtiden höll
/// terminalen, terminalen höll sändaren, sändaren höll bakgrundsloopen
/// vid liv, och loopen var den enda som kunde skicka `Closed` och därmed
/// avsluta framtiden. Följden var att en stängd flik lämnade sin
/// anslutning ÖPPEN — mätt med `ss -tn` mot en riktig server, tre
/// anslutningar levde vidare efter att både rutor och flik stängts.
/// `Sender::close()` stänger kanalen för alla klonerna på en gång och
/// bryter cykeln där den ska brytas.
fn close_session_channel(terminal: &vte::Terminal) {
    unsafe {
        if let Some(input) = terminal.steal_data::<async_channel::Sender<Vec<u8>>>("bastion-ssh-input")
        {
            input.close();
        }
    }
}

/// Stänger EN ruta: stryper anslutningen och låter syskonrutan ta över
/// platsen. Var rutan flikens enda stängs hela fliken, precis som innan
/// delad vy fanns.
fn close_session_pane(area: &Rc<SessionArea>, terminal: &vte::Terminal, page: &adw::TabPage) {
    close_session_channel(terminal);
    if split::close_pane(terminal) == split::PaneClosed::PageEmpty && tab_view_contains(area, page)
    {
        area.tab_view.close_page(page);
    }
}

/// Delar den ruta som har fokus i den valda fliken och öppnar en till
/// session mot samma sak i den nya halvan. Gör ingenting om fliken inte
/// är en terminalflik (Docker/SFTP/tunnlar har inga rutor) eller om rutan
/// inte går att duplicera (seriell port, se [`PaneSource`]).
fn split_focused_pane(area: &Rc<SessionArea>, orientation: gtk::Orientation) {
    let Some(page) = area.tab_view.selected_page() else {
        return;
    };
    let Some(pane) = split::focused_terminal(&page.child()) else {
        return;
    };
    let Some(source) = pane_source(&pane) else {
        return;
    };
    let target = SessionTarget::Split { pane, orientation };

    match source {
        PaneSource::Ssh { host, jump } => {
            let jump = jump.map(|j| *j);
            let host = *host;
            if matches!(host.auth, host::HostAuth::AskPassword) {
                prompt_password_then(area, host, move |area, host, password| {
                    start_session(area, host, Some(password), jump.clone(), target.clone());
                });
            } else {
                start_session(area, host, None, jump, target);
            }
        }
        PaneSource::Telnet { host, port } => start_telnet_session(area, host, port, target),
    }
}

/// Sätter ett bokmärke vid den rad som ligger överst i rutan med fokus.
///
/// Gör ingenting utan en terminalruta — Docker-, SFTP- och tunnelflikar
/// har ingen skrollbuffert att peka in i.
fn bookmark_focused_pane(area: &Rc<SessionArea>) {
    let Some(pane) = focused_pane(area) else {
        return;
    };
    use chrono::Timelike as _;
    let now = chrono::Local::now();
    let label = bookmarks::default_label(now.hour(), now.minute(), now.second());
    let row = pane.vadjustment().map(|adj| adj.value()).unwrap_or(0.0);
    pane_bookmarks(&pane).borrow_mut().add(row, label.clone());
    area.toasts
        .add_toast(adw::Toast::new(&format!("Bokmärke satt: {label}")));
}

/// Terminalrutan med fokus i den valda fliken, om fliken har någon.
fn focused_pane(area: &Rc<SessionArea>) -> Option<vte::Terminal> {
    split::focused_terminal(&area.tab_view.selected_page()?.child())
}

/// Sen bindning av "rita om listan" till sig själv: raderna behöver kunna
/// utlösa en ombyggnad, men de skapas AV ombyggnaden, så stängningen kan
/// inte fånga sig själv utan att gå via en cell som fylls i efteråt.
type Rebuild = Rc<RefCell<Option<Box<dyn Fn()>>>>;

/// Listan över rutans bokmärken, med hopp, omdöpning och borttagning.
///
/// Byggs om från listan varje gång något ändras (`rebuild`) i stället för
/// att raderna uppdateras på plats: en omdöpning kan flytta ingenting,
/// men en borttagning ändrar vilka rader som finns, och två vägar att
/// hålla vyn i synk är en väg för mycket.
fn show_bookmarks_dialog(area: &Rc<SessionArea>) {
    let Some(pane) = focused_pane(area) else {
        return;
    };
    let list = pane_bookmarks(&pane);

    let rows = gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&rows)
        .min_content_height(280)
        .vexpand(true)
        .build();

    // Har skrollbufferten svämmat över pekar bokmärkena på senare rader
    // än de sattes vid. Att säga det rakt ut är hela poängen — se
    // `bookmarks`-modulens kommentar.
    let (rows_in_buffer, visible) = match pane.vadjustment() {
        Some(adj) => (adj.upper(), adj.page_size()),
        None => (0.0, 0.0),
    };
    let drift_banner = adw::Banner::builder()
        .title(
            "Skrollbufferten är full — äldre rader har fallit ur, så \
             bokmärkena kan peka en bit fel.",
        )
        .revealed(bookmarks::positions_may_have_drifted(
            rows_in_buffer,
            f64::from(SCROLLBACK_LINES) + visible,
        ))
        .build();

    let close_button = gtk::Button::with_label("Klar");
    let header = adw::HeaderBar::builder().show_end_title_buttons(false).build();
    header.pack_start(&close_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&drift_banner);
    content.append(&scrolled);

    let win = dialog_window(&session_window(area), "Bokmärken", DialogSize::List, &content);

    let rebuild: Rebuild = Rc::new(RefCell::new(None));
    let build = {
        let rows = rows.clone();
        let list = list.clone();
        let pane = pane.clone();
        let win = win.clone();
        let rebuild = rebuild.clone();
        move || {
            while let Some(child) = rows.first_child() {
                rows.remove(&child);
            }
            if list.borrow().is_empty() {
                let empty = adw::ActionRow::builder()
                    .title("Inga bokmärken")
                    .subtitle("Ctrl+Shift+D sätter ett vid raden överst i rutan")
                    .build();
                rows.append(&empty);
                return;
            }
            for bookmark in list.borrow().all() {
                let row = adw::ActionRow::builder()
                    .title(&bookmark.label)
                    .subtitle(format!("rad {}", bookmark.row.round() as i64))
                    .activatable(true)
                    .build();

                let rename = gtk::Button::builder()
                    .icon_name("document-edit-symbolic")
                    .tooltip_text("Byt namn")
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();
                let remove = gtk::Button::builder()
                    .icon_name("user-trash-symbolic")
                    .tooltip_text("Ta bort")
                    .valign(gtk::Align::Center)
                    .css_classes(["flat"])
                    .build();
                row.add_suffix(&rename);
                row.add_suffix(&remove);

                // Att aktivera raden hoppar dit OCH stänger dialogen: den
                // fanns bara för att hitta tillbaka, och att lämna den
                // öppen skymmer det man just hoppade till.
                let target = bookmark.row;
                row.connect_activated(clone!(
                    #[weak]
                    pane,
                    #[weak]
                    win,
                    move |_| {
                        if let Some(adj) = pane.vadjustment() {
                            adj.set_value(target);
                        }
                        win.close();
                    }
                ));

                let id = bookmark.id;
                remove.connect_clicked(clone!(
                    #[strong]
                    list,
                    #[strong]
                    rebuild,
                    move |_| {
                        list.borrow_mut().remove(id);
                        if let Some(again) = rebuild.borrow().as_ref() {
                            again();
                        }
                    }
                ));

                let current = bookmark.label.clone();
                rename.connect_clicked(clone!(
                    #[strong]
                    list,
                    #[strong]
                    rebuild,
                    #[weak]
                    win,
                    move |_| {
                        let entry = gtk::Entry::builder().text(&current).build();
                        let dialog = adw::AlertDialog::new(Some("Byt namn"), None);
                        dialog.set_extra_child(Some(&entry));
                        dialog.add_response("cancel", "Avbryt");
                        dialog.add_response("save", "Spara");
                        dialog.set_response_appearance(
                            "save",
                            adw::ResponseAppearance::Suggested,
                        );
                        dialog.set_default_response(Some("save"));
                        dialog.connect_response(
                            None,
                            clone!(
                                #[strong]
                                list,
                                #[strong]
                                rebuild,
                                #[weak]
                                entry,
                                move |_, response| {
                                    if response != "save" {
                                        return;
                                    }
                                    let label = entry.text().trim().to_string();
                                    // Ett tomt namn vore sämre än det
                                    // gamla: raden blir omöjlig att skilja
                                    // från grannen.
                                    if label.is_empty() {
                                        return;
                                    }
                                    list.borrow_mut().rename(id, label);
                                    if let Some(again) = rebuild.borrow().as_ref() {
                                        again();
                                    }
                                }
                            ),
                        );
                        dialog.present(Some(&win));
                    }
                ));

                rows.append(&row);
            }
        }
    };
    build();
    *rebuild.borrow_mut() = Some(Box::new(build));

    close_button.connect_clicked(clone!(
        #[weak]
        win,
        move |_| win.close()
    ));

    win.present();
}

/// Stänger rutan med fokus. Ett kortkommando för den som har en hängd
/// session i en av rutorna — en fungerande session stängs annars med
/// `exit` i skalet, som den alltid har kunnat.
fn close_focused_pane(area: &Rc<SessionArea>) {
    let Some(page) = area.tab_view.selected_page() else {
        return;
    };
    let Some(pane) = split::focused_terminal(&page.child()) else {
        // Ingen terminal i fliken (Docker/SFTP/…): då är hela fliken det
        // enda som finns att stänga, samma sak som `app.close-tab`.
        area.tab_view.close_page(&page);
        return;
    };
    close_session_pane(area, &pane, &page);
}

fn start_session(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
    target: SessionTarget,
) {
    let terminal = new_themed_terminal();

    let cols = 80u32;
    let rows = 24u32;
    let session = ssh::spawn_shell(host.clone(), password, cols, rows, jump.clone());

    // Lagras på widgeten så städningen hittar kanalen och kan STÄNGA den
    // när rutan eller fliken stängs — se `close_session_channel` för varför
    // det måste vara en stängning och inte bara ett släpp.
    unsafe {
        terminal.set_data("bastion-ssh-input", session.input.clone());
    }
    set_pane_source(
        &terminal,
        PaneSource::Ssh {
            host: Box::new(host.clone()),
            jump: jump.map(Box::new),
        },
    );

    let title = unique_session_tab_title(
        area,
        &tab_title::base_title(&host.alias, &host.user, &host.host_name),
    );
    let page = place_terminal(area, &terminal, target, &title);

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
                        // RSA-stoppet är något användaren behöver agera på,
                        // inte ett vanligt anslutningsfel — därför en dialog
                        // med klickbar länk ovanpå den röda terminalraden.
                        if msg.starts_with(ssh::RSA_DISABLED_PREFIX) {
                            show_rsa_disabled_dialog(&area);
                        }
                    }
                    SshEvent::Connected => {}
                    SshEvent::Closed => {
                        close_session_pane(&area, &terminal, &page);
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
fn start_telnet_session(area: &Rc<SessionArea>, host: String, port: u16, target: SessionTarget) {
    let terminal = new_themed_terminal();
    let handle = telnet::spawn(host.clone(), port);

    unsafe {
        terminal.set_data("bastion-ssh-input", handle.input.clone());
    }
    set_pane_source(
        &terminal,
        PaneSource::Telnet {
            host: host.clone(),
            port,
        },
    );

    let page = place_terminal(area, &terminal, target, &format!("{host}:{port} (telnet)"));

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
                        close_session_pane(&area, &terminal, &page);
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

    let win = dialog_window(&app_window(app), "Telnet", DialogSize::Compact, &content);

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
            start_telnet_session(&area, host, port, SessionTarget::NewTab);
        }
    ));

    win.present();
}

fn start_serial_session(area: &Rc<SessionArea>, path: String, baud_rate: u32) {
    let terminal = new_themed_terminal();
    let handle = serial::spawn(serial::SerialConfig { path: path.clone(), baud_rate });

    unsafe {
        terminal.set_data("bastion-ssh-input", handle.input.clone());
    }

    // Ingen `set_pane_source`: en seriell port går inte att öppna två
    // gånger, så rutan går inte att duplicera (se `PaneSource`).
    let page = place_terminal(
        area,
        &terminal,
        SessionTarget::NewTab,
        &format!("{path} ({baud_rate} baud)"),
    );

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
                    serial::SerialEvent::Data(bytes) => terminal.feed(&bytes),
                    serial::SerialEvent::Error(msg) => {
                        terminal.feed(
                            format!("\r\n\x1b[31m[bastion] fel: {msg}\x1b[0m\r\n").as_bytes(),
                        );
                    }
                    serial::SerialEvent::Connected => {}
                    serial::SerialEvent::Closed => {
                        close_session_pane(&area, &terminal, &page);
                        break;
                    }
                }
            }
        }
    ));
}

/// Ad-hoc anslutningsdialog för en seriell/USB-port — inget sparande i
/// värdlistan, motsvarar `App/SerialConnectView.swift` (bara sökväg+
/// baudhastighet, ingen auth — en fysisk port är inte en användarkontobar
/// resurs). `serial::available_paths()` föreslår kandidater
/// (`/dev/ttyUSB*`/`/dev/ttyACM*`/`/dev/ttyS*`) men fältet är fritext —
/// listan är best-effort, inte en spärr mot att skriva en annan sökväg.
fn show_serial_connect_dialog(app: &adw::Application, area: &Rc<SessionArea>) {
    let path_row = adw::ComboRow::builder().title("Port").build();
    let available = serial::available_paths();
    let path_model = gtk::StringList::new(&available.iter().map(String::as_str).collect::<Vec<_>>());
    path_row.set_model(Some(&path_model));
    path_row.set_enable_search(true);

    let path_entry_row = adw::EntryRow::builder()
        .title("Egen sökväg (t.ex. /dev/ttyUSB0)")
        .build();
    let baud_row = adw::ComboRow::builder().title("Baudhastighet").build();
    let baud_labels: Vec<String> = serial::COMMON_BAUD_RATES.iter().map(|b| b.to_string()).collect();
    let baud_model = gtk::StringList::new(&baud_labels.iter().map(String::as_str).collect::<Vec<_>>());
    baud_row.set_model(Some(&baud_model));
    // 9600 är den vanligaste standardhastigheten för konsolportar — sätt
    // den som förval om den finns i listan, annars lämna OS-förvalet (index 0).
    if let Some(idx) = serial::COMMON_BAUD_RATES.iter().position(|&b| b == 9600) {
        baud_row.set_selected(idx as u32);
    }

    let group = adw::PreferencesGroup::new();
    if !available.is_empty() {
        group.add(&path_row);
    }
    group.add(&path_entry_row);
    group.add(&baud_row);

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

    let win = dialog_window(&app_window(app), "Seriell/USB-anslutning", DialogSize::Compact, &content);

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
        available,
        move |_| {
            // Fritextfältet vinner om det är ifyllt — annars den valda
            // posten ur den föreslagna listan (om listan visades alls).
            let typed = path_entry_row.text().trim().to_string();
            let path = if !typed.is_empty() {
                typed
            } else if let Some(p) = available.get(path_row.selected() as usize) {
                p.clone()
            } else {
                return;
            };
            let baud_rate = serial::COMMON_BAUD_RATES
                .get(baud_row.selected() as usize)
                .copied()
                .unwrap_or(9600);
            win.close();
            start_serial_session(&area, path, baud_rate);
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

    let win = dialog_window(&app_window(app), "Snabbanslutning", DialogSize::Compact, &content);

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
            start_session(&area, host, password, None, SessionTarget::NewTab);
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
    search_query: &Rc<RefCell<String>>,
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

    let win = dialog_window(&app_window(app), "Tailscale", DialogSize::List, &content);

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
        search_query,
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
                search_query,
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
                                    search_query,
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
                                            &search_query,
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

/// En lista i valvet — samma form som värdlistan, så att sidopanelen ser
/// likadan ut oavsett kategori.
fn vault_list_box() -> gtk::ListBox {
    gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build()
}

/// Raden som visas när en valvkategori är tom.
///
/// En tom `boxed-list` ritas som en blank vit ruta, och en blank ruta går
/// inte att skilja från något som gått sönder. Raden säger både att det
/// är tomt och hur man fyller det.
fn vault_empty_row(title: &str, subtitle: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build();
    row.set_sensitive(false);
    row
}

/// Rullbar behållare runt en valvlista.
fn vault_page(list: &gtk::ListBox) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder()
        .child(list)
        .vexpand(true)
        .build()
}

/// Byter kategori i valvet, angivet med `vault::VaultCategory::id`.
///
/// Går via väljaren och inte direkt till stacken: väljaren måste visa
/// samma sak som panelen, annars står det "Värdar" ovanför en lista med
/// S3-anslutningar. `set_selected` utlöser dess `notify`, som i sin tur
/// byter sida — ett ställe som byter, inte två.
fn select_vault_category(dropdown: &gtk::DropDown, id: &str) {
    if let Some(index) = vault::index_of(id) {
        dropdown.set_selected(index as u32);
    }
}

/// Värdnycklarna appen litar på (`~/.bastion/known_hosts`), med möjlighet
/// att glömma en.
///
/// Filen läses om från disk vid varje uppdatering i stället för att en
/// laddad kopia hålls vid liv: varje ny SSH-session kan lägga till en rad
/// (TOFU), och de raderna skrivs av anslutningens egen `KnownHosts`-
/// instans, inte av den här. En cachad lista hade tyst blivit inaktuell.
fn refresh_known_hosts_list(list: &gtk::ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }

    let known = match known_hosts::KnownHosts::open(Some(known_hosts::KnownHosts::default_path())) {
        Ok(known) => known,
        Err(e) => {
            // Ett läsfel får INTE se ut som "inga kända värdar" — det är
            // hela poängen med att `open` fallerar i stället för att ge
            // en tom karta (se `known_hosts::load`). Raden säger vad som
            // hänt i stället för att panelen ser tom och lugn ut.
            let row = adw::ActionRow::builder()
                .title("Kunde inte läsa kända värdar")
                .subtitle(e.to_string())
                .build();
            row.add_css_class("error");
            list.append(&row);
            return;
        }
    };

    let entries = known.entries();
    if entries.is_empty() {
        list.append(&vault_empty_row(
            "Inga kända värdar än",
            "En värd hamnar här första gången du ansluter till den",
        ));
        return;
    }

    for entry in entries {
        let row = adw::ActionRow::builder()
            .title(&entry.id)
            .subtitle(format!("{} · {}", entry.algorithm(), entry.fingerprint()))
            .subtitle_selectable(true)
            .build();

        let forget_button = gtk::Button::from_icon_name("user-trash-symbolic");
        forget_button.set_tooltip_text(Some("Glöm den här värdnyckeln"));
        forget_button.add_css_class("flat");
        forget_button.connect_clicked(clone!(
            #[weak]
            list,
            #[strong(rename_to = id)]
            entry.id,
            move |button| {
                // Bekräftelse först: att glömma en nyckel är att stänga
                // av MITM-skyddet för den värden till nästa anslutning.
                let dialog = adw::AlertDialog::new(
                    Some("Glömma värdnyckeln?"),
                    Some(&format!(
                        "Nästa anslutning till {id} godtar vilken nyckel servern än \
                         visar, utan att jämföra med den här. Gör det bara om du VET \
                         att servern bytt nyckel."
                    )),
                );
                dialog.add_response("cancel", "Avbryt");
                dialog.add_response("forget", "Glöm");
                dialog.set_response_appearance("forget", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.connect_response(
                    None,
                    clone!(
                        #[weak]
                        list,
                        #[strong]
                        id,
                        move |_, response| {
                            if response != "forget" {
                                return;
                            }
                            let opened = known_hosts::KnownHosts::open(Some(
                                known_hosts::KnownHosts::default_path(),
                            ));
                            match opened.and_then(|known| known.forget(&id)) {
                                Ok(_) => refresh_known_hosts_list(&list),
                                Err(e) => eprintln!("kunde inte glömma värdnyckeln: {e}"),
                            }
                        }
                    ),
                );
                dialog.present(Some(button));
            }
        ));
        row.add_suffix(&forget_button);

        list.append(&row);
    }
}

/// Valvets WireGuard-profiler — toppnivå, inte kopplat till en specifik
/// `Host` (en profil beskriver en VPN-anslutning, inte en SSH-värd).
/// Samma v1-avgränsning som App/-motsvarigheten: profilhantering
/// (klistra in/visa/redigera/ta bort `.conf`-text), INTE att faktiskt
/// upprätta tunneln — se `wireguard.rs`s doc-kommentar.
fn refresh_wireguard_profile_list(
    app: &adw::Application,
    wireguard_store: &Rc<RefCell<wireguard::WireGuardProfileStore>>,
    list: &gtk::ListBox,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    if wireguard_store.borrow().all().is_empty() {
        list.append(&vault_empty_row(
            "Inga profiler än",
            "Lägg till en med + och klistra in din .conf-text",
        ));
        return;
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
            #[strong(rename_to = profile_id)]
            profile.id,
            move |_| {
                if let Err(e) = wireguard_store.borrow_mut().delete(profile_id) {
                    eprintln!("kunde inte ta bort wireguard-profilen: {e}");
                    return;
                }
                refresh_wireguard_profile_list(&app, &wireguard_store, &list);
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
            #[strong]
            profile,
            move |_| show_wireguard_profile_edit(&app, &wireguard_store, &list, profile.clone())
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

    let win = dialog_window(&app_window(app), "WireGuard-profil", DialogSize::Form, &content);

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
            refresh_wireguard_profile_list(&app, &wireguard_store, &list);
            win.close();
        }
    ));

    win.present();
}

/// Valvets S3-anslutningar. "Bläddra" på en rad öppnar en bucket-/
/// objektbläddare i en ny flik (`open_s3_bucket_browser`); raden själv
/// öppnar redigeringsdialogen (namn/endpoint/region/nycklar).
fn refresh_s3_connection_list(
    app: &adw::Application,
    area: &Rc<SessionArea>,
    s3_store: &Rc<RefCell<s3::S3ConnectionStore>>,
    list: &gtk::ListBox,
) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    if s3_store.borrow().all().is_empty() {
        list.append(&vault_empty_row(
            "Inga anslutningar än",
            "Lägg till en med + (endpoint, region och nycklar)",
        ));
        return;
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
            #[strong]
            connection,
            move |_| {
                // Ingen stängning här längre: listan bor i sidopanelen,
                // och det som förr var dialogfönstret är numera appens
                // huvudfönster.
                open_s3_bucket_browser(&area, connection.clone());
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
            #[strong(rename_to = connection_id)]
            connection.id,
            move |_| {
                if let Err(e) = s3_store.borrow_mut().delete(connection_id) {
                    eprintln!("kunde inte ta bort s3-anslutningen: {e}");
                    return;
                }
                refresh_s3_connection_list(&app, &area, &s3_store, &list);
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
            #[strong]
            connection,
            move |_| show_s3_connection_edit(&app, &area, &s3_store, &list, connection.clone())
        ));

        list.append(&row);
    }
}

fn show_s3_connection_edit(
    app: &adw::Application,
    area: &Rc<SessionArea>,
    s3_store: &Rc<RefCell<s3::S3ConnectionStore>>,
    list: &gtk::ListBox,
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

    let win = dialog_window(&app_window(app), "S3-anslutning", DialogSize::Form, &content);

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
            refresh_s3_connection_list(&app, &area, &s3_store, &list);
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
                    // Strömmar direkt till fil — ett flergigabyte-objekt
                    // (backup, diskavbildning) ska inte behöva rymmas i
                    // RAM först, se `s3::S3Client::get_object_to_file`.
                    let rx = s3::spawn_download_object(connection, bucket_name, key, local_path.clone());
                    match rx.recv().await {
                        Ok(Ok(())) => {}
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

    let win = dialog_window(&session_window(area), "Ny bucket", DialogSize::Compact, &content);

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
/// Kubernetes-vy för `host`: poddar, deployments och noder via `kubectl`
/// över SSH.
///
/// Andra integrationen bredvid Docker, och medvetet byggd med samma
/// mönster — en `ViewSwitcher` över kategorier, bara den synliga
/// kategorin hämtas, GTK-fri radmappning som går att testa. När en
/// tredje tillkommer är det de här tre likheterna som är värda att
/// extrahera, inte en abstraktion gissad i förväg.
///
/// Namnrymdsväljaren är det som INTE finns i Docker-vyn. Den fylls från
/// klustret vid öppning, med "Alla namnrymder" först: den som felsöker
/// vet sällan i vilken namnrymd problemet sitter.
fn open_kubernetes_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
) {
    let pods_list = docker_category_list();
    let deployments_list = docker_category_list();
    let nodes_list = docker_category_list();

    let stack = adw::ViewStack::new();
    let category = adw::ViewSwitcher::builder()
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    category.set_stack(Some(&stack));
    stack.add_titled(&docker_category_scroller(&pods_list), Some("pods"), "Poddar");
    stack.add_titled(&docker_category_scroller(&deployments_list), Some("deployments"), "Deployments");
    stack.add_titled(&docker_category_scroller(&nodes_list), Some("nodes"), "Noder");

    // "Alla namnrymder" ligger först och är förval. Noder påverkas inte —
    // de är kluster-globala.
    let namespace_row = gtk::DropDown::from_strings(&["Alla namnrymder"]);
    namespace_row.set_valign(gtk::Align::Center);
    namespace_row.set_tooltip_text(Some("Namnrymd"));
    let namespaces: Rc<RefCell<Vec<kubernetes::Namespace>>> =
        Rc::new(RefCell::new(vec![kubernetes::Namespace::All]));

    let refresh_button = gtk::Button::from_icon_name("view-refresh-symbolic");
    refresh_button.set_tooltip_text(Some("Uppdatera"));

    let toolbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    toolbar.append(
        &gtk::Label::builder()
            .label(format!("Kubernetes: {}", host.alias))
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build(),
    );
    toolbar.append(&namespace_row);
    toolbar.append(&category);
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&stack);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("K8s: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    let load_visible = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let stack = stack.clone();
        let namespace_row = namespace_row.clone();
        let namespaces = namespaces.clone();
        let pods_list = pods_list.clone();
        let deployments_list = deployments_list.clone();
        let nodes_list = nodes_list.clone();
        move || {
            let selected = namespaces
                .borrow()
                .get(namespace_row.selected() as usize)
                .cloned()
                .unwrap_or(kubernetes::Namespace::All);
            let name = stack.visible_child_name().unwrap_or_else(|| "pods".into());
            match name.as_str() {
                "deployments" => refresh_kubernetes_category(
                    &area, host.clone(), password.clone(), &deployments_list, jump.clone(),
                    KubernetesCategory::Deployments, selected,
                ),
                "nodes" => refresh_kubernetes_category(
                    &area, host.clone(), password.clone(), &nodes_list, jump.clone(),
                    KubernetesCategory::Nodes, selected,
                ),
                _ => refresh_kubernetes_category(
                    &area, host.clone(), password.clone(), &pods_list, jump.clone(),
                    KubernetesCategory::Pods, selected,
                ),
            }
        }
    };

    refresh_button.connect_clicked({
        let load_visible = load_visible.clone();
        move |_| load_visible()
    });
    stack.connect_visible_child_name_notify({
        let load_visible = load_visible.clone();
        move |_| load_visible()
    });
    namespace_row.connect_selected_notify({
        let load_visible = load_visible.clone();
        move |_| load_visible()
    });

    // Namnrymderna hämtas en gång vid öppning. Listan ändras sällan, och
    // att hämta om den vid varje kategoribyte vore en round-trip för
    // data som nästan aldrig skiljer sig.
    let rx = ssh::run_command(
        host.clone(),
        password.clone(),
        kubernetes::namespaces_command(),
        jump.clone(),
    );
    glib::spawn_future_local(clone!(
        #[weak]
        namespace_row,
        #[strong]
        namespaces,
        #[strong]
        load_visible,
        async move {
            if let Ok(Ok(output)) = rx.recv().await {
                let found = kubernetes::parse_namespaces(&output);
                if !found.is_empty() {
                    let mut labels = vec!["Alla namnrymder".to_string()];
                    let mut values = vec![kubernetes::Namespace::All];
                    for name in found {
                        labels.push(name.clone());
                        values.push(kubernetes::Namespace::Named(name));
                    }
                    let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
                    namespace_row.set_model(Some(&gtk::StringList::new(&refs)));
                    *namespaces.borrow_mut() = values;
                }
            }
            // Körs oavsett: misslyckas namnrymdshämtningen ska poddarna
            // ändå visas (eller felet från dem synas), inte en tom vy.
            load_visible();
        }
    ));
}

/// Hämtar och ritar en av de tre Kubernetes-listorna.
fn refresh_kubernetes_category(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    list: &gtk::ListBox,
    jump: Option<host::Host>,
    category: KubernetesCategory,
    namespace: kubernetes::Namespace,
) {
    let command = match category {
        KubernetesCategory::Pods => kubernetes::pods_command(&namespace),
        KubernetesCategory::Deployments => kubernetes::deployments_command(&namespace),
        KubernetesCategory::Nodes => Ok(kubernetes::nodes_command()),
    };
    let empty = (
        match category {
            KubernetesCategory::Pods => "Inga poddar",
            KubernetesCategory::Deployments => "Inga deployments",
            KubernetesCategory::Nodes => "Inga noder",
        },
        // `kubectl` som saknas ger tom utdata här, eftersom felet gått
        // till /dev/null — därför nämns det i stället för att lämna
        // användaren att gissa.
        Some("Tomt svar — kontrollera att kubectl finns på värden och når ett kluster"),
    );

    refresh_integration_list(
        host.clone(),
        password.clone(),
        list,
        jump.clone(),
        command,
        empty,
        clone!(
            #[strong]
            area,
            #[strong]
            host,
            #[strong]
            password,
            #[strong]
            jump,
            #[strong]
            namespace,
            move |output: &str, list: &gtk::ListBox| {
                for (title, subtitle, target) in
                    kubernetes_category_rows(output, category, &namespace)
                {
                    list.append(&build_kubernetes_row(
                        &area, &host, &password, list, &jump, category, &namespace, title,
                        subtitle, target,
                    ));
                }
            }
        ),
    );
}

/// En rad i podd-, deployment- eller nodlistan.
#[allow(clippy::too_many_arguments)]
fn build_kubernetes_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    list: &gtk::ListBox,
    jump: &Option<host::Host>,
    category: KubernetesCategory,
    namespace: &kubernetes::Namespace,
    title: String,
    subtitle: String,
    target: Option<(String, String)>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(&title).subtitle(&subtitle).build();

    let Some((ns, name)) = target else {
        return row; // noder har inga åtgärder
    };

    let reload = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let list = list.clone();
        let namespace = namespace.clone();
        move || {
            refresh_kubernetes_category(
                &area, host.clone(), password.clone(), &list, jump.clone(), category,
                namespace.clone(),
            )
        }
    };

    let run_then_reload = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let reload = reload.clone();
        move |command: Result<String, String>| {
            let Ok(command) = command else {
                show_message_dialog(&area, "Kubernetes", "ogiltigt namn — kommandot byggdes aldrig");
                return;
            };
            let rx = ssh::run_command(host.clone(), password.clone(), command, jump.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                reload,
                async move {
                    match rx.recv().await {
                        Ok(Ok(_)) => reload(),
                        Ok(Err(e)) => show_message_dialog(&area, "Kubernetes", &e),
                        Err(_) => show_message_dialog(&area, "Kubernetes", "SSH-anslutningen avbröts oväntat"),
                    }
                }
            ));
        }
    };

    if category == KubernetesCategory::Pods {
        // `describe` före loggar: när en podd inte startar finns svaret i
        // händelserna, inte i loggen — den är tom.
        let describe = gtk::Button::from_icon_name("dialog-information-symbolic");
        describe.set_tooltip_text(Some("Beskriv (händelser och orsak)"));
        describe.set_valign(gtk::Align::Center);
        describe.add_css_class("flat");
        describe.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            host,
            #[strong]
            password,
            #[strong]
            jump,
            #[strong]
            ns,
            #[strong]
            name,
            move |_| {
                let Ok(command) = kubernetes::describe_pod_command(&ns, &name) else {
                    show_message_dialog(&area, "Kubernetes", "ogiltigt namn");
                    return;
                };
                show_command_output(&area, &host, &password, &jump, &format!("describe {name}"), command);
            }
        ));
        row.add_suffix(&describe);

        let logs = gtk::Button::from_icon_name("text-x-generic-symbolic");
        logs.set_tooltip_text(Some("Visa loggar"));
        logs.set_valign(gtk::Align::Center);
        logs.add_css_class("flat");
        logs.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            host,
            #[strong]
            password,
            #[strong]
            jump,
            #[strong]
            ns,
            #[strong]
            name,
            move |_| {
                let Ok(command) = kubernetes::pod_logs_command(&ns, &name, 200) else {
                    show_message_dialog(&area, "Kubernetes", "ogiltigt namn");
                    return;
                };
                show_command_output(&area, &host, &password, &jump, &format!("logs {name}"), command);
            }
        ));
        row.add_suffix(&logs);

        // Bekräftas, och texten säger vad som FAKTISKT händer: podden
        // raderas. Har den ingen controller kommer den inte tillbaka, och
        // den som tror att knappen betyder "starta om" blir förvånad.
        let delete = gtk::Button::from_icon_name("view-refresh-symbolic");
        delete.set_tooltip_text(Some("Ersätt podden (radera)"));
        delete.set_valign(gtk::Align::Center);
        delete.add_css_class("flat");
        delete.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            run_then_reload,
            #[strong]
            ns,
            #[strong]
            name,
            move |_| {
                let dialog = adw::AlertDialog::new(
                    Some("Ersätt podden"),
                    Some(&format!(
                        "Radera {name} i {ns}?\n\nEn podd startas aldrig om — den ersätts. \
                         Har den en controller (Deployment, StatefulSet, DaemonSet) skapas en ny \
                         direkt. Saknar den controller kommer den INTE tillbaka."
                    )),
                );
                dialog.add_response("cancel", "Avbryt");
                dialog.add_response("delete", "Radera");
                dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.connect_response(
                    None,
                    clone!(
                        #[strong]
                        run_then_reload,
                        #[strong]
                        ns,
                        #[strong]
                        name,
                        move |_, response| {
                            if response == "delete" {
                                run_then_reload(kubernetes::delete_pod_command(&ns, &name));
                            }
                        }
                    ),
                );
                dialog.present(Some(&area.overlay));
            }
        ));
        row.add_suffix(&delete);
    }

    if category == KubernetesCategory::Deployments {
        // Rullande omstart, inte radering — poddarna byts en i taget utan
        // avbrott. Därför behöver den ingen bekräftelse.
        let restart = gtk::Button::from_icon_name("view-refresh-symbolic");
        restart.set_tooltip_text(Some("Rullande omstart"));
        restart.set_valign(gtk::Align::Center);
        restart.add_css_class("flat");
        restart.connect_clicked({
            let run_then_reload = run_then_reload.clone();
            let ns = ns.clone();
            let name = name.clone();
            move |_| run_then_reload(kubernetes::restart_deployment_command(&ns, &name))
        });
        row.add_suffix(&restart);
    }

    row
}

/// Vilken Kubernetes-resurs en listning gäller.
#[derive(Debug, Clone, Copy, PartialEq)]
enum KubernetesCategory {
    Pods,
    Deployments,
    Nodes,
}

/// Rådata → (rubrik, underrubrik, åtgärder).
///
/// GTK-fri och därför testbar, precis som `docker_category_rows`. Sista
/// fältet är `(namnrymd, namn)` för de poster som har åtgärder; noder har
/// inga och får `None`.
type KubernetesRow = (String, String, Option<(String, String)>);

fn kubernetes_category_rows(
    output: &str,
    category: KubernetesCategory,
    namespace: &kubernetes::Namespace,
) -> Vec<KubernetesRow> {
    match category {
        KubernetesCategory::Pods => kubernetes::parse_pods(output, namespace)
            .into_iter()
            .map(|p| {
                // Ohälsosamma poddar säger VAD som är fel i underrubriken.
                // "Running" räcker inte som lugnande besked när bara en av
                // tre containrar är uppe.
                let mut subtitle = format!("{} · {} klara", p.status, p.ready);
                if p.restarts != "0" {
                    subtitle.push_str(&format!(" · {} omstarter", p.restarts));
                }
                if !p.is_healthy() {
                    subtitle.push_str(" · ⚠");
                }
                let title = format!("{}/{}", p.namespace, p.name);
                (title, subtitle, Some((p.namespace, p.name)))
            })
            .collect(),
        KubernetesCategory::Deployments => kubernetes::parse_deployments(output, namespace)
            .into_iter()
            .map(|d| {
                let subtitle = if d.is_fully_available() {
                    format!("{} tillgängliga", d.ready)
                } else {
                    format!("{} tillgängliga · ⚠", d.ready)
                };
                (format!("{}/{}", d.namespace, d.name), subtitle, Some((d.namespace, d.name)))
            })
            .collect(),
        KubernetesCategory::Nodes => kubernetes::parse_nodes(output)
            .into_iter()
            .map(|n| {
                let mut subtitle = n.version.clone();
                if n.is_cordoned() {
                    subtitle.push_str(" · avstängd för schemaläggning");
                }
                if !n.is_ready() {
                    subtitle.push_str(" · ⚠ inte redo");
                }
                (n.name, subtitle, None)
            })
            .collect(),
    }
}

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

    // VISION räknar upp Containers/Images/Volumes/Networks som en och
    // samma Docker-yta. En växlare i stället för fyra flikar: det är fyra
    // vyer av EN värd, och att öppna fyra flikar per server hade gjort
    // flikraden oläslig så fort man tittar på mer än en maskin.
    let category = adw::ViewSwitcher::builder()
        .policy(adw::ViewSwitcherPolicy::Wide)
        .build();
    let stack = adw::ViewStack::new();
    category.set_stack(Some(&stack));

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
    toolbar.append(&category);
    toolbar.append(&refresh_button);

    let images_list = docker_category_list();
    let volumes_list = docker_category_list();
    let networks_list = docker_category_list();
    stack.add_titled(&scrolled, Some("containers"), "Containrar");
    stack.add_titled(&docker_category_scroller(&images_list), Some("images"), "Images");
    stack.add_titled(&docker_category_scroller(&volumes_list), Some("volumes"), "Volymer");
    stack.add_titled(&docker_category_scroller(&networks_list), Some("networks"), "Nätverk");
    let compose_list = docker_category_list();
    stack.add_titled(&docker_category_scroller(&compose_list), Some("compose"), "Compose");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&stack);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Docker: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    // Bara den synliga kategorin hämtas. Fyra SSH-kommandon vid varje
    // öppning hade kostat fyra round-trips för tre listor man kanske
    // aldrig tittar på — och `docker images` på en välanvänd värd är inte
    // gratis.
    let load_visible = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let stack = stack.clone();
        let list = list.clone();
        let images_list = images_list.clone();
        let volumes_list = volumes_list.clone();
        let networks_list = networks_list.clone();
        let compose_list = compose_list.clone();
        move || {
            let name = stack.visible_child_name().unwrap_or_else(|| "containers".into());
            match name.as_str() {
                "images" => refresh_docker_category(
                    &area, host.clone(), password.clone(), &images_list, jump.clone(),
                    DockerCategory::Images,
                ),
                "volumes" => refresh_docker_category(
                    &area, host.clone(), password.clone(), &volumes_list, jump.clone(),
                    DockerCategory::Volumes,
                ),
                "networks" => refresh_docker_category(
                    &area, host.clone(), password.clone(), &networks_list, jump.clone(),
                    DockerCategory::Networks,
                ),
                "compose" => refresh_docker_category(
                    &area, host.clone(), password.clone(), &compose_list, jump.clone(),
                    DockerCategory::Compose,
                ),
                _ => refresh_docker_list(&area, host.clone(), password.clone(), &list, jump.clone()),
            }
        }
    };

    refresh_button.connect_clicked({
        let load_visible = load_visible.clone();
        move |_| load_visible()
    });
    stack.connect_visible_child_name_notify({
        let load_visible = load_visible.clone();
        move |_| load_visible()
    });

    load_visible();
}

/// Vilken Docker-resurs en listning gäller. Skiljer sig från containrarna
/// genom att vara ren läsning plus borttagning — ingen start/stopp/logg,
/// för det finns inget att köra.
#[derive(Debug, Clone, Copy, PartialEq)]
enum DockerCategory {
    Images,
    Volumes,
    Networks,
    /// Compose skiljer sig från de andra tre: posterna går att STARTA och
    /// STOPPA, inte bara tas bort. Raderna byggs därför av
    /// `build_compose_row` i stället för `build_docker_resource_row`.
    Compose,
}

/// Hämtar och ritar en av de tre resurslistorna.
///
/// En funktion för alla tre i stället för tre nästan identiska: det enda
/// som skiljer är kommandot, parsningen och vad raden heter — och tre
/// kopior av samma hämta-rensa-rita-slinga hade drivit isär vid första
/// felhanteringsändringen.
/// Hämtar ett kommandos utdata över SSH och ritar om en lista med det.
///
/// # Vad som visade sig gemensamt, och vad som inte gjorde det
///
/// Extraherad ur `refresh_docker_category` och
/// `refresh_kubernetes_category` EFTER att båda fanns, inte gissad i
/// förväg. Skelettet var identiskt rad för rad: rensa listan, tre utfall
/// från kanalen, tom utdata som eget fall, annars rita rader.
///
/// Radbyggarna var det INTE — 126 mot 187 rader, olika knappar, olika
/// bekräftelsetexter, olika begrepp (ta bort en volym mot ersätta en
/// podd). Att pressa ihop dem hade gett en funktion med ett dussin
/// flaggor och sämre felmeddelanden. Därför tar den här funktionen en
/// stängning som får rita raderna själv.
///
/// `command` kommer in FÄRDIGBYGGT, inklusive eventuellt valideringsfel:
/// varje integration har sin egen namnregel (Dockers tillåter versaler
/// och punkter, Kubernetes RFC 1123-etiketter) och den kunskapen hör
/// hemma i respektive modul, inte här.
///
/// `empty` är rubrik plus valfri förklaring för tomt svar. Att skilja
/// "inget finns" från "det gick fel" är hela poängen — en tom lista och
/// en trasig ser annars likadana ut.
fn refresh_integration_list(
    host: host::Host,
    password: Option<String>,
    list: &gtk::ListBox,
    jump: Option<host::Host>,
    command: Result<String, String>,
    empty: (&'static str, Option<&'static str>),
    build_rows: impl Fn(&str, &gtk::ListBox) + 'static,
) {
    let clear = |list: &gtk::ListBox| {
        while let Some(row) = list.row_at_index(0) {
            list.remove(&row);
        }
    };

    let command = match command {
        Ok(command) => command,
        Err(e) => {
            clear(list);
            list.append(&error_row(&e));
            return;
        }
    };

    let rx = ssh::run_command(host, password, command, jump);
    glib::spawn_future_local(clone!(
        #[weak]
        list,
        async move {
            clear(&list);
            let output = match rx.recv().await {
                Ok(Ok(output)) => output,
                Ok(Err(e)) => {
                    list.append(&error_row(&e));
                    return;
                }
                Err(_) => {
                    list.append(&error_row("SSH-anslutningen avbröts oväntat"));
                    return;
                }
            };

            let before = list.row_at_index(0).is_some();
            build_rows(&output, &list);
            if !before && list.row_at_index(0).is_none() {
                let (title, subtitle) = empty;
                let row = adw::ActionRow::builder().title(title);
                let row = match subtitle {
                    Some(subtitle) => row.subtitle(subtitle),
                    None => row,
                };
                list.append(&row.build());
            }
        }
    ));
}

fn refresh_docker_category(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    list: &gtk::ListBox,
    jump: Option<host::Host>,
    category: DockerCategory,
) {
    let command = Ok(match category {
        DockerCategory::Images => docker::images_command(),
        DockerCategory::Volumes => docker::volumes_command(),
        DockerCategory::Networks => docker::networks_command(),
        DockerCategory::Compose => docker::compose_ls_command(),
    });
    let empty = (
        match category {
            DockerCategory::Images => "Inga images",
            DockerCategory::Volumes => "Inga volymer",
            DockerCategory::Networks => "Inga nätverk",
            DockerCategory::Compose => "Inga Compose-projekt",
        },
        None,
    );

    refresh_integration_list(
        host.clone(),
        password.clone(),
        list,
        jump.clone(),
        command,
        empty,
        clone!(
            #[strong]
            area,
            #[strong]
            host,
            #[strong]
            password,
            #[strong]
            jump,
            move |output: &str, list: &gtk::ListBox| {
                // Compose har egen radbyggare: ett projekt startas och
                // stoppas som helhet och har inget "ta bort".
                if category == DockerCategory::Compose {
                    for project in docker::parse_compose_projects(output) {
                        list.append(&build_compose_row(&area, &host, &password, list, &jump, project));
                    }
                    return;
                }
                for (title, subtitle, removable) in docker_category_rows(output, category) {
                    list.append(&build_docker_resource_row(
                        &area, &host, &password, list, &jump, category, title, subtitle, removable,
                    ));
                }
            }
        ),
    );
}

/// En rad i images/volymer/nätverk-listan.
///
/// Borttagning är den enda muterande åtgärden här, och den är alltid
/// bekräftad: `docker rmi`/`volume rm` är oåterkalleligt, och en volym
/// bär data. Images får dessutom en `pull` — den hämtar en nyare version
/// men startar INTE om något, så den går alltid att låta bli att agera på.
#[allow(clippy::too_many_arguments)]
fn build_docker_resource_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    list: &gtk::ListBox,
    jump: &Option<host::Host>,
    category: DockerCategory,
    title: String,
    subtitle: String,
    removable: Option<String>,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(&title).subtitle(&subtitle).build();

    let reload = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let list = list.clone();
        move || refresh_docker_category(&area, host.clone(), password.clone(), &list, jump.clone(), category)
    };

    let run_then_reload = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let list = list.clone();
        let reload = reload.clone();
        move |command: Result<String, String>| {
            let Ok(command) = command else {
                list.append(&error_row("kunde inte bygga kommandot — ogiltig referens"));
                return;
            };
            let rx = ssh::run_command(host.clone(), password.clone(), command, jump.clone());
            glib::spawn_future_local(clone!(
                #[strong]
                area,
                #[strong]
                reload,
                async move {
                    match rx.recv().await {
                        Ok(Ok(_)) => reload(),
                        Ok(Err(e)) => show_message_dialog(&area, "Docker", &e),
                        Err(_) => show_message_dialog(&area, "Docker", "SSH-anslutningen avbröts oväntat"),
                    }
                }
            ));
        }
    };

    // Bara images kan hämtas om; en volym eller ett nätverk har ingen
    // uppström att jämföra mot.
    if let (DockerCategory::Images, Some(reference)) = (category, removable.clone()) {
        let pull = gtk::Button::from_icon_name("folder-download-symbolic");
        pull.set_tooltip_text(Some("Hämta nyare version (startar inte om något)"));
        pull.set_valign(gtk::Align::Center);
        pull.add_css_class("flat");
        pull.connect_clicked({
            let run_then_reload = run_then_reload.clone();
            move |_| run_then_reload(docker::pull_image_command(&reference))
        });
        row.add_suffix(&pull);
    }

    if let Some(reference) = removable {
        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.set_tooltip_text(Some("Ta bort"));
        remove.set_valign(gtk::Align::Center);
        remove.add_css_class("flat");
        remove.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            run_then_reload,
            #[strong]
            reference,
            #[strong]
            title,
            move |_| {
                let body = match category {
                    DockerCategory::Images => format!("Ta bort imagen {title}?"),
                    // Volymen är den enda som bär data — säg det, i
                    // stället för att lita på att användaren vet.
                    DockerCategory::Volumes => {
                        format!("Ta bort volymen {title}? All data i den försvinner.")
                    }
                    DockerCategory::Networks => format!("Ta bort nätverket {title}?"),
                    // Compose ritas av `build_compose_row` och kommer
                    // aldrig hit. Ett projekt "tas" inte heller bort — det
                    // stoppas med `down`, vilket är en annan sak.
                    DockerCategory::Compose => unreachable!("Compose har egen radbyggare"),
                };
                let dialog = adw::AlertDialog::new(Some("Ta bort"), Some(&body));
                dialog.add_response("cancel", "Avbryt");
                dialog.add_response("remove", "Ta bort");
                dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.connect_response(
                    None,
                    clone!(
                        #[strong]
                        run_then_reload,
                        #[strong]
                        reference,
                        move |_, response| {
                            if response != "remove" {
                                return;
                            }
                            run_then_reload(match category {
                                DockerCategory::Images => docker::remove_image_command(&reference),
                                DockerCategory::Volumes => docker::remove_volume_command(&reference),
                                DockerCategory::Networks => docker::remove_network_command(&reference),
                                DockerCategory::Compose => unreachable!("Compose har egen radbyggare"),
                            });
                        }
                    ),
                );
                dialog.present(Some(&area.overlay));
            }
        ));
        row.add_suffix(&remove);
    }

    row
}

/// En rad för ett Compose-projekt.
///
/// Egen byggare i stället för `build_docker_resource_row`, för ett
/// projekt är inte en resurs som tas bort: det STARTAS och STOPPAS. Ett
/// `down` river dessutom containrarna, vilket är ett driftbeslut och
/// därför bekräftas — till skillnad från `up` och `restart`, som är
/// återställande.
fn build_compose_row(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    list: &gtk::ListBox,
    jump: &Option<host::Host>,
    project: docker::ComposeProject,
) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&project.name)
        .subtitle(&project.status)
        .build();

    let run_then_reload = {
        let area = area.clone();
        let host = host.clone();
        let password = password.clone();
        let jump = jump.clone();
        let list = list.clone();
        move |command: Result<String, String>| {
            let command = match command {
                Ok(command) => command,
                Err(e) => {
                    // Felet är begripligt (t.ex. citattecken i sökvägen)
                    // och förtjänar att synas ordagrant.
                    show_message_dialog(&area, "Compose", &e);
                    return;
                }
            };
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
                #[weak]
                list,
                async move {
                    match rx.recv().await {
                        Ok(Ok(_)) => refresh_docker_category(
                            &area, host, password, &list, jump, DockerCategory::Compose,
                        ),
                        Ok(Err(e)) => show_message_dialog(&area, "Compose", &e),
                        Err(_) => show_message_dialog(
                            &area,
                            "Compose",
                            "SSH-anslutningen avbröts oväntat",
                        ),
                    }
                }
            ));
        }
    };

    let files = project.config_files.clone();

    let logs = gtk::Button::from_icon_name("text-x-generic-symbolic");
    logs.set_tooltip_text(Some("Visa loggar"));
    logs.set_valign(gtk::Align::Center);
    logs.add_css_class("flat");
    logs.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[strong]
        files,
        #[strong(rename_to = name)]
        project.name,
        move |_| {
            let Ok(command) = docker::compose_logs_command(&files, 200) else {
                show_message_dialog(&area, "Compose", "projektet saknar compose-filer");
                return;
            };
            show_command_output(&area, &host, &password, &jump, &format!("Compose: {name}"), command);
        }
    ));
    row.add_suffix(&logs);

    // Uppdatering finns bara här, inte på containerraderna. Skälet står
    // i `docker::compose_update_command`: `docker` kan inte byta image på
    // en körande container, och att riva och återskapa den kräver en
    // konfiguration vi inte har. Compose har den, i filerna.
    //
    // Bekräftas trots att `up -d` bara rör det som ändrats: en ny image
    // kan innehålla en migrering eller en brytande ändring, och det ska
    // vara ett aktivt val — inte ett klick bredvid "visa loggar".
    let update = gtk::Button::from_icon_name("software-update-available-symbolic");
    update.set_tooltip_text(Some("Uppdatera (pull + up -d)"));
    update.set_valign(gtk::Align::Center);
    update.add_css_class("flat");
    update.connect_clicked(clone!(
        #[strong]
        area,
        #[strong]
        run_then_reload,
        #[strong]
        files,
        #[strong(rename_to = name)]
        project.name,
        move |_| {
            let dialog = adw::AlertDialog::new(
                Some("Uppdatera projektet"),
                Some(&format!(
                    "Hämta nyare images för {name} och återskapa de tjänster vars \
                     image ändrats?\n\nOförändrade tjänster startas inte om. \
                     Hämtningen måste lyckas innan något återskapas."
                )),
            );
            dialog.add_response("cancel", "Avbryt");
            dialog.add_response("update", "Uppdatera");
            dialog.set_response_appearance("update", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("cancel"));
            dialog.connect_response(
                None,
                clone!(
                    #[strong]
                    run_then_reload,
                    #[strong]
                    files,
                    move |_, response| {
                        if response == "update" {
                            run_then_reload(docker::compose_update_command(&files));
                        }
                    }
                ),
            );
            dialog.present(Some(&area.overlay));
        }
    ));
    row.add_suffix(&update);

    // `up` och `restart` är återställande och bekräftas inte. `down`
    // river containrarna och gör det.
    if project.is_running() {
        let restart = gtk::Button::from_icon_name("view-refresh-symbolic");
        restart.set_tooltip_text(Some("Starta om projektet"));
        restart.set_valign(gtk::Align::Center);
        restart.add_css_class("flat");
        restart.connect_clicked({
            let run_then_reload = run_then_reload.clone();
            let files = files.clone();
            move |_| run_then_reload(docker::compose_restart_command(&files))
        });
        row.add_suffix(&restart);

        let down = gtk::Button::from_icon_name("media-playback-stop-symbolic");
        down.set_tooltip_text(Some("Stoppa projektet (down)"));
        down.set_valign(gtk::Align::Center);
        down.add_css_class("flat");
        down.connect_clicked(clone!(
            #[strong]
            area,
            #[strong]
            run_then_reload,
            #[strong]
            files,
            #[strong(rename_to = name)]
            project.name,
            move |_| {
                let dialog = adw::AlertDialog::new(
                    Some("Stoppa projektet"),
                    Some(&format!(
                        "Kör `docker compose down` för {name}? Containrarna rivs — \
                         namngivna volymer rörs inte."
                    )),
                );
                dialog.add_response("cancel", "Avbryt");
                dialog.add_response("down", "Stoppa");
                dialog.set_response_appearance("down", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.connect_response(
                    None,
                    clone!(
                        #[strong]
                        run_then_reload,
                        #[strong]
                        files,
                        move |_, response| {
                            if response == "down" {
                                run_then_reload(docker::compose_down_command(&files));
                            }
                        }
                    ),
                );
                dialog.present(Some(&area.overlay));
            }
        ));
        row.add_suffix(&down);
    } else {
        let up = gtk::Button::from_icon_name("media-playback-start-symbolic");
        up.set_tooltip_text(Some("Starta projektet (up -d)"));
        up.set_valign(gtk::Align::Center);
        up.add_css_class("flat");
        up.connect_clicked({
            let run_then_reload = run_then_reload.clone();
            let files = files.clone();
            move |_| run_then_reload(docker::compose_up_command(&files))
        });
        row.add_suffix(&up);
    }

    row
}

/// Rådata → (rubrik, underrubrik, referens att ta bort med).
///
/// GTK-fri och därför testbar: hela skillnaden mellan de tre kategorierna
/// bor här, inte i widgetkoden. `None` som tredje fält betyder "erbjud
/// ingen borttagning" — Dockers egna nätverk går inte att ta bort.
fn docker_category_rows(
    output: &str,
    category: DockerCategory,
) -> Vec<(String, String, Option<String>)> {
    match category {
        DockerCategory::Images => docker::parse_images(output)
            .into_iter()
            .map(|i| {
                let title = if i.is_dangling() {
                    format!("<none> ({})", i.id)
                } else {
                    format!("{}:{}", i.repository, i.tag)
                };
                let subtitle = if i.is_dangling() {
                    format!("{} · dinglande, inget pekar på den", i.size)
                } else {
                    format!("{} · {}", i.size, i.id)
                };
                (title, subtitle, Some(i.reference()))
            })
            .collect(),
        DockerCategory::Volumes => docker::parse_volumes(output)
            .into_iter()
            .map(|v| (v.name.clone(), format!("drivrutin: {}", v.driver), Some(v.name)))
            .collect(),
        DockerCategory::Networks => docker::parse_networks(output)
            .into_iter()
            .map(|n| {
                let subtitle = format!("{} · {}", n.driver, n.scope);
                let removable = if n.is_builtin() { None } else { Some(n.name.clone()) };
                (n.name, subtitle, removable)
            })
            .collect(),
        // Compose ritas av `build_compose_row`, men den här funktionen
        // avgör fortfarande om listan är TOM — och den frågan måste
        // besvaras för alla kategorier, annars visas "Inga Compose-projekt"
        // ovanför projekt som finns.
        DockerCategory::Compose => docker::parse_compose_projects(output)
            .into_iter()
            .map(|p| (p.name, p.status, None))
            .collect(),
    }
}

fn docker_category_list() -> gtk::ListBox {
    gtk::ListBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .css_classes(["boxed-list"])
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build()
}

fn docker_category_scroller(list: &gtk::ListBox) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder().child(list).vexpand(true).build()
}

/// Öppnar en "Systemöversikt"-flik för `host`: EN kombinerad SSH-round-trip
/// (`dashboard::COMMAND`) ger last/minne/disk/uptime/OS/Docker i ett svep —
/// motsvarar `App/DashboardView.swift`. Ingen auto-poll (Swift-sidans
/// `DashboardModel.startPolling()`, 15 s) än, se ROADMAP "Kvar" — bara
/// manuell uppdatering via knappen.
fn open_dashboard_view(area: &Rc<SessionArea>, host: host::Host, password: Option<String>, jump: Option<host::Host>) {
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
    let toolbar = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .margin_start(12)
        .margin_end(12)
        .margin_top(8)
        .build();
    toolbar.append(
        &gtk::Label::builder()
            .label(format!("Systemöversikt: {}", host.alias))
            .hexpand(true)
            .halign(gtk::Align::Start)
            .build(),
    );
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Översikt: {}", host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    refresh_button.connect_clicked(clone!(
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        #[weak]
        list,
        move |_| refresh_dashboard(host.clone(), password.clone(), &list, jump.clone())
    ));

    refresh_dashboard(host.clone(), password.clone(), &list, jump.clone());

    // Auto-poll var 15:e sekund, samma intervall som Swift-sidans
    // `DashboardModel.startPolling()` — hämtar direkt (ovan), sedan om och
    // om igen tills fliken stängs. `refresh_dashboard_once` AWAITAS direkt
    // i stället för att (som knappens `refresh_dashboard` gör) eldas iväg
    // som en egen fristående task: `ssh::COMMAND_TIMEOUT` är 30 s, längre
    // än 15 s-intervallet, så en fire-and-forget-variant hade kunnat
    // trigga en ANDRA, överlappande uppdatering mot samma `ListBox` om en
    // anslutning var ovanligt långsam (rensa/fylla-race, synligt som
    // flimmer eller en halvfärdig lista). Genom att invänta varje
    // uppdatering blir intervallet i värsta fall hämtningstid+15 s i
    // stället — aldrig kortare, aldrig överlappande.
    //
    // OBS: `clone!`s `#[weak]`-uppgradering sker bara EN gång, vid start
    // av `async move`-blocket (page/list är garanterat levande då, precis
    // skapade) — INTE per loop-varv. Det faktiska stoppvillkoret är
    // därför uteslutande `tab_view_contains` nedan: en levande
    // fråga mot flikvyns widget-träd, oberoende av hur många Rust-
    // referenser till `page` som råkar finnas kvar — så fliken må hållas
    // vid liv några extra sekunder efter stängning (ofarligt), men loopen
    // upptäcker och avslutar sig pålitligt. Kollas EFTER en (ev.
    // långsam) uppdatering också, inte bara innan — fliken kan ha
    // stängts medan den pågick.
    glib::spawn_future_local(clone!(
        #[strong]
        area,
        #[weak]
        page,
        #[weak]
        list,
        #[strong]
        host,
        #[strong]
        password,
        #[strong]
        jump,
        async move {
            loop {
                glib::timeout_future_seconds(15).await;
                if !tab_view_contains(&area, &page) {
                    break;
                }
                refresh_dashboard_once(host.clone(), password.clone(), &list, jump.clone()).await;
                if !tab_view_contains(&area, &page) {
                    break;
                }
            }
        }
    ));
}

/// Kärnan i en uppdatering — rensar listan, hämtar, fyller i den igen.
/// Delas av knappens `refresh_dashboard` (fire-and-forget, en egen task)
/// och auto-pollningens loop (som AWAITAR den här direkt, se dess
/// kommentar om varför) — samma logik, olika körningssätt.
async fn refresh_dashboard_once(host: host::Host, password: Option<String>, list: &gtk::ListBox, jump: Option<host::Host>) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
    let rx = ssh::run_command(host, password, dashboard::COMMAND.to_string(), jump);
    match rx.recv().await {
        Ok(Ok(output)) => {
            for row in build_dashboard_rows(&dashboard::parse(&output)) {
                list.append(&row);
            }
        }
        Ok(Err(e)) => list.append(&error_row(&e)),
        Err(_) => list.append(&error_row("SSH-anslutningen avbröts oväntat")),
    }
}

fn refresh_dashboard(host: host::Host, password: Option<String>, list: &gtk::ListBox, jump: Option<host::Host>) {
    glib::spawn_future_local(clone!(
        #[weak]
        list,
        async move { refresh_dashboard_once(host, password, &list, jump).await }
    ));
}

/// Formaterar en `SystemSnapshot` som en rad `adw::ActionRow`-poster —
/// sammanfattning, drifttid, last, minne, en rad per disk, en rad per
/// Docker-container (med en play/stop-ikon efter `is_running()`, samma
/// signal som Swift-sidans gröna/röda statuspunkt).
fn build_dashboard_rows(snap: &dashboard::SystemSnapshot) -> Vec<adw::ActionRow> {
    let mut rows = Vec::new();

    let mut summary = Vec::new();
    if let Some(os) = &snap.os {
        summary.push(os.clone());
    }
    if let Some(kernel) = &snap.kernel {
        summary.push(kernel.clone());
    }
    if let Some(cpu) = snap.cpu_count {
        summary.push(format!("{cpu} kärnor"));
    }
    rows.push(
        adw::ActionRow::builder()
            .title(snap.hostname.clone().unwrap_or_else(|| "Värd".to_string()))
            .subtitle(if summary.is_empty() { "Ingen systemdata".to_string() } else { summary.join(" · ") })
            .build(),
    );

    if let Some(uptime) = snap.uptime_seconds {
        rows.push(adw::ActionRow::builder().title("Drifttid").subtitle(format_uptime(uptime)).build());
    }
    if let Some(load) = snap.load {
        rows.push(
            adw::ActionRow::builder()
                .title("Systemlast")
                .subtitle(format!("{:.2} / {:.2} / {:.2} (1/5/15 min)", load.one, load.five, load.fifteen))
                .build(),
        );
    }
    if let Some(mem) = snap.memory {
        rows.push(
            adw::ActionRow::builder()
                .title("Minne")
                .subtitle(format!(
                    "{} / {} ({:.0} %)",
                    format_bytes(mem.used_bytes()),
                    format_bytes(mem.total_bytes),
                    mem.used_fraction() * 100.0
                ))
                .build(),
        );
    }
    for disk in &snap.disks {
        rows.push(
            adw::ActionRow::builder()
                .title(disk.mount.clone())
                .subtitle(format!(
                    "{} / {} ({} %) — {}",
                    format_bytes(disk.used_bytes),
                    format_bytes(disk.size_bytes),
                    disk.capacity_percent,
                    disk.filesystem
                ))
                .build(),
        );
    }
    for c in &snap.containers {
        let row = adw::ActionRow::builder().title(c.name.clone()).subtitle(format!("{} — {}", c.image, c.status)).build();
        let icon_name = if c.is_running() { "media-playback-start-symbolic" } else { "media-playback-stop-symbolic" };
        row.add_prefix(&gtk::Image::from_icon_name(icon_name));
        rows.push(row);
    }
    rows
}

fn format_bytes(bytes: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes.max(0) as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

fn format_uptime(seconds: f64) -> String {
    let total = seconds.max(0.0) as u64;
    let days = total / 86400;
    let hours = (total % 86400) / 3600;
    let minutes = (total % 3600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
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
                    start_session(&area, shell_host, password.clone(), jump.clone(), SessionTarget::NewTab);
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
    show_command_output(
        area,
        host,
        password,
        jump,
        &format!("Loggar: {}", container.name),
        cmd,
    );
}

/// Kör ett kommando på värden och visar utdatan i ett skrollbart fönster.
///
/// Bröts ut ur `show_docker_logs` när Compose behövde exakt samma sak för
/// `docker compose logs`. Två kopior av "kör, vänta, skriv i en buffert"
/// hade drivit isär vid första ändringen av felhanteringen — och felen är
/// just det som skiljer en användbar loggvy från en tom ruta.
fn show_command_output(
    area: &Rc<SessionArea>,
    host: &host::Host,
    password: &Option<String>,
    jump: &Option<host::Host>,
    title: &str,
    command: String,
) {
    let text_view = gtk::TextView::builder()
        .editable(false)
        .monospace(true)
        .build();
    let scrolled = gtk::ScrolledWindow::builder()
        .child(&text_view)
        .vexpand(true)
        .build();
    let win = dialog_window(&session_window(area), title, DialogSize::Viewer, &scrolled);
    win.present();

    let rx = ssh::run_command(host.clone(), password.clone(), command, jump.clone());
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
    start_session(area, h, password, jump, SessionTarget::NewTab);
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

    let win = dialog_window(&session_window(area), "Fyll i kommandot", DialogSize::Form, &content);

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

    let win = dialog_window(&session_window(area), "Snippet", DialogSize::Form, &content);

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
    /// Namnen som är ikryssade i den AKTUELLA katalogen. BTreeSet, inte
    /// HashSet: ordningen blir då densamma varje gång, så `tar`-kommandot
    /// (och därmed arkivet) är reproducerbart. Töms vid katalogbyte —
    /// namnen är relativa till katalogen och betyder inget utanför den.
    selection: Rc<RefCell<std::collections::BTreeSet<String>>>,
    /// Knappen som packar markeringen. Ligger i kontexten och inte som
    /// parameter enbart för att kontexten redan når varenda kodväg som kan
    /// ändra markeringen (radbyggare, ta bort, byt namn, katalogbyte) —
    /// alternativet vore en extra parameter genom tio anropsställen.
    compress_button: gtk::Button,
}

impl SftpContext {
    /// Ett enda ställe som håller mängden och knappen i synk. Knappen ska
    /// vara tryckbar exakt när det finns något att packa.
    fn sync_compress_button(&self) {
        let any = !self.selection.borrow().is_empty();
        self.compress_button.set_sensitive(any);
    }

    fn clear_selection(&self) {
        self.selection.borrow_mut().clear();
        self.sync_compress_button();
    }

    fn set_selected(&self, name: &str, selected: bool) {
        if selected {
            self.selection.borrow_mut().insert(name.to_string());
        } else {
            self.selection.borrow_mut().remove(name);
        }
        self.sync_compress_button();
    }
}

fn open_sftp_view(
    area: &Rc<SessionArea>,
    host: host::Host,
    password: Option<String>,
    jump: Option<host::Host>,
) {
    let handle = sftp::spawn(host.clone(), password.clone(), jump.clone());
    // Komprimerar de MARKERADE filerna, till skillnad från mappknappen på
    // varje mapprad som packar hela mappen. Avstängd tills något är
    // markerat — en knapp som inte kan göra något ska inte gå att trycka på.
    let compress_selected_button = gtk::Button::from_icon_name("package-x-generic-symbolic");
    compress_selected_button.set_tooltip_text(Some("Komprimera markerade"));
    compress_selected_button.set_sensitive(false);
    let ctx = SftpContext {
        handle,
        host,
        password,
        jump,
        selection: Rc::new(RefCell::new(std::collections::BTreeSet::new())),
        compress_button: compress_selected_button.clone(),
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
    let symlink_button = gtk::Button::from_icon_name("emblem-symbolic-link");
    symlink_button.set_tooltip_text(Some("Ny symbolisk länk"));
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
    toolbar.append(&compress_selected_button);
    toolbar.append(&mkdir_button);
    toolbar.append(&symlink_button);
    toolbar.append(&refresh_button);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&toolbar);
    content.append(&scrolled);

    let page = area.tab_view.append(&content);
    page.set_title(&format!("Filer: {}", ctx.host.alias));
    area.tab_view.set_selected_page(&page);
    area.update_placeholder();

    compress_selected_button.connect_clicked(clone!(
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
            let names: Vec<String> = ctx.selection.borrow().iter().cloned().collect();
            if names.is_empty() {
                return;
            }
            let dir = current_path.borrow().clone();
            // Tidsstämplat namn: markeringen kan vara godtyckligt lång, och
            // ett fast namn hade skrivit över ett tidigare arkiv tyst.
            let archive_name = archive::multi_selection_archive_name(
                &chrono::Local::now().format("%Y%m%d-%H%M%S").to_string(),
            );
            let command = archive::create_tar_gz_command(&names, &archive_name, &dir);
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
                async move {
                    match rx.recv().await {
                        Ok(Err(e)) => list.append(&error_row(&e)),
                        // Arkivet är en ny fil i katalogen — listan måste
                        // läsas om för att den ska synas, och omläsningen
                        // nollställer markeringen (se refresh_sftp_list).
                        _ => refresh_sftp_list(&area, ctx, current_path.clone(), dir, &list, &path_label),
                    }
                }
            ));
        }
    ));


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

    symlink_button.connect_clicked(clone!(
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
        move |_| prompt_new_symlink(
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
    // Markeringen är relativ till katalogen vi lämnar — den får inte följa
    // med till nästa, där samma namn kan betyda en helt annan fil (eller
    // ingen alls). Knappen slocknar med den.
    ctx.clear_selection();
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

use crate::sftp::joined_path;


fn build_sftp_entry_row(
    area: &Rc<SessionArea>,
    ctx: SftpContext,
    current_path: Rc<RefCell<String>>,
    path: String,
    entry: sftp::Entry,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) -> adw::ActionRow {
    // En symbolisk länk beskrivs av vad den PEKAR PÅ, inte av sin egen
    // storlek — en länk är några tiotal byte oavsett vad som ligger i
    // andra änden, så "41 bytes" hade varit sant och samtidigt
    // vilseledande. Pilen är samma konvention som `ls -l`.
    let subtitle = match (&entry.link_target, entry.is_dir) {
        (Some(target), true) => format!("→ {target} (mapp)"),
        (Some(target), false) => format!("→ {target}"),
        (None, true) => "Mapp".to_string(),
        (None, false) => format!("{} bytes", entry.size),
    };
    let row = adw::ActionRow::builder()
        .title(&entry.name)
        .subtitle(subtitle)
        .activatable(true)
        .build();
    // Kryssrutan sitter FÖRE ikonen och markerar raden för komprimering.
    // `set_activates_default(false)` + egen toggle-hanterare: ett klick i
    // rutan ska bara ändra markeringen, inte råka aktivera raden (som
    // navigerar in i mappar respektive öppnar filer).
    let select_check = gtk::CheckButton::new();
    select_check.set_valign(gtk::Align::Center);
    select_check.set_tooltip_text(Some("Markera för komprimering"));
    select_check.set_active(ctx.selection.borrow().contains(&entry.name));
    select_check.connect_toggled(clone!(
        #[strong]
        ctx,
        #[strong(rename_to = name)]
        entry.name,
        move |check| ctx.set_selected(&name, check.is_active())
    ));
    row.add_prefix(&select_check);

    let icon = gtk::Image::from_icon_name(match (&entry.link_target, entry.is_dir) {
        // Egen ikon för länkar, så att de går att skilja från det de
        // pekar på utan att läsa underrubriken.
        (Some(_), _) => "emblem-symbolic-link",
        (None, true) => "folder-symbolic",
        (None, false) => "text-x-generic-symbolic",
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
            } else if sftp::looks_like_image(&entry.name) {
                open_sftp_image_preview(&area, ctx.handle.clone(), full_path, entry.size);
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

    let win = dialog_window(&session_window(area), "Ny mapp", DialogSize::Compact, &content);

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

/// Skapar en symbolisk länk i den katalog vyn står i.
///
/// Två fält, och ordningen är medveten: namnet på länken först, målet
/// sedan — samma ordning som `ln -s` skriver ut den och som raden sedan
/// visas i vyn ("namn → mål"). Att SFTP-protokollet internt skickar dem
/// tvärtom mot OpenSSH är en detalj som stannar i `sftp::symlink`.
fn prompt_new_symlink(
    area: &Rc<SessionArea>,
    ctx: SftpContext,
    current_path: Rc<RefCell<String>>,
    list: &gtk::ListBox,
    path_label: &gtk::Label,
) {
    let name_row = adw::EntryRow::builder().title("Länkens namn").build();
    let target_row = adw::EntryRow::builder().title("Pekar på (sökväg)").build();
    let group = adw::PreferencesGroup::builder()
        // Relativa mål är det vanliga och det som överlever att katalogen
        // flyttas — värt att säga, eftersom fältet annars inbjuder till
        // att klistra in en absolut sökväg.
        .description("Målet får vara relativt katalogen du står i, eller absolut. Det behöver inte finnas än.")
        .build();
    group.add(&name_row);
    group.add(&target_row);
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

    let win = dialog_window(&session_window(area), "Ny symbolisk länk", DialogSize::Form, &content);

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
            let name = name_row.text().trim().to_string();
            let target = target_row.text().trim().to_string();
            // Båda krävs. Ett tomt mål hade gett en länk som pekar på
            // ingenting alls, vilket är något annat än en trasig länk.
            if name.is_empty() || target.is_empty() {
                return;
            }
            win.close();
            let base = current_path.borrow().clone();
            let link_path = joined_path(&base, &name);
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
                    if let Err(e) = ctx.handle.symlink(link_path, target).await {
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

    let win = dialog_window(&session_window(area), "Döp om", DialogSize::Compact, &content);

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

    let win = dialog_window(
        &session_window(area),
        "Rättigheter/ägare",
        DialogSize::Form,
        &content,
    );

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

/// Visar en bildfil i stället för att öppna den i textredigeraren.
/// VISION.md listar förhandsvisning under SFTP-filhanteraren; utan den
/// hamnade en PNG i en textbuffert som binärskräp.
///
/// Storleken kontrolleras INNAN nedladdningen — `handle.read` läser hela
/// filen till minne, och katalogen kan innehålla flergigabytesfiler.
fn open_sftp_image_preview(area: &Rc<SessionArea>, handle: sftp::SftpHandle, path: String, size: u64) {
    let status = gtk::Label::builder()
        .label("Hämtar …")
        .margin_top(24)
        .margin_bottom(24)
        .wrap(true)
        .build();
    let picture = gtk::Picture::builder()
        .can_shrink(true)
        .vexpand(true)
        .hexpand(true)
        .visible(false)
        .build();
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&status);
    content.append(&picture);

    let win = dialog_window(
        &session_window(area),
        &format!("Förhandsvisning: {path}"),
        DialogSize::Viewer,
        &content,
    );
    win.present();

    if size > sftp::PREVIEW_MAX_BYTES {
        status.set_label(&format!(
            "Filen är {:.1} MB — för stor för förhandsvisning (gränsen är {} MB).\n\
             Ladda ner den i stället.",
            size as f64 / (1024.0 * 1024.0),
            sftp::PREVIEW_MAX_BYTES / (1024 * 1024)
        ));
        return;
    }

    glib::spawn_future_local(clone!(
        #[strong]
        handle,
        #[strong]
        path,
        #[weak]
        status,
        #[weak]
        picture,
        async move {
            match handle.read(path).await {
                Ok(bytes) => {
                    // GDK avgör själv formatet ur innehållet. Att det
                    // misslyckas är helt normalt — filnamnet gissade bara
                    // att det VAR en bild — så felet visas, inget kraschar.
                    match gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(bytes)) {
                        Ok(texture) => {
                            picture.set_paintable(Some(&texture));
                            picture.set_visible(true);
                            status.set_visible(false);
                        }
                        Err(e) => status.set_label(&format!("Kunde inte tolka filen som en bild: {e}")),
                    }
                }
                Err(e) => status.set_label(&format!("Kunde inte hämta filen: {e}")),
            }
        }
    ));
}

/// Läser filen och visar den redigerbar om innehållet är giltig UTF-8 —
/// annars en tydlig platshållartext (samma "spara MÅSTE vara avstängt för
/// binärt innehåll"-lärdom som Swiftsidans `EditingFile.isBinary`).
fn open_sftp_file_editor(area: &Rc<SessionArea>, handle: sftp::SftpHandle, path: String) {
    // GtkSourceView i stället för GtkTextView: samma widget-kontrakt (den
    // ÄRVER TextView, så buffert-API:t nedan är oförändrat) men med
    // syntax highlighting, radnummer och parentesmatchning. VISION.md
    // "Editor" listar YAML, JSON, Docker Compose, Bash, Python, Go, Rust,
    // JavaScript och Markdown — samtliga ingår i GtkSourceViews egna
    // språkdefinitioner, så inget eget lexer-arbete behövs.
    // `as _` — traiten behövs för metoderna nedan men får inte dra in sitt
    // namn i scope: gtk-preluden har egna `set_language`/`set_style_scheme`
    // på andra typer, och en namngiven import gör anropen tvetydiga.
    use sourceview5::prelude::BufferExt as _;

    let buffer = sourceview5::Buffer::new(None);
    // Språket gissas från filnamnet. `guess_language` klarar både ändelser
    // och kända filnamn utan ändelse (Dockerfile, Makefile), vilket är
    // precis vad en fjärrkatalog är full av. Ingen träff = ingen
    // highlighting, aldrig fel highlighting.
    let language = sourceview5::LanguageManager::default().guess_language(Some(&path), None);
    buffer.set_language(language.as_ref());
    // Följer appens ljusa/mörka läge i stället för att låsa ett tema —
    // annars blir editorn ljus i en mörk app (eller tvärtom).
    let scheme_name = if adw::StyleManager::default().is_dark() { "Adwaita-dark" } else { "Adwaita" };
    if let Some(scheme) = sourceview5::StyleSchemeManager::default().scheme(scheme_name) {
        buffer.set_style_scheme(Some(&scheme));
    }
    let text_view = sourceview5::View::builder()
        .buffer(&buffer)
        .monospace(true)
        .show_line_numbers(true)
        .highlight_current_line(true)
        .auto_indent(true)
        .build();
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

    let win = dialog_window(
        &session_window(area),
        &format!("Redigerar {path}"),
        DialogSize::Viewer,
        &content,
    );
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

#[cfg(test)]
mod docker_category_tests {
    use super::*;

    /// Hela skillnaden mellan de tre kategorierna bor i den här
    /// funktionen, så det är den som är värd att testa — widgetkoden
    /// däromkring gör bara samma sak med olika strängar.
    #[test]
    fn images_show_size_and_id_and_can_be_removed_by_reference() {
        let rows = docker_category_rows("sha1|nginx|1.27|54MB", DockerCategory::Images);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "nginx:1.27");
        assert_eq!(rows[0].1, "54MB · sha1");
        assert_eq!(rows[0].2.as_deref(), Some("nginx:1.27"));
    }

    /// En dinglande image har inget namn att visa eller referera med.
    /// Raden ska säga VARFÖR den ser konstig ut, inte bara visa `<none>`.
    #[test]
    fn dangling_images_are_labelled_and_removed_by_id() {
        let rows = docker_category_rows("sha9|<none>|<none>|142MB", DockerCategory::Images);
        assert_eq!(rows[0].0, "<none> (sha9)");
        assert!(rows[0].1.contains("dinglande"), "skälet ska stå i raden: {}", rows[0].1);
        assert_eq!(rows[0].2.as_deref(), Some("sha9"), "utan namn är id:t enda referensen");
    }

    /// Dockers egna nätverk går inte att ta bort. `None` betyder att
    /// raden inte ens erbjuder knappen — bättre än en knapp som alltid
    /// ger ett felmeddelande.
    #[test]
    fn builtin_networks_offer_no_removal_but_custom_ones_do() {
        let rows = docker_category_rows(
            "n1|bridge|bridge|local\nn2|mitt-nat|bridge|local",
            DockerCategory::Networks,
        );
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "bridge");
        assert!(rows[0].2.is_none(), "bridge ska inte gå att ta bort");
        assert_eq!(rows[1].2.as_deref(), Some("mitt-nat"));
    }

    #[test]
    fn volumes_are_removed_by_name_and_show_their_driver() {
        let rows = docker_category_rows("data|local", DockerCategory::Volumes);
        assert_eq!(rows[0].0, "data");
        assert_eq!(rows[0].1, "drivrutin: local");
        assert_eq!(rows[0].2.as_deref(), Some("data"));
    }

    /// Tom utdata ska ge noll rader, inte en tom post. Vyn skiljer sedan
    /// "inget finns" från "det gick fel" på just den skillnaden.
    /// Compose ritas av `build_compose_row`, men `docker_category_rows`
    /// avgör fortfarande om listan är tom. Missas den frågan visas
    /// "Inga Compose-projekt" ovanför projekt som faktiskt finns.
    #[test]
    fn compose_rows_are_counted_so_the_empty_state_is_not_shown_over_real_projects() {
        let out = r#"[{"Name":"webb","Status":"running(3)","ConfigFiles":"/srv/c.yml"}]"#;
        let rows = docker_category_rows(out, DockerCategory::Compose);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].0, "webb");
        assert_eq!(rows[0].1, "running(3)");
        assert!(rows[0].2.is_none(), "ett projekt tas inte bort — det stoppas");
    }

    #[test]
    fn empty_output_yields_no_rows_for_any_category() {
        for category in [
            DockerCategory::Images,
            DockerCategory::Volumes,
            DockerCategory::Networks,
            DockerCategory::Compose,
        ] {
            assert!(docker_category_rows("", category).is_empty(), "{category:?}");
            assert!(docker_category_rows("\n\n", category).is_empty(), "{category:?}");
        }
    }
}

#[cfg(test)]
mod kubernetes_row_tests {
    use super::*;

    fn ns(name: &str) -> kubernetes::Namespace {
        kubernetes::Namespace::Named(name.to_string())
    }

    /// Poängen med vyn: en podd i `Running` med 1/3 klara containrar ska
    /// se problematisk ut, inte lugnande. Utan varningen är den visuellt
    /// omöjlig att skilja från en frisk.
    #[test]
    fn unhealthy_pods_are_marked_while_healthy_ones_are_not() {
        let out = "web-1  3/3  Running  0  1d\nweb-2  1/3  Running  7  1d";
        let rows = kubernetes_category_rows(out, KubernetesCategory::Pods, &ns("prod"));
        assert_eq!(rows.len(), 2);

        assert_eq!(rows[0].0, "prod/web-1");
        assert!(!rows[0].1.contains('⚠'), "en frisk podd ska inte varna: {}", rows[0].1);
        assert!(!rows[0].1.contains("omstarter"), "noll omstarter är inte värt en rad");

        assert!(rows[1].1.contains('⚠'), "1/3 klara i Running måste varna");
        assert!(rows[1].1.contains("7 omstarter"), "omstarterna är det som förklarar varför");
    }

    /// Noder är kluster-globala och har inga åtgärder — `None` betyder
    /// att raden inte får några knappar alls.
    #[test]
    fn nodes_have_no_actions_but_do_report_cordon_and_readiness() {
        let out = "\
node-a  Ready                     control-plane  30d  v1.31.2
node-b  Ready,SchedulingDisabled  <none>         30d  v1.31.2
node-c  NotReady                  <none>         30d  v1.30.8";
        let rows = kubernetes_category_rows(out, KubernetesCategory::Nodes, &kubernetes::Namespace::All);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().all(|r| r.2.is_none()), "noder ska inte ha åtgärder");

        assert_eq!(rows[0].1, "v1.31.2");
        assert!(rows[1].1.contains("avstängd"), "cordon ska synas: {}", rows[1].1);
        assert!(!rows[1].1.contains("inte redo"), "avstängd är inte samma sak som trasig");
        assert!(rows[2].1.contains("inte redo"));
    }

    /// Åtgärderna behöver namnrymd OCH namn. Med `--all-namespaces` kommer
    /// namnrymden från utdatan, annars från frågan — och båda vägarna
    /// måste ge en användbar referens.
    #[test]
    fn actions_carry_namespace_from_output_or_from_the_query() {
        let rows = kubernetes_category_rows(
            "kube-system  coredns-abc  1/1  Running  0  5d",
            KubernetesCategory::Pods,
            &kubernetes::Namespace::All,
        );
        assert_eq!(rows[0].2, Some(("kube-system".to_string(), "coredns-abc".to_string())));

        let rows = kubernetes_category_rows(
            "coredns-abc  1/1  Running  0  5d",
            KubernetesCategory::Pods,
            &ns("kube-system"),
        );
        assert_eq!(rows[0].2, Some(("kube-system".to_string(), "coredns-abc".to_string())));
    }

    #[test]
    fn partially_available_deployments_are_marked() {
        let rows = kubernetes_category_rows(
            "web  2/3  3  2  5d\napi  4/4  4  4  5d",
            KubernetesCategory::Deployments,
            &ns("prod"),
        );
        assert!(rows[0].1.contains('⚠'), "2/3 ska varna");
        assert!(!rows[1].1.contains('⚠'), "4/4 ska inte varna");
        assert_eq!(rows[1].2, Some(("prod".to_string(), "api".to_string())));
    }

    #[test]
    fn empty_output_yields_no_rows_for_any_category() {
        for c in [KubernetesCategory::Pods, KubernetesCategory::Deployments, KubernetesCategory::Nodes] {
            assert!(kubernetes_category_rows("", c, &ns("default")).is_empty(), "{c:?}");
            assert!(kubernetes_category_rows("\n\n", c, &ns("default")).is_empty(), "{c:?}");
        }
    }
}
