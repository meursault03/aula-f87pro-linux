use super::i18n::{t, tf};
use super::lighting_preview::Lighting;
use super::{assignments, keyboard, profiles, session::Session};
use aula_f87pro::{
    KEYS,
    state::{self, Snapshot},
};
use gtk::{gio, glib, prelude::*};
use serde::Deserialize;
use std::{
    cell::{Cell, RefCell},
    ffi::OsString,
    path::PathBuf,
    rc::Rc,
};

#[derive(Deserialize)]
struct Applied {
    changed: bool,
    backup: Option<String>,
    state: Snapshot,
}

pub struct Editor {
    pub root: gtk::Box,
    pub window: gtk::ApplicationWindow,
    pub session: RefCell<Session>,
    pub busy: Cell<bool>,
    pub preferences: Rc<RefCell<super::app_settings::Preferences>>,
    settings: RefCell<Option<Rc<super::settings_ui::Settings>>>,
    transport: Cell<Option<aula_f87pro::discovery::Transport>>,
    syncing: Cell<bool>,
    selected: Cell<u8>,
    controls_for: Cell<(u8, u8)>,
    content: gtk::Box,
    status: gtk::Label,
    connection: gtk::Label,
    pending: gtk::Label,
    details: gtk::TextBuffer,
    spinner: gtk::Spinner,
    apply: gtk::Button,
    undo: gtk::Button,
    discard: gtk::Button,
    reload: gtk::Button,
    layer: gtk::DropDown,
    key_name: gtk::Label,
    key_current: gtk::Label,
    key_draft: gtk::Label,
    effect: gtk::DropDown,
    brightness: gtk::Scale,
    speed: gtk::Scale,
    palette: gtk::DropDown,
    kind: gtk::DropDown,
    function: gtk::DropDown,
    modifiers: Vec<gtk::CheckButton>,
    keys: RefCell<Vec<(u8, gtk::ToggleButton)>>,
    lighting: Rc<Cell<Option<Lighting>>>,
    animate: Rc<Cell<bool>>,
    preview_caption: gtk::Label,
    pages: gtk::Stack,
    profile: gtk::DropDown,
    profile_name: gtk::Entry,
    profile_entries: RefCell<Vec<(String, PathBuf)>>,
    data: PathBuf,
    macro_editor: RefCell<Option<Rc<super::macro_ui::MacroEditor>>>,
}

pub fn text(value: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(value));
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.add_css_class(class);
    label
}

pub fn section(title: &str, description: &str) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.add_css_class("settings");
    page.append(&text(title, "section-title"));
    page.append(&text(description, "muted"));
    page
}

fn field(page: &gtk::Box, title: &str, widget: &impl IsA<gtk::Widget>) {
    let title = gtk::Label::with_mnemonic(title);
    title.set_xalign(0.0);
    title.set_mnemonic_widget(Some(widget));
    page.append(&title);
    page.append(widget);
}

fn scale() -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 4.0, 1.0);
    scale.set_digits(0);
    scale.set_draw_value(true);
    scale.set_hexpand(true);
    scale
}

impl Editor {
    pub fn new(window: &gtk::ApplicationWindow) -> Rc<Self> {
        let effects = [
            t("Desligado", "Off"),
            t("Fixo", "Static"),
            t("Respiração", "Breathing"),
            t("Arco-íris", "Rainbow"),
            t("Disparo", "Trigger"),
            t("Chuva", "Rain"),
            t("Roda arco-íris", "Rainbow wheel"),
            t("Ondas", "Waves"),
            t("Estrelas", "Stars"),
            t("Rastro", "Trail"),
            t("Fluxo", "Flow"),
            t("Corrente neon", "Neon stream"),
            t("Reação", "Reactive"),
            t("Senoide", "Sine wave"),
            t("Varredura", "Sweep"),
            t("Moinho", "Windmill"),
            t("Cascata", "Cascade"),
            t("Florescer", "Blossom"),
            t("Tempestade", "Storm"),
            t("Personalizado / outro", "Custom / other"),
        ];
        let editor = Rc::new(Self {
            root: gtk::Box::new(gtk::Orientation::Vertical, 0),
            window: window.clone(),
            session: RefCell::new(Session::default()),
            busy: Cell::new(false),
            preferences: Rc::new(RefCell::new(
                super::app_settings::Preferences::load(
                    &glib::user_config_dir().join("aula-f87pro/settings.json"),
                )
                .unwrap_or_default(),
            )),
            settings: RefCell::new(None),
            transport: Cell::new(None),
            syncing: Cell::new(false),
            selected: Cell::new(50),
            controls_for: Cell::new((255, 255)),
            content: gtk::Box::new(gtk::Orientation::Vertical, 12),
            status: text(
                t(
                    "Conecte o teclado pelo cabo USB.",
                    "Connect the keyboard using its USB cable.",
                ),
                "status",
            ),
            connection: text(
                t("Procurando teclado…", "Looking for keyboard…"),
                "connection",
            ),
            pending: text(
                t("Nenhuma alteração pendente", "No pending changes"),
                "muted",
            ),
            details: gtk::TextBuffer::new(None),
            spinner: gtk::Spinner::new(),
            apply: gtk::Button::with_label(t("Aplicar alterações", "Apply changes")),
            undo: gtk::Button::with_label(t("Desfazer última alteração", "Undo last change")),
            discard: gtk::Button::with_label(t("Descartar edições", "Discard edits")),
            reload: gtk::Button::with_label(t("Recarregar teclado", "Reload keyboard")),
            layer: gtk::DropDown::from_strings(&[t("0 — principal", "0 — main"), "1", "2", "3"]),
            key_name: text(t("Tecla A", "Key A"), "section-title"),
            key_current: text(t("Atual no teclado: —", "Current on keyboard: —"), "muted"),
            key_draft: text("", "pending-text"),
            effect: gtk::DropDown::from_strings(&effects),
            brightness: scale(),
            speed: scale(),
            palette: gtk::DropDown::from_strings(&[
                t("Vermelho", "Red"),
                t("Verde", "Green"),
                t("Azul", "Blue"),
                t("Amarelo", "Yellow"),
                "Magenta",
                t("Ciano", "Cyan"),
                t("Branco", "White"),
                t("Multicolorido", "Multicolor"),
            ]),
            kind: gtk::DropDown::from_strings(&[
                t("Tecla ou atalho", "Key or shortcut"),
                t("Multimídia", "Media"),
                t(
                    "Copiar função de outra tecla",
                    "Copy another key's function",
                ),
                t("Desativar", "Disable"),
            ]),
            function: gtk::DropDown::from_strings(&[]),
            modifiers: ["Ctrl", "Shift", "Alt", "Super"]
                .iter()
                .map(|name| gtk::CheckButton::with_label(name))
                .collect(),
            keys: RefCell::new(Vec::new()),
            lighting: Rc::new(Cell::new(None)),
            animate: Rc::new(Cell::new(true)),
            preview_caption: text(
                t(
                    "Prévia ilustrativa dos LEDs · aguardando configuração.",
                    "Illustrative LED preview · waiting for configuration.",
                ),
                "board-caption",
            ),
            pages: gtk::Stack::new(),
            profile: gtk::DropDown::from_strings(&[]),
            profile_name: gtk::Entry::builder()
                .placeholder_text(t("Ex.: Trabalho, Jogos…", "E.g. Work, Games…"))
                .max_length(80)
                .build(),
            profile_entries: RefCell::new(Vec::new()),
            data: glib::user_data_dir().join("aula-f87pro"),
            macro_editor: RefCell::new(None),
        });
        editor.build();
        editor
    }

