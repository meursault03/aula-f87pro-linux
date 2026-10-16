use super::{
    app_settings,
    editor::{Editor, section, text},
    i18n::t,
};
use aula_f87pro::{
    device_settings::{Controls, SLEEP_OPTIONS},
    discovery::{self, Transport},
    state::Snapshot,
};
use gtk::{glib, prelude::*};
use std::{
    cell::Cell,
    rc::{Rc, Weak},
};

pub struct Settings {
    pub widget: gtk::Box,
    editor: Weak<Editor>,
    syncing: Cell<bool>,
    device: gtk::Box,
    firmware: gtk::Label,
    connection: gtk::Label,
    note: gtk::Label,
    debounce: gtk::DropDown,
    sleep: gtk::DropDown,
    tap: gtk::CheckButton,
    sensitivity: gtk::Scale,
}

fn row(parent: &gtk::Box, title: &str, description: &str, control: &impl IsA<gtk::Widget>) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
    labels.set_hexpand(true);
    let label = text(title, "setting-label");
    labels.append(&label);
    labels.append(&text(description, "muted"));
    control.set_valign(gtk::Align::Center);
    control
        .as_ref()
        .update_property(&[gtk::accessible::Property::Label(title)]);
    row.append(&labels);
    row.append(control);
    parent.append(&row);
}

