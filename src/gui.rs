use aula_f87pro::discovery::{self, Transport};
use gtk::{glib, prelude::*};
use std::{cell::Cell, rc::Rc};
#[path = "gui/assignments.rs"]
mod assignments;
#[path = "gui/editor.rs"]
mod editor;
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
    let e = editor.clone();
    let may_close = closing.clone();
    window.connect_close_request(move |window| {
        if may_close.get() { return glib::Propagation::Proceed; }
        if e.busy.get() { e.message("Aguarde a operação terminar antes de fechar."); return glib::Propagation::Stop; }
        if e.has_unsaved() {
            let dialog = gtk::AlertDialog::builder().message("Há alterações ainda não aplicadas")
                .detail("Você pode continuar editando e salvar um perfil, ou descartar as edições ao fechar.")
                .buttons(["Continuar editando", "Descartar e fechar"]).default_button(0).cancel_button(0).build();
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
    app.connect_activate(build);
    app.run()
}