    fn build(self: &Rc<Self>) {
        self.root.add_css_class("suite");
        self.content.add_css_class("workspace");
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let titles = gtk::Box::new(gtk::Orientation::Vertical, 4);
        titles.set_hexpand(true);
        titles.append(&text("AULA F87 Pro", "device-title"));
        titles.append(&self.connection);
        heading.append(&titles);
        heading.append(&self.spinner);
        let motion = gtk::CheckButton::with_mnemonic(t("_Animar prévia", "_Animate preview"));
        motion.set_active(true);
        motion.set_tooltip_text(Some(t(
            "Anima apenas a representação na tela, sem alterar os LEDs do teclado.",
            "Animates the on-screen preview without changing the keyboard's LEDs.",
        )));
        let animate = self.animate.clone();
        motion.connect_toggled(move |toggle| animate.set(toggle.is_active()));
        heading.append(&motion);
        heading.append(&self.reload);
        self.content.append(&heading);
        self.pages
            .set_transition_type(gtk::StackTransitionType::Crossfade);
        self.pages.set_transition_duration(140);
        self.pages.set_vhomogeneous(false);
        let switcher = gtk::StackSwitcher::new();
        switcher.set_stack(Some(&self.pages));
        switcher.set_halign(gtk::Align::Center);
        self.content.append(&switcher);
        let weak = Rc::downgrade(self);
        let board = keyboard::build(
            move |id| {
                if let Some(e) = weak.upgrade() {
                    e.selected.set(id);
                    e.pages.set_visible_child_name("keys");
                    e.refresh();
                }
            },
            self.lighting.clone(),
            self.animate.clone(),
        );
        self.keys.replace(board.keys);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&board.widget)
            .overlay_scrolling(false)
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build();
        self.content.append(&scroll);
        let caption = self.preview_caption.clone();
        self.content.append(&caption);
        let animate = self.animate.clone();
        self.pages.connect_visible_child_name_notify(move |pages| {
            let lights = pages
                .visible_child_name()
                .is_some_and(|name| name == "lights");
            motion.set_visible(lights);
            animate.set(lights && motion.is_active());
            let visible = pages
                .visible_child_name()
                .is_some_and(|name| matches!(name.as_str(), "lights" | "keys"));
            scroll.set_visible(visible);
            caption.set_visible(visible);
        });
        self.build_lights();
        self.build_keys();
        let macro_editor = super::macro_ui::MacroEditor::new(self);
        self.pages
            .add_titled(&macro_editor.widget, Some("macros"), "Macros");
        self.macro_editor.replace(Some(macro_editor));
        self.build_profiles();
        let settings = super::settings_ui::Settings::new(self);
        self.pages.add_titled(
            &settings.widget,
            Some("settings"),
            t("Configurações", "Settings"),
        );
        self.settings.replace(Some(settings));
        for (index, page) in ["lights", "keys", "macros", "profiles", "settings"]
            .iter()
            .enumerate()
        {
            let name = format!("page-{page}");
            let action = gio::SimpleAction::new(&name, None);
            let stack = self.pages.downgrade();
            let page = *page;
            action.connect_activate(move |_, _| {
                if let Some(stack) = stack.upgrade() {
                    stack.set_visible_child_name(page);
                }
            });
            self.window.add_action(&action);
            if let Some(app) = self.window.application() {
                app.set_accels_for_action(
                    &format!("win.{name}"),
                    &[&format!("<Primary>{}", index + 1)],
                );
            }
        }