impl Settings {
    pub fn new(editor: &Rc<Editor>) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 16);
        let sleep_labels: Vec<String> = std::iter::once(t("Desligada", "Disabled").to_owned())
            .chain(SLEEP_OPTIONS.map(|seconds| format!("{seconds} s")))
            .collect();
        let settings = Rc::new(Self {
            widget,
            editor: Rc::downgrade(editor),
            syncing: Cell::new(false),
            device: section(
                t("Comportamento do teclado", "Keyboard behavior"),
                t(
                    "Edite os ajustes e use Aplicar alterações. O backup e Desfazer incluem estes valores.",
                    "Edit these settings, then use Apply changes. Backups and Undo include these values.",
                ),
            ),
            firmware: text("—", "setting-value"),
            connection: text("—", "muted"),
            note: text("", "muted"),
            debounce: gtk::DropDown::from_strings(&[
                t("Estágio 1", "Stage 1"),
                t("Estágio 2", "Stage 2"),
                t("Estágio 3", "Stage 3"),
                t("Estágio 4", "Stage 4"),
                t("Estágio 5", "Stage 5"),
            ]),
            sleep: gtk::DropDown::from_strings(
                &sleep_labels.iter().map(String::as_str).collect::<Vec<_>>(),
            ),
            tap: gtk::CheckButton::with_label(t("Ativar Tap", "Enable Tap")),
            sensitivity: gtk::Scale::with_range(gtk::Orientation::Horizontal, 10.0, 127.0, 1.0),
        });
        let info = section(t("Dispositivo", "Device"), "AULA F87 Pro");
        row(
            &info,
            t(
                "Versão informada pelo dispositivo",
                "Device-reported version",
            ),
            t(
                "No modo 2,4 GHz, este valor vem do descritor do receptor.",
                "In 2.4 GHz mode, this value comes from the receiver descriptor.",
            ),
            &settings.firmware,
        );
        info.append(&settings.connection);
        row(
            &info,
            t("Versão do aplicativo", "Application version"),
            "Linux · Rust / GTK4",
            &text(env!("CARGO_PKG_VERSION"), "setting-value"),
        );
        settings.widget.append(&info);

        row(
            &settings.device,
            "Debounce",
            t(
                "Cinco estágios, como no driver original. Não representam milissegundos confirmados.",
                "Five stages, as in the original driver. These are not verified millisecond values.",
            ),
            &settings.debounce,
        );
        row(
            &settings.device,
            t("Suspensão por inatividade", "Idle sleep"),
            t(
                "Tempo configurado para o teclado entrar em suspensão.",
                "Configured timeout before the keyboard enters sleep.",
            ),
            &settings.sleep,
        );
        let tap = gtk::Box::new(gtk::Orientation::Vertical, 8);
        tap.set_width_request(200);
        tap.append(&settings.tap);
        settings.sensitivity.set_digits(0);
        settings.sensitivity.set_value(127.0);
        tap.append(&settings.sensitivity);
        row(
            &settings.device,
            t("Tap · experimental", "Tap · experimental"),
            t(
                "Sensibilidade do driver original, de 10 a 127. A unidade e o efeito físico ainda precisam de validação.",
                "Original driver sensitivity, from 10 to 127. Its unit and physical effect still need validation.",
            ),
            &tap,
        );
        settings.device.append(&settings.note);
        let reset = gtk::Button::with_label(t(
            "Restaurar estes três ajustes",
            "Restore these three settings",
        ));
        reset.set_halign(gtk::Align::Start);
        reset.set_tooltip_text(Some(t("Prepara debounce no estágio 2, Tap desligado e suspensão em 300 s. Preserva iluminação, teclas e macros.", "Stages debounce at stage 2, Tap off and sleep at 300 s. Preserves lighting, keys and macros.")));
        let weak = Rc::downgrade(editor);
        reset.connect_clicked(move |_| {
            if let Some(e) = weak.upgrade() {
                e.stage_controls(Controls::defaults());
            }
        });
        settings.device.append(&reset);
        settings.widget.append(&settings.device);
        for dropdown in [&settings.debounce, &settings.sleep] {
            let weak = Rc::downgrade(&settings);
            dropdown.connect_selected_notify(move |_| {
                if let Some(s) = weak.upgrade() {
                    s.stage();
                }
            });
        }
        let weak = Rc::downgrade(&settings);
        settings.tap.connect_toggled(move |_| {
            if let Some(s) = weak.upgrade() {
                s.stage();
            }
        });
        let weak = Rc::downgrade(&settings);
        settings.sensitivity.connect_value_changed(move |_| {
            if let Some(s) = weak.upgrade() {
                s.stage();
            }
        });
        settings.build_preferences(editor);
        let firmware = section(
            t("Firmware e recuperação", "Firmware and recovery"),
            t(
                "A atualização de firmware e a restauração completa de fábrica ainda não estão disponíveis. Para recuperar uma configuração, use Carregar backup em Perfis e backups.",
                "Firmware updates and a full factory reset are not available yet. To recover a configuration, use Load backup in Profiles and backups.",
            ),
        );
        settings.widget.append(&firmware);
        settings
    }

    fn build_preferences(&self, editor: &Rc<Editor>) {
        let page = section(
            t("Aplicativo", "Application"),
            t(
                "Estas preferências são salvas neste computador e não alteram o teclado.",
                "These preferences are saved on this computer and do not change the keyboard.",
            ),
        );
        let config = glib::user_config_dir();
        let file = config.join("aula-f87pro/settings.json");
        let prefs = *editor.preferences.borrow();
        let language = gtk::DropDown::from_strings(&["Português", "English"]);
        language.set_selected(u32::from(prefs.english));
        row(
            &page,
            t("Idioma", "Language"),
            t(
                "A mudança de idioma entra em vigor ao reabrir o aplicativo.",
                "The language change takes effect when you reopen the application.",
            ),
            &language,
        );
        let weak = Rc::downgrade(editor);
        let path = file.clone();
        language.connect_selected_notify(move |dropdown| {
            if let Some(e) = weak.upgrade() {
                let mut prefs = *e.preferences.borrow();
                prefs.english = dropdown.selected() == 1;
                match prefs.save(&path) {
                    Ok(()) => { e.preferences.replace(prefs); e.message(t("Idioma salvo. Feche e reabra o aplicativo para usar o novo idioma.", "Language saved. Close and reopen the application to use the new language.")); }
                    Err(err) => e.error(err),
                }
            }
        });
        let autostart = gtk::Switch::new();
        autostart.set_active(app_settings::autostart_enabled(&config));
        row(
            &page,
            t("Iniciar ao entrar na sessão", "Start at login"),
            t(
                "Abre o configurador ao iniciar sua sessão gráfica do Linux.",
                "Opens the configurator when your Linux desktop session starts.",
            ),
            &autostart,
        );
        let weak = Rc::downgrade(editor);
        autostart.connect_state_set(move |toggle, enabled| {
            let result = std::env::current_exe()
                .and_then(|exe| app_settings::set_autostart(&config, &exe, enabled));
            if let Some(e) = weak.upgrade() {
                match result {
                    Ok(()) => {
                        toggle.set_state(enabled);
                        e.message(t(
                            "Preferência de inicialização salva.",
                            "Startup preference saved.",
                        ));
                    }
                    Err(err) => {
                        toggle.set_active(toggle.state());
                        e.error(err);
                    }
                }
            }
            glib::Propagation::Stop
        });
        let close = gtk::DropDown::from_strings(&[
            t("Encerrar aplicativo", "Quit application"),
            t("Minimizar janela", "Minimize window"),
        ]);
        close.set_selected(u32::from(prefs.minimize_on_close));
        row(
            &page,
            t("Ao fechar a janela", "When closing the window"),
            t(
                "Ao minimizar, a janela permanece na barra de tarefas. Para sair, use Encerrar aplicativo no menu superior.",
                "When minimized, the window stays in the taskbar. To exit, use Quit application in the top menu.",
            ),
            &close,
        );
        let weak = Rc::downgrade(editor);
        close.connect_selected_notify(move |dropdown| {
            if let Some(e) = weak.upgrade() {
                let mut prefs = *e.preferences.borrow();
                prefs.minimize_on_close = dropdown.selected() == 1;
                match prefs.save(&file) {
                    Ok(()) => {
                        e.preferences.replace(prefs);
                        e.message(t(
                            "Comportamento ao fechar salvo.",
                            "Window closing preference saved.",
                        ));
                    }
                    Err(err) => e.error(err),
                }
            }
        });
        if let Err(error) = app_settings::Preferences::load(
            &glib::user_config_dir().join("aula-f87pro/settings.json"),
        ) {
            page.set_sensitive(false);
            self.widget.append(&text(
                &format!(
                    "{} {error}",
                    t(
                        "Não foi possível ler as preferências:",
                        "Could not read preferences:"
                    )
                ),
                "status",
            ));
        }
        self.widget.append(&page);
    }

    fn stage(&self) {
        if self.syncing.get() {
            return;
        }
        self.sensitivity.set_sensitive(self.tap.is_active());
        if let Some(editor) = self.editor.upgrade() {
            editor.stage_controls(Controls {
                debounce_stage: (self.debounce.selected() + 1) as u8,
                tap: self
                    .tap
                    .is_active()
                    .then_some(self.sensitivity.value() as u8),
                sleep_seconds: self
                    .sleep
                    .selected()
                    .checked_sub(1)
                    .and_then(|index| SLEEP_OPTIONS.get(index as usize).copied()),
            });
        }
    }

    pub fn sync(
        &self,
        snapshot: Option<&Snapshot>,
        transport: Option<Transport>,
        connected: bool,
        busy: bool,
    ) {
        self.syncing.set(true);
        let controls = snapshot.and_then(|s| Controls::read(&s.leds).ok());
        self.device.set_sensitive(controls.is_some() && !busy);
        if let Some(controls) = controls {
            self.debounce
                .set_selected(u32::from(controls.debounce_stage - 1));
            self.sleep.set_selected(
                controls
                    .sleep_seconds
                    .and_then(|seconds| SLEEP_OPTIONS.iter().position(|s| *s == seconds))
                    .map_or(0, |i| i as u32 + 1),
            );
            self.tap.set_active(controls.tap.is_some());
            if let Some(value) = controls.tap {
                self.sensitivity.set_value(f64::from(value));
            }
            self.sensitivity.set_sensitive(controls.tap.is_some());
            self.note.set_text(if connected {
                ""
            } else {
                t(
                    "Edições locais. Reconecte o teclado antes de aplicar.",
                    "Local edits. Reconnect the keyboard before applying.",
                )
            });
        } else {
            self.note.set_text(t("Carregue uma configuração reconhecida para editar estes ajustes. Valores desconhecidos são preservados.", "Load a recognized configuration to edit these settings. Unknown values are preserved."));
        }
        let device = discovery::connections()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| transport.is_none_or(|transport| transport == c.transport))
            .min_by_key(|c| c.transport != Transport::UsbCable);
        self.firmware.set_text(
            &device
                .as_ref()
                .and_then(|c| c.descriptor_version)
                .map(discovery::format_descriptor_version)
                .unwrap_or_else(|| t("Indisponível", "Unavailable").into()),
        );
        self.connection.set_text(
            match (connected, transport.or(device.map(|c| c.transport))) {
                (true, Some(Transport::UsbCable)) => {
                    t("USB · configuração carregada", "USB · configuration loaded")
                }
                (true, Some(Transport::Receiver24G)) => t(
                    "2,4 GHz · configuração carregada · suporte experimental",
                    "2.4 GHz · configuration loaded · experimental support",
                ),
                (_, Some(Transport::Receiver24G)) => t(
                    "Receptor detectado; teclado indisponível.",
                    "Receiver detected; keyboard unavailable.",
                ),
                (_, Some(Transport::UsbCable)) => t(
                    "USB detectado; configuração indisponível.",
                    "USB detected; configuration unavailable.",
                ),
                (_, None) => t("Nenhum teclado conectado.", "No keyboard connected."),
            },
        );
        self.syncing.set(false);
    }
}
