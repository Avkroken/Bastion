use adw::prelude::*;
use gtk::glib;

const APP_ID: &str = "se.denied.bastion";

fn main() -> glib::ExitCode {
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &adw::Application) {
    let header = adw::HeaderBar::new();

    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.append(&header);
    content.append(&gtk::Label::new(Some("Bastion")));

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Bastion")
        .default_width(800)
        .default_height(600)
        .content(&content)
        .build();

    window.present();
}
