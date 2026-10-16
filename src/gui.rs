use aula_f87pro::discovery::{self, Transport};
use gtk::{gio, glib, prelude::*};
use i18n::t;
use std::{cell::Cell, rc::Rc};
#[path = "gui/app_settings.rs"]
mod app_settings;
#[path = "gui/assignments.rs"]
mod assignments;
#[path = "gui/editor.rs"]
mod editor;
#[path = "gui/i18n.rs"]
mod i18n;
#[path = "gui/keyboard.rs"]
mod keyboard;
#[path = "gui/lighting_preview.rs"]
mod lighting_preview;
#[path = "gui/macro_ui.rs"]
mod macro_ui;
#[path = "gui/profiles.rs"]
mod profiles;
#[path = "gui/session.rs"]
mod session;
#[path = "gui/settings_ui.rs"]
mod settings_ui;

fn connection() -> Option<Transport> {
    discovery::connections()
        .ok()?
        .into_iter()
        .map(|c| c.transport)
        .min_by_key(|transport| *transport != Transport::UsbCable)
}

fn build(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("AULA F87 Pro")
        .default_width(1080)
        .default_height(900)
        .build();
    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("AULA F87 Pro · Linux"))));
    window.set_titlebar(Some(&header));
    let editor = editor::Editor::new(&window);
    window.set_child(Some(&editor.root));
    let closing = Rc::new(Cell::new(false));
    let explicit_quit = Rc::new(Cell::new(false));
    let quit = gio::SimpleAction::new("quit", None);
    let requested = explicit_quit.clone();
    let parent = window.clone();
    quit.connect_activate(move |_, _| {
        requested.set(true);
        parent.close();
    });
    app.add_action(&quit);
    app.set_accels_for_action("app.quit", &["<Primary>q"]);
    let menu = gio::Menu::new();
    menu.append(
        Some(t("Encerrar aplicativo", "Quit application")),
        Some("app.quit"),
    );
    let menu_button = gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text(t("Menu do aplicativo", "Application menu"))
        .build();
    header.pack_end(&menu_button);
    let e = editor.clone();
    let may_close = closing.clone();
    window.connect_close_request(move |window| {
        if may_close.get() { return glib::Propagation::Proceed; }
        if !explicit_quit.replace(false) && e.preferences.borrow().minimize_on_close {
            window.minimize();
            return glib::Propagation::Stop;
        }
        if e.busy.get() { e.message(t("Aguarde a operação terminar antes de fechar.", "Wait for the operation to finish before closing.")); return glib::Propagation::Stop; }
        if e.has_unsaved() {
            let dialog = gtk::AlertDialog::builder().message(t("Há alterações ainda não aplicadas", "There are unapplied changes"))
                .detail(t("Você pode continuar editando e salvar um perfil, ou descartar as edições ao fechar.", "You can keep editing and save a profile, or discard the edits when closing."))
                .buttons([t("Continuar editando", "Keep editing"), t("Descartar e fechar", "Discard and close")]).default_button(0).cancel_button(0).build();
            let parent = window.clone(); let closing = may_close.clone();
            glib::MainContext::default().spawn_local(async move {
                if dialog.choose_future(Some(&parent)).await == Ok(1) { closing.set(true); parent.close(); }
            });
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.present();
    let initial = connection();
    editor.set_connection(initial);
    let last = Cell::new(initial);
    let retry = Cell::new(std::time::Instant::now());
    let weak = Rc::downgrade(&editor);
    glib::timeout_add_seconds_local(2, move || {
        let Some(editor) = weak.upgrade() else {
            return glib::ControlFlow::Break;
        };
        if !editor.window.is_visible() {
            return glib::ControlFlow::Break;
        }
        let current = connection();
        let reconnect = current == Some(Transport::Receiver24G)
            && !editor.session.borrow().connected
            && retry.get().elapsed() >= std::time::Duration::from_secs(15);
        if (current != last.get() || reconnect) && !editor.busy.get() {
            last.set(current);
            retry.set(std::time::Instant::now());
            editor.set_connection(current);
        }
        glib::ControlFlow::Continue
    });
}

fn main() -> glib::ExitCode {
    let preferences =
        app_settings::Preferences::load(&glib::user_config_dir().join("aula-f87pro/settings.json"))
            .unwrap_or_default();
    i18n::init(preferences.english);
    let app = gtk::Application::builder()
        .application_id("io.github.aula_f87pro.Control")
        .build();
    app.connect_startup(|_| {
        let css = gtk::CssProvider::new();
        css.load_from_data(include_str!("gui/style.css"));
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().expect("display"),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    app.connect_activate(|app| {
        if let Some(window) = app.active_window() {
            window.present();
        } else {
            build(app);
        }
    });
    app.run()
}