        self.content.append(&self.pages);
        let view = gtk::TextView::with_buffer(&self.details);
        view.set_editable(false);
        view.set_monospace(true);
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&view)
            .min_content_height(80)
            .max_content_height(140)
            .build();
        let expander = gtk::Expander::builder()
            .label(t("Detalhes da última operação", "Last operation details"))
            .child(&scroll)
            .build();
        self.content.append(&expander);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&self.content)
            .vexpand(true)
            .overlay_scrolling(false)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .build();
        self.root.append(&scroll);
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 8);
        bar.add_css_class("action-bar");
        bar.append(&self.status);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        self.pending.set_hexpand(true);
        actions.append(&self.pending);
        actions.append(&self.undo);
        actions.append(&self.discard);
        actions.append(&self.apply);
        self.apply.add_css_class("suggested-action");
        bar.append(&actions);
        self.root.append(&bar);
        let e = self.clone();
        self.reload.connect_clicked(move |_| e.load(false));
        let e = self.clone();
        self.discard.connect_clicked(move |_| e.load(true));
        let e = self.clone();
        self.apply.connect_clicked(move |_| e.apply_draft(false));
        let e = self.clone();
        self.undo.connect_clicked(move |_| e.apply_draft(true));
        let e = self.clone();
        self.layer.connect_selected_notify(move |_| {
            if !e.syncing.get() {
                e.refresh();
            }
        });
        self.update_profiles();
        self.refresh();
    }

    fn build_lights(self: &Rc<Self>) {
        let lights = section(
            t("Iluminação", "Lighting"),
            t(
                "Efeito e parâmetros lidos do teclado. As edições ficam pendentes até aplicar.",
                "Effect and parameters read from the keyboard. Edits remain pending until you apply them.",
            ),
        );
        let selectors = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        for (title, dropdown) in [
            (t("_Efeito", "_Effect"), &self.effect),
            (t("_Cor do efeito", "Effect _color"), &self.palette),
        ] {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
            column.set_hexpand(true);
            field(&column, title, dropdown);
            selectors.append(&column);
        }
        lights.append(&selectors);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
        for (title, scale) in [
            (t("_Brilho", "_Brightness"), &self.brightness),
            (t("_Velocidade", "_Speed"), &self.speed),
        ] {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
            column.set_hexpand(true);
            field(&column, title, scale);
            row.append(&column);
        }
        lights.append(&row);
        lights.append(&text(
            t(
                "Os parâmetros são salvos por efeito. Nem todo efeito utiliza cor ou velocidade.",
                "Parameters are saved per effect. Some effects do not use color or speed.",
            ),
            "muted",
        ));
        let e = self.clone();
        self.effect
            .connect_selected_notify(move |_| e.edit_lights(true));
        let e = self.clone();
        self.brightness
            .connect_value_changed(move |_| e.edit_lights(false));
        let e = self.clone();
        self.speed
            .connect_value_changed(move |_| e.edit_lights(false));
        let e = self.clone();
        self.palette
            .connect_selected_notify(move |_| e.edit_lights(false));
        self.pages
            .add_titled(&lights, Some("lights"), t("Iluminação", "Lighting"));
    }

    fn build_keys(self: &Rc<Self>) {
        let keys = section(
            t("Atribuições", "Key assignments"),
            t(
                "Prepare quantas teclas quiser; todas as alterações serão aplicadas juntas.",
                "Prepare as many keys as you like; all changes will be applied together.",
            ),
        );
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        self.key_name.set_hexpand(true);
        row.append(&self.key_name);
        row.append(&self.layer);
        keys.append(&row);
        keys.append(&self.key_current);
        keys.append(&self.key_draft);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let kind = gtk::Box::new(gtk::Orientation::Vertical, 6);
        kind.set_hexpand(true);
        field(&kind, t("_Tipo de função", "Function _type"), &self.kind);
        let value = gtk::Box::new(gtk::Orientation::Vertical, 6);
        value.set_hexpand(true);
        field(&value, t("_Função", "_Function"), &self.function);
        row.append(&kind);
        row.append(&value);
        keys.append(&row);
        let modifiers = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        for modifier in &self.modifiers {
            modifiers.append(modifier);
        }
        keys.append(&modifiers);
        let prepare = gtk::Button::with_label(t("Preparar atribuição", "Prepare assignment"));
        prepare.set_halign(gtk::Align::End);
        keys.append(&prepare);
        let e = self.clone();
        self.kind
            .connect_selected_notify(move |_| e.update_functions());
        let e = self.clone();
        prepare.connect_clicked(move |_| e.stage_assignment());
        self.pages
            .add_titled(&keys, Some("keys"), t("Teclas", "Keys"));
        self.update_functions();
    }

    fn update_functions(&self) {
        let names: Vec<&str> = match self.kind.selected() {
            0 => assignments::keyboard_choices()
                .iter()
                .map(|(name, _)| *name)
                .collect(),
            1 => assignments::media().iter().map(|(name, _)| *name).collect(),
            2 => KEYS.iter().map(|(name, _)| *name).collect(),
            _ => vec![t("Sem função", "No function")],
        };
        self.function.set_model(Some(&gtk::StringList::new(&names)));
        self.function.set_selected(0);
        self.function.set_sensitive(self.kind.selected() != 3);
        for modifier in &self.modifiers {
            modifier.set_sensitive(self.kind.selected() == 0);
            modifier.set_active(false);
        }
    }

    fn stage_assignment(&self) {
        let mut session = self.session.borrow_mut();
        let Some(draft) = &mut session.draft else {
            return;
        };
        let index = self.function.selected() as usize;
        let layer = self.layer.selected() as u8;
        let code = match self.kind.selected() {
            0 => assignments::keyboard_choices().get(index).map(|(_, code)| {
                let mut code = *code;
                for (bit, modifier) in self.modifiers.iter().enumerate() {
                    if modifier.is_active() {
                        code[1] |= 1 << bit;
                    }
                }
                code
            }),
            1 => assignments::media().get(index).map(|(_, code)| *code),
            2 => draft.key_code(layer, (index + 1) as u8).ok(),
            _ => Some([0; 4]),
        };
        if let Some(code) = code {
            let _ = draft.set_key_code(layer, self.selected.get(), code);
        }
        drop(session);
        self.status.set_text(t(
            "Atribuição preparada. Clique em Aplicar alterações para gravar.",
            "Assignment prepared. Click Apply changes to write it to the keyboard.",
        ));
        self.refresh();
    }

    fn sync_assignment(&self, code: [u8; 4]) {
        if code == [0; 4] {
            self.kind.set_selected(3);
            return;
        }
        if let Some(index) = assignments::media()
            .iter()
            .position(|(_, value)| *value == code)
        {
            self.kind.set_selected(1);
            self.function.set_selected(index as u32);
            return;
        }
        let choices = assignments::keyboard_choices();
        let exact = choices.iter().position(|(_, value)| *value == code);
        let index = exact.or_else(|| {
            if code[0] == 0 && code[2] == 0 && code[3] != 0 {
                choices
                    .iter()
                    .position(|(_, value)| *value == [0, 0, 0, code[3]])
            } else {
                None
            }
        });
        if let Some(index) = index {
            self.kind.set_selected(0);
            self.function.set_selected(index as u32);
            for (bit, modifier) in self.modifiers.iter().enumerate() {
                modifier.set_active(exact.is_none() && code[1] & (1 << bit) != 0);
            }
        } else {
            self.kind.set_selected(2);
            self.function
                .set_selected(u32::from(self.selected.get() - 1));
        }
    }

    fn edit_lights(&self, effect_changed: bool) {
        if self.syncing.get() {
            return;
        }
        let effect = self.effect.selected() as u8;
        let mut session = self.session.borrow_mut();
        let Some(draft) = &mut session.draft else {
            return;
        };
        if effect > 18 {
            return;
        }
        let original: &[u8; 128] = draft.leds.as_slice().try_into().expect("validated LEDs");
        let changed = if effect_changed {
            aula_f87pro::led_effect(original, effect)
        } else {
            aula_f87pro::led_settings(
                original,
                effect,
                self.brightness.value() as u8,
                self.speed.value() as u8,
                self.palette.selected().min(7) as u8,
            )
        };
        if let Ok(changed) = changed {
            draft.leds = changed.to_vec();
        }
        drop(session);
        self.refresh();
    }

    pub fn refresh(&self) {
        if let Some(settings) = self.settings.borrow().as_ref() {
            let session = self.session.borrow();
            settings.sync(
                session.draft.as_ref(),
                self.transport.get(),
                session.connected,
                self.busy.get(),
            );
        }
        self.syncing.set(true);
        let session = self.session.borrow();
        let dirty = session.dirty();
        let unprepared = self.has_unprepared();
        let compatible = match (&session.baseline, &session.draft) {
            (Some(current), Some(draft)) => current.version == draft.version,
            _ => true,
        };
        let enabled = session.connected && !self.busy.get();
        self.apply
            .set_sensitive(enabled && dirty && !session.conflict && !unprepared && compatible);
        self.undo.set_sensitive(
            enabled
                && !dirty
                && !unprepared
                && !session.conflict
                && session
                    .undo
                    .as_ref()
                    .zip(session.baseline.as_ref())
                    .is_some_and(|(undo, current)| undo.version == current.version),
        );
        self.discard
            .set_sensitive(enabled && (dirty || session.conflict || unprepared));
        self.reload.set_sensitive(!self.busy.get());
        self.content.set_sensitive(!self.busy.get());
        self.pending.set_text(if session.conflict {
            t(
                "Conflito — salve o perfil ou recarregue",
                "Conflict — save a profile or reload",
            )
        } else if dirty {
            t("Alterações ainda não aplicadas", "Changes not yet applied")
        } else {
            t("Sincronizado com o teclado", "Synced with keyboard")
        });
        if unprepared {
            self.pending.set_text(t(
                "Prepare ou descarte a edição da macro",
                "Prepare or discard the macro edit",
            ));
        }
        if !compatible {
            self.pending.set_text(t(
                "Este perfil usa outro transporte. Use a conexão em que ele foi salvo.",
                "This profile uses a different transport. Use the connection it was saved with.",
            ));
        }
        if !session.connected {
            self.pending.set_text(t(
                "Desconectado · edições locais preservadas",
                "Disconnected · local edits preserved",
            ));
        }
        if session.baseline.is_none() {
            self.pending.set_text(t(
                "Aguardando leitura do teclado",
                "Waiting to read keyboard",
            ));
        }
        if let Some(draft) = &session.draft {
            if self
                .layer
                .model()
                .is_some_and(|model| model.n_items() != draft.layers.len() as u32)
            {
                let labels = [t("0 — principal", "0 — main"), "1", "2", "3"];
                let selected = self.layer.selected().min(draft.layers.len() as u32 - 1);
                self.layer
                    .set_model(Some(&gtk::StringList::new(&labels[..draft.layers.len()])));
                self.layer.set_selected(selected);
            }
            self.lighting.set(Lighting::read(&draft.leds));
            self.preview_caption.set_text(if draft.leds[9] != 0 || draft.leds[10] > 18 {
                t("Prévia indisponível para iluminação personalizada. Clique numa tecla para editar.", "Preview unavailable for custom lighting. Click a key to edit its assignment.")
            } else if !session.connected {
                t("Prévia ilustrativa · configuração local · teclado desconectado.", "Illustrative preview · local configuration · keyboard disconnected.")
            } else if session.baseline.as_ref().is_some_and(|s| s.leds != draft.leds) {
                t("Prévia ilustrativa · iluminação editada, ainda não aplicada.", "Illustrative preview · edited lighting, not yet applied.")
            } else {
                t("Prévia ilustrativa · configuração lida do teclado, sem sincronia dos LEDs.", "Illustrative preview · configuration read from keyboard, LEDs not synchronized.")
            });
            let effect = if draft.leds[9] != 0 || draft.leds[10] > 18 {
                19
            } else {
                draft.leds[10] as u32
            };
            self.effect.set_selected(effect);
            let adjustable = (1..=18).contains(&effect);
            for widget in [
                self.brightness.upcast_ref::<gtk::Widget>(),
                self.speed.upcast_ref(),
                self.palette.upcast_ref(),
            ] {
                widget.set_sensitive(adjustable);
            }
            self.speed.set_sensitive(adjustable && effect != 1);
            self.palette
                .set_sensitive(adjustable && ![3, 15, 16, 17].contains(&effect));
            if adjustable {
                let offset = 58 + 2 * (effect as usize - 1);
                self.brightness
                    .set_value(f64::from(draft.leds[offset]).min(4.0));
                self.speed
                    .set_value(f64::from(draft.leds[offset + 1] >> 4).min(4.0));
                self.palette
                    .set_selected(u32::from(draft.leds[offset + 1] & 15).min(7));
            }
            self.key_name.set_text(&tf!(
                "Tecla {}",
                "Key {}",
                KEYS[usize::from(self.selected.get()) - 1].0
            ));
            let layer = self.layer.selected() as u8;
            let current = session
                .baseline
                .as_ref()
                .and_then(|s| s.key_code(layer, self.selected.get()).ok());
            let prepared = draft.key_code(layer, self.selected.get()).ok();
            let target = (layer, self.selected.get());
            if self.controls_for.replace(target) != target
                && let Some(code) = prepared
            {
                self.sync_assignment(code);
            }
            self.key_current.set_text(&tf!(
                "Atual no teclado: {}",
                "Current on keyboard: {}",
                current
                    .map(assignments::describe)
                    .unwrap_or_else(|| "—".into())
            ));
            self.key_draft.set_text(
                &prepared
                    .filter(|c| Some(*c) != current)
                    .map(|c| tf!("Preparado: {}", "Prepared: {}", assignments::describe(c)))
                    .unwrap_or_default(),
            );
            for (id, button) in self.keys.borrow().iter() {
                let code = draft.key_code(layer, *id).unwrap_or([0; 4]);
                let changed = session
                    .baseline
                    .as_ref()
                    .and_then(|s| s.key_code(layer, *id).ok())
                    != Some(code);
                if changed {
                    button.add_css_class("edited");
                } else {
                    button.remove_css_class("edited");
                }
                button.set_tooltip_text(Some(&format!(
                    "{} · {}{}",
                    KEYS[usize::from(*id) - 1].0,
                    assignments::describe(code),
                    if changed {
                        t(" · pendente", " · pending")
                    } else {
                        ""
                    }
                )));
            }
        }
        drop(session);
        if let Some(editor) = self.macro_editor.borrow().as_ref() {
            editor.sync();
        }
        self.syncing.set(false);
    }

    async fn invoke(&self, args: Vec<OsString>) -> Result<String, String> {
        let executable = std::env::current_exe()
            .map_err(|e| e.to_string())?
            .with_file_name("aula-f87pro");
        let mut argv = vec![executable.into_os_string()];
        argv.extend(args);
        let launcher = gio::SubprocessLauncher::new(
            gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_PIPE,
        );
        let backups = self.data.join("backups");
        std::fs::create_dir_all(&backups).map_err(|e| e.to_string())?;
        launcher.set_cwd(&backups);
        let process = launcher
            .spawn(&argv.iter().map(|a| a.as_os_str()).collect::<Vec<_>>())
            .map_err(|e| e.to_string())?;
        let (out, err) = process
            .communicate_utf8_future(None)
            .await
            .map_err(|e| e.to_string())?;
        if process.is_successful() {
            Ok(out.unwrap_or_default().to_string())
        } else {
            Err(format!(
                "{}{}",
                out.unwrap_or_default(),
                err.unwrap_or_default()
            ))
        }
    }

    fn start(&self, message: &str) -> bool {
        if self.busy.replace(true) {
            return false;
        }
        self.spinner.start();
        self.status.set_text(message);
        self.refresh();
        true
    }
    fn finish(&self) {
        self.busy.set(false);
        self.spinner.stop();
        self.refresh();
    }
    pub fn error(&self, error: impl ToString) {
        let message = error.to_string();
        self.details.set_text(&message);
        let (status, conflict) = super::session::error_status(&message);
        if conflict {
            self.session.borrow_mut().conflict = true;
        }
        self.status.set_text(status);
    }

    pub fn load(self: &Rc<Self>, discard: bool) {
        if self.has_unprepared() && !discard {
            self.status.set_text(t(
                "Prepare ou descarte a edição da macro antes de recarregar.",
                "Prepare or discard the macro edit before reloading.",
            ));
            return;
        }
        if discard && let Some(editor) = self.macro_editor.borrow().as_ref() {
            editor.discard();
        }
        if !self.start(t(
            "Lendo a configuração disponível nesta conexão…",
            "Reading configuration available through this connection…",
        )) {
            return;
        }
        let editor = self.clone();
        glib::MainContext::default().spawn_local(async move {
            match editor
                .invoke(vec!["snapshot".into()])
                .await
                .and_then(|out| state::decode(out.as_bytes()).map_err(|e| e.to_string()))
            {
                Ok(state) => {
                    let radio = state.version == 2;
                    editor.session.borrow_mut().load(state, discard);
                    if editor.session.borrow().undo.is_none()
                        && let Ok(bytes) = profiles::read_bytes(&editor.data.join("undo.json"))
                        && let Ok(undo) = state::decode(&bytes)
                    {
                        editor.session.borrow_mut().undo = Some(undo);
                    }
                    editor.connection.set_text(if radio {
                        t("2,4 GHz conectado · iluminação e teclas disponíveis · macros preservadas", "2.4 GHz connected · lighting and keys available · macros preserved")
                    } else {
                        t("USB conectado · estado lido do teclado", "USB connected · state read from keyboard")
                    });
                    editor.status.set_text(if editor.session.borrow().conflict {
                        t("O teclado mudou. Salve o perfil ou descarte as edições para recarregar.", "The keyboard changed. Save a profile or discard your edits to reload.")
                    } else {
                        t("Leitura concluída. As alterações só serão gravadas ao aplicar.", "Read complete. Changes will only be written when you apply them.")
                    });
                }
                Err(error) => {
                    editor.session.borrow_mut().connected = false;
                    editor.connection.set_text(
                        if editor.transport.get()
                            == Some(aula_f87pro::discovery::Transport::Receiver24G)
                        {
                            t("Receptor detectado · teclado sem resposta ou leitura incompleta", "Receiver detected · keyboard not responding or read incomplete")
                        } else {
                            t("Teclado indisponível · verifique conexão e permissões", "Keyboard unavailable · check connection and permissions")
                        },
                    );
                    editor.error(error);
                }
            }
            editor.finish();
        });
    }

    pub fn set_connection(self: &Rc<Self>, transport: Option<aula_f87pro::discovery::Transport>) {
        self.transport.set(transport);
        if transport.is_some() {
            self.connection.set_text(
                if transport == Some(aula_f87pro::discovery::Transport::Receiver24G) {
                    t(
                        "Receptor 2,4 GHz detectado · consultando teclado…",
                        "2.4 GHz receiver detected · querying keyboard…",
                    )
                } else {
                    t(
                        "USB detectado · consultando teclado…",
                        "USB detected · querying keyboard…",
                    )
                },
            );
            self.load(false);
        } else {
            self.session.borrow_mut().connected = false;
            self.connection.set_text(t(
                "Teclado não detectado · edições locais disponíveis",
                "Keyboard not detected · local edits available",
            ));
            self.status.set_text(t(
                "Teclado desconectado. Suas edições foram mantidas.",
                "Keyboard disconnected. Your edits have been preserved.",
            ));
            self.refresh();
        }
    }

    fn apply_draft(self: &Rc<Self>, undoing: bool) {
        if self.has_unprepared() {
            self.status.set_text(t(
                "Prepare ou descarte a edição da macro antes de aplicar.",
                "Prepare or discard the macro edit before applying changes.",
            ));
            return;
        }
        let (desired, expected) = {
            let s = self.session.borrow();
            if !s.connected || s.conflict {
                return;
            }
            let desired = if undoing {
                s.undo.clone()
            } else {
                s.draft.clone()
            };
            let (Some(desired), Some(expected)) = (desired, s.baseline.clone()) else {
                return;
            };
            (desired, expected)
        };
        if !self.start(t(
            "Salvando backup, aplicando e verificando no teclado…",
            "Saving backup, applying and verifying on the keyboard…",
        )) {
            return;
        }
        let editor = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let folder = editor.data.join(format!("transaction-{}", std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos()));
            let result = async {
                let desired_path = folder.join("desired.json"); let expected_path = folder.join("expected.json");
                profiles::atomic_write(&desired_path, &desired.encode().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                profiles::atomic_write(&expected_path, &expected.encode().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
                let output = editor.invoke(vec!["apply-state".into(), desired_path.into_os_string(), expected_path.into_os_string()]).await?;
                let applied: Applied = serde_json::from_str(&output).map_err(|e| e.to_string())?; applied.state.validate().map_err(|e| e.to_string())?;
                Ok::<_, String>(applied)
            }.await;
            let _ = std::fs::remove_dir_all(folder);
            match result {
                Ok(applied) => {
                    editor.session.borrow_mut().applied(applied.state, undoing);
                    let persistence = if undoing { match std::fs::remove_file(editor.data.join("undo.json")) { Ok(()) => Ok(()), Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()), Err(e) => Err(e) } }
                        else { expected.encode().and_then(|bytes| profiles::atomic_write(&editor.data.join("undo.json"), &bytes)) };
                    let backup = applied.backup.unwrap_or_else(|| t("Nenhuma gravação necessária.", "No write needed.").into()); editor.details.set_text(&backup);
                    editor.status.set_text(if !applied.changed { t("O teclado já está com essa configuração.", "The keyboard already has this configuration.") } else if undoing { t("Última alteração desfeita e verificada no teclado.", "Last change undone and verified on the keyboard.") } else { t("Alterações aplicadas e verificadas. Backup do estado acessível salvo.", "Changes applied and verified. Backup of the accessible state saved.") });
                    if let Err(error) = persistence { editor.details.set_text(&tf!("Backup: {backup}\nNão foi possível persistir o atalho Desfazer: {error}", "Backup: {backup}\nCould not persist the Undo shortcut: {error}")); }
                }
                Err(error) => editor.error(error),
            }
            editor.finish();
        });
    }

    fn build_profiles(self: &Rc<Self>) {
        let page = section(
            t("Perfis e recuperação", "Profiles and recovery"),
            t(
                "Salve a configuração disponível nesta conexão. Carregar prepara as edições; Aplicar grava no teclado.",
                "Save the configuration available through this connection. Loading prepares edits; Apply writes them to the keyboard.",
            ),
        );
        field(&page, t("_Perfil salvo", "Saved _profile"), &self.profile);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        for (name, action) in [
            (t("Carregar perfil", "Load profile"), 0),
            (t("Importar perfil…", "Import profile…"), 1),
            (t("Exportar rascunho…", "Export draft…"), 2),
            (t("Carregar backup…", "Load backup…"), 3),
            (t("Carregar só RGB", "Load RGB only"), 4),
        ] {
            let button = gtk::Button::with_label(name);
            let e = self.clone();
            button.connect_clicked(move |_| match action {
                0 | 4 => e.load_profile(action == 4),
                1 | 3 => e.import_file(action == 3),
                _ => e.export_profile(),
            });
            actions.append(&button);
        }
        page.append(&actions);
        let save = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        self.profile_name.set_hexpand(true);
        self.profile_name
            .update_property(&[gtk::accessible::Property::Label(t(
                "Nome do perfil",
                "Profile name",
            ))]);
        save.append(&self.profile_name);
        let button = gtk::Button::with_label(t("Salvar perfil", "Save profile"));
        let e = self.clone();
        button.connect_clicked(move |_| e.save_profile());
        save.append(&button);
        page.append(&save);
        page.append(&text(t("Salvar um nome existente atualiza esse perfil. Perfis e backups ficam apenas neste computador.", "Saving an existing name updates that profile. Profiles and backups stay on this computer."), "muted"));
        self.pages.add_titled(
            &page,
            Some("profiles"),
            t("Perfis e backups", "Profiles and backups"),
        );
    }

    fn update_profiles(&self) {
        match profiles::entries(&self.data.join("profiles")) {
            Ok(entries) => {
                let names: Vec<_> = entries.iter().map(|(name, _)| name.as_str()).collect();
                if entries.is_empty() {
                    self.profile.set_model(Some(&gtk::StringList::new(&[t(
                        "Nenhum perfil salvo",
                        "No saved profiles",
                    )])));
                    self.profile.set_selected(0);
                    self.profile.set_sensitive(false);
                } else {
                    self.profile.set_model(Some(&gtk::StringList::new(&names)));
                    self.profile.set_selected(
                        entries
                            .iter()
                            .position(|(name, _)| name == self.profile_name.text().as_str())
                            .unwrap_or(0) as u32,
                    );
                    self.profile.set_sensitive(true);
                }
                self.profile_entries.replace(entries);
            }
            Err(error) => self.error(error),
        }
    }

    fn save_profile(&self) {
        if self.has_unprepared() {
            self.status.set_text(t(
                "Prepare ou descarte a edição da macro antes de usar perfis.",
                "Prepare or discard the macro edit before using profiles.",
            ));
            return;
        }
        let session = self.session.borrow();
        let Some(draft) = &session.draft else {
            return;
        };
        match profiles::save(
            &self.data.join("profiles"),
            self.profile_name.text().as_str(),
            draft,
        ) {
            Ok(_) => {
                self.update_profiles();
                self.status.set_text(t(
                    "Perfil salvo neste computador. O teclado não foi alterado.",
                    "Profile saved on this computer. The keyboard was not changed.",
                ));
            }
            Err(error) => self.error(error),
        }
    }

    fn load_profile(&self, lighting_only: bool) {
        if self.has_unprepared() {
            self.status.set_text(t(
                "Prepare ou descarte a edição da macro antes de usar perfis.",
                "Prepare or discard the macro edit before using profiles.",
            ));
            return;
        }
        let path = self
            .profile_entries
            .borrow()
            .get(self.profile.selected() as usize)
            .map(|(_, path)| path.clone());
        if path.is_none() {
            self.status.set_text(t(
                "Salve ou importe um perfil antes de carregar.",
                "Save or import a profile before loading.",
            ));
        }
        if let Some(path) = path {
            match profiles::read(&path) {
                Ok(profile) => {
                    let state = if lighting_only {
                        let result = self
                            .session
                            .borrow()
                            .draft
                            .as_ref()
                            .ok_or_else(|| {
                                std::io::Error::other(t(
                                    "Leia o teclado antes de carregar a iluminação.",
                                    "Read the keyboard before loading lighting.",
                                ))
                            })
                            .and_then(|draft| draft.with_lighting_from(&profile.state));
                        match result {
                            Ok(state) => state,
                            Err(error) => {
                                self.error(error);
                                return;
                            }
                        }
                    } else {
                        profile.state
                    };
                    self.profile_name.set_text(&profile.name);
                    self.session.borrow_mut().draft = Some(state);
                    self.status.set_text(t(
                        "Perfil carregado como rascunho. Revise e aplique quando quiser.",
                        "Profile loaded as a draft. Review and apply when ready.",
                    ));
                    self.refresh();
                    if lighting_only {
                        self.pages.set_visible_child_name("lights");
                    }
                }
                Err(error) => self.error(error),
            }
        }
    }

    fn export_profile(self: &Rc<Self>) {
        if self.has_unprepared() {
            self.status.set_text(t(
                "Prepare ou descarte a edição da macro antes de usar perfis.",
                "Prepare or discard the macro edit before using profiles.",
            ));
            return;
        }
        let Some(draft) = self.session.borrow().draft.clone() else {
            return;
        };
        let name = if self.profile_name.text().trim().is_empty() {
            t("Perfil F87 Pro", "F87 Pro profile").into()
        } else {
            self.profile_name.text().to_string()
        };
        let e = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let dialog = gtk::FileDialog::builder()
                .title(t("Exportar perfil F87 Pro", "Export F87 Pro profile"))
                .initial_name(t("perfil-f87pro.json", "f87pro-profile.json"))
                .build();
            match dialog.save_future(Some(&e.window)).await {
                Ok(file) => {
                    if let Some(path) = file.path() {
                        match profiles::export(&path, &name, &draft) {
                            Ok(()) => e.status.set_text(t(
                                "Perfil exportado. Nenhuma configuração foi gravada no teclado.",
                                "Profile exported. No configuration was written to the keyboard.",
                            )),
                            Err(error) => e.error(error),
                        }
                    }
                }
                Err(error) if error.matches(gtk::DialogError::Dismissed) => (),
                Err(error) => e.error(error),
            }
        });
    }

    fn import_file(self: &Rc<Self>, backup: bool) {
        if self.has_unprepared() {
            self.status.set_text(t(
                "Prepare ou descarte a edição da macro antes de usar perfis.",
                "Prepare or discard the macro edit before using profiles.",
            ));
            return;
        }
        let e = self.clone();
        glib::MainContext::default().spawn_local(async move {
            let dialog = gtk::FileDialog::builder().title(if backup { t("Carregar backup F87 Pro", "Load F87 Pro backup") } else { t("Importar perfil F87 Pro", "Import F87 Pro profile") }).build();
            match dialog.open_future(Some(&e.window)).await {
                Ok(file) => if let Some(path) = file.path() {
                    let result = profiles::read_bytes(&path).and_then(|bytes| {
                        if let Ok(profile) = serde_json::from_slice::<profiles::Profile>(&bytes) { profile.state.validate()?; return Ok(profile); }
                        if let Ok(state) = state::decode(&bytes) { return Ok(profiles::Profile { name: "Backup F87 Pro".into(), state }); }
                        let mut state = e.session.borrow().draft.clone().ok_or_else(|| std::io::Error::other(t("Leia o teclado antes de importar um backup antigo.", "Read the keyboard before importing a legacy backup.")))?;
                        let (layer, payload) = state::legacy_backup(&bytes, bytes.len() != 134)?;
                        if let Some(index) = layer {
                            let layer = state.layers.get_mut(index as usize).filter(|layer| layer.len() == payload.len())
                                .ok_or_else(|| std::io::Error::other(t("A camada ou o transporte do backup difere da conexão atual.", "The backup's layer or transport differs from the current connection.")))?;
                            layer.copy_from_slice(payload);
                        } else {
                            if state.version == 2 && state.leds[14] != payload[14] {
                                return Err(std::io::Error::other(t("Este backup LED pertence a outra conexão. Restaure usando o cabo ou carregue apenas RGB de um perfil.", "This LED backup belongs to a different connection. Restore using the cable or load only RGB from a profile.")));
                            }
                            state.leds = payload.to_vec();
                        }
                        Ok(profiles::Profile { name: t("Backup importado", "Imported backup").into(), state })
                    });
                    match result {
                        Ok(profile) => {
                            if !backup && let Err(error) = profiles::save(&e.data.join("profiles"), &profile.name, &profile.state) { e.error(error); return; }
                            e.profile_name.set_text(&profile.name); e.session.borrow_mut().draft = Some(profile.state); e.update_profiles();
                            e.status.set_text(t("Arquivo carregado como rascunho. Revise as alterações antes de aplicar.", "File loaded as a draft. Review changes before applying.")); e.refresh();
                        }
                        Err(error) => e.error(error),
                    }
                },
                Err(error) if error.matches(gtk::DialogError::Dismissed) => (),
                Err(error) => e.error(error),
            }
        });
    }

    pub fn has_unprepared(&self) -> bool {
        self.macro_editor
            .borrow()
            .as_ref()
            .is_some_and(|e| e.has_unprepared())
    }
    pub fn has_unsaved(&self) -> bool {
        self.session.borrow().dirty() || self.has_unprepared()
    }
    pub fn selected_key(&self) -> (u8, u8) {
        (self.layer.selected() as u8, self.selected.get())
    }
    pub fn stage_controls(&self, controls: aula_f87pro::device_settings::Controls) {
        if self.busy.get() {
            return;
        }
        let result = {
            let mut session = self.session.borrow_mut();
            session
                .draft
                .as_mut()
                .map(|state| controls.apply(&mut state.leds))
        };
        match result {
            Some(Ok(())) => {
                self.message(t(
                    "Ajustes preparados. Use Aplicar alterações para gravar no teclado.",
                    "Settings staged. Use Apply changes to write them to the keyboard.",
                ));
            }
            Some(Err(error)) => self.error(error),
            None => self.message(t(
                "Carregue uma configuração antes de editar os ajustes.",
                "Load a configuration before editing settings.",
            )),
        }
    }

    pub fn message(&self, message: &str) {
        self.status.set_text(message);
        self.refresh();
    }
}
