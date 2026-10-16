use super::i18n::{t, tf};
use super::{
    assignments,
    editor::{Editor, section, text},
};
use aula_f87pro::{
    KEYS,
    macros::{self, MacroDefinition, MacroEvent},
};
use gtk::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

pub struct MacroEditor {
    pub widget: gtk::Box,
    editor: Weak<Editor>,
    slot: gtk::DropDown,
    name: gtk::Entry,
    key: gtk::DropDown,
    action: gtk::DropDown,
    delay: gtk::SpinButton,
    mode: gtk::DropDown,
    repeat: gtk::SpinButton,
    bind: gtk::CheckButton,
    target: gtk::Label,
    target_key: gtk::DropDown,
    target_layer: gtk::DropDown,
    memory: gtk::Label,
    warning: gtk::Label,
    list: gtk::Box,
    prepare: gtk::Button,
    events: RefCell<Vec<MacroEvent>>,
    buffer: RefCell<Vec<u8>>,
    blobs: RefCell<Vec<Vec<u8>>>,
    syncing: Cell<bool>,
    last_target: Cell<(u8, u8)>,
    last_binding: Cell<Option<[u8; 4]>>,
    initial: RefCell<Option<MacroDefinition>>,
    last_slot: Cell<u32>,
}

fn definition_changed(
    initial: Option<&MacroDefinition>,
    name: &str,
    events: &[MacroEvent],
) -> bool {
    initial.is_some_and(|initial| initial.name != name || initial.events != events)
}

fn key_choices() -> Vec<(&'static str, u8)> {
    assignments::keyboard_choices()
        .into_iter()
        .filter_map(|(name, code)| {
            if code[3] != 0 {
                Some((name, code[3]))
            } else if code[1].count_ones() == 1 {
                Some((name, 0xe0 + code[1].trailing_zeros() as u8))
            } else {
                None
            }
        })
        .collect()
}

impl MacroEditor {
    pub fn new(editor: &Rc<Editor>) -> Rc<Self> {
        let names = key_choices();
        let this = Rc::new(Self {
            widget: section(
                t("Sequências de teclado", "Keyboard sequences"),
                t(
                    "Adicione pressionamentos, liberações e intervalos. A macro precisa soltar todas as teclas antes de terminar.",
                    "Add key presses, releases and delays. The macro must release all keys before it ends.",
                ),
            ),
            editor: Rc::downgrade(editor),
            slot: gtk::DropDown::from_strings(&[t("Nova macro", "New macro")]),
            name: gtk::Entry::builder()
                .placeholder_text(t("Nome da macro", "Macro name"))
                .max_length(30)
                .build(),
            key: gtk::DropDown::from_strings(
                &names.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ),
            action: gtk::DropDown::from_strings(&[
                t("Toque completo", "Full tap"),
                t("Pressionar", "Press"),
                t("Soltar", "Release"),
            ]),
            delay: gtk::SpinButton::with_range(0.0, 1_048_575.0, 1.0),
            mode: gtk::DropDown::from_strings(&[
                t("Repetir N vezes", "Repeat N times"),
                t("Enquanto pressionada", "While held"),
                t("Alternar ao pressionar", "Toggle on press"),
            ]),
            repeat: gtk::SpinButton::with_range(1.0, 255.0, 1.0),
            bind: gtk::CheckButton::with_label(t(
                "Vincular à tecla selecionada",
                "Bind to selected key",
            )),
            target: text("", "muted"),
            target_key: gtk::DropDown::from_strings(
                &KEYS.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            ),
            target_layer: gtk::DropDown::from_strings(&[
                t("Camada 0 — principal", "Layer 0 — main"),
                t("Camada 1", "Layer 1"),
                t("Camada 2", "Layer 2"),
                t("Camada 3", "Layer 3"),
            ]),
            memory: text(t("Aguardando leitura…", "Waiting to read…"), "muted"),
            warning: text("", "pending-text"),
            list: gtk::Box::new(gtk::Orientation::Vertical, 6),
            prepare: gtk::Button::with_label(t("Preparar macro", "Prepare macro")),
            events: RefCell::new(Vec::new()),
            buffer: RefCell::new(Vec::new()),
            blobs: RefCell::new(Vec::new()),
            syncing: Cell::new(false),
            last_target: Cell::new((255, 255)),
            last_binding: Cell::new(None),
            initial: RefCell::new(None),
            last_slot: Cell::new(0),
        });
        this.delay.set_value(30.0);
        this.repeat.set_value(1.0);
        this.bind.set_active(true);
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        this.slot.set_hexpand(true);
        this.name.set_hexpand(true);
        this.slot
            .update_property(&[gtk::accessible::Property::Label(t(
                "Macro para editar",
                "Macro to edit",
            ))]);
        this.name
            .update_property(&[gtk::accessible::Property::Label(t(
                "Nome da macro",
                "Macro name",
            ))]);
        top.append(&this.slot);
        top.append(&this.name);
        this.widget.append(&top);
        this.widget.append(&this.memory);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        for (caption, widget) in [
            (t("Tecla", "Key"), this.key.upcast_ref::<gtk::Widget>()),
            (t("Evento", "Event"), this.action.upcast_ref()),
            (t("Intervalo (ms)", "Delay (ms)"), this.delay.upcast_ref()),
        ] {
            let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
            column.set_hexpand(true);
            column.append(&text(caption, "muted"));
            widget.update_property(&[gtk::accessible::Property::Label(caption)]);
            column.append(widget);
            row.append(&column);
        }
        let add = gtk::Button::with_label(t("Adicionar evento", "Add event"));
        row.append(&add);
        this.widget.append(&row);
        let scroll = gtk::ScrolledWindow::builder()
            .child(&this.list)
            .min_content_height(120)
            .max_content_height(220)
            .propagate_natural_height(true)
            .build();
        this.widget.append(&scroll);
        let playback = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        this.mode.set_hexpand(true);
        this.mode
            .update_property(&[gtk::accessible::Property::Label(t(
                "Modo de repetição",
                "Playback mode",
            ))]);
        this.repeat
            .update_property(&[gtk::accessible::Property::Label(t(
                "Número de repetições",
                "Repeat count",
            ))]);
        playback.append(&this.mode);
        playback.append(&this.repeat);
        this.widget.append(&playback);
        this.widget.append(&this.bind);
        let target = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        this.target_key.set_hexpand(true);
        this.target_key
            .update_property(&[gtk::accessible::Property::Label(t(
                "Tecla de destino da macro",
                "Macro target key",
            ))]);
        this.target_layer
            .update_property(&[gtk::accessible::Property::Label(t(
                "Camada de destino da macro",
                "Macro target layer",
            ))]);
        target.append(&this.target_key);
        target.append(&this.target_layer);
        this.widget.append(&target);
        this.widget.append(&this.target);
        this.widget.append(&this.warning);
        this.prepare.set_halign(gtk::Align::End);
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        actions.set_halign(gtk::Align::End);
        let discard = gtk::Button::with_label(t("Descartar edição da macro", "Discard macro edit"));
        actions.append(&discard);
        actions.append(&this.prepare);
        this.widget.append(&actions);
        let m = this.clone();
        this.slot.connect_selected_notify(move |_| {
            if !m.syncing.get() {
                if m.has_unprepared() {
                    m.syncing.set(true);
                    m.slot.set_selected(m.last_slot.get());
                    m.syncing.set(false);
                    m.warning.set_text(t(
                        "Prepare ou descarte a edição atual antes de trocar de macro.",
                        "Prepare or discard the current edit before switching macros.",
                    ));
                } else {
                    m.select();
                }
            }
        });
        let m = this.clone();
        discard.connect_clicked(move |_| m.discard());
        let m = this.clone();
        this.name.connect_changed(move |_| m.changed());
        for dropdown in [&this.target_key, &this.target_layer] {
            let m = this.clone();
            dropdown.connect_selected_notify(move |_| {
                if !m.syncing.get() {
                    m.update_target();
                    m.changed();
                }
            });
        }
        let m = this.clone();
        this.mode
            .connect_selected_notify(move |_| m.repeat.set_sensitive(m.mode.selected() == 0));
        let m = this.clone();
        add.connect_clicked(move |_| m.add());
        let m = this.clone();
        this.prepare.connect_clicked(move |_| m.prepare());
        this
    }

    pub fn has_unprepared(&self) -> bool {
        definition_changed(
            self.initial.borrow().as_ref(),
            self.name.text().as_str(),
            &self.events.borrow(),
        )
    }

    pub fn discard(self: &Rc<Self>) {
        self.select();
        self.update_target();
        self.changed();
    }

    fn changed(&self) {
        if !self.syncing.get()
            && let Some(editor) = self.editor.upgrade()
        {
            editor.refresh();
        }
    }

    fn target_selection(&self) -> (u8, u8) {
        (
            self.target_layer.selected().min(3) as u8,
            self.target_key.selected().min(86) as u8 + 1,
        )
    }

    fn update_target(self: &Rc<Self>) {
        let target = self.target_selection();
        self.target.set_text(&tf!(
            "Destino: {} · camada {}",
            "Target: {} · layer {}",
            KEYS[usize::from(target.1) - 1].0,
            target.0
        ));
        if self.has_unprepared() {
            return;
        }
        if let Some(editor) = self.editor.upgrade() {
            let code = editor
                .session
                .borrow()
                .draft
                .as_ref()
                .and_then(|draft| draft.key_code(target.0, target.1).ok());
            if let Some(code) = code
                && code[0] == 3
            {
                self.syncing.set(true);
                self.mode.set_selected(match code[1] {
                    2 => 1,
                    4 => 2,
                    _ => 0,
                });
                self.repeat.set_value(f64::from(code[2].max(1)));
                let slot = u32::from(code[3]);
                if (slot as usize) < self.blobs.borrow().len() {
                    self.slot.set_selected(slot);
                    self.syncing.set(false);
                    self.select();
                } else {
                    self.syncing.set(false);
                }
            }
        }
    }

    pub fn sync(self: &Rc<Self>) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let target = editor.selected_key();
        let target_changed = self.last_target.replace(target) != target;
        if target_changed {
            self.syncing.set(true);
            self.target_layer.set_selected(u32::from(target.0));
            self.target_key.set_selected(u32::from(target.1 - 1));
            self.syncing.set(false);
        }
        let session = editor.session.borrow();
        let Some(draft) = &session.draft else {
            self.prepare.set_sensitive(false);
            return;
        };
        let radio = draft.version == 2 || session.baseline.as_ref().is_some_and(|s| s.version == 2);
        self.widget.set_sensitive(!radio);
        if radio {
            self.prepare.set_sensitive(false);
            self.memory.set_text(t(
                "Macros preservadas · edição disponível por cabo USB.",
                "Macros preserved · editing available over USB cable.",
            ));
            self.warning.set_text(t("O receptor permite ler apenas parte da memória. Conecte por cabo para editar macros com backup completo.", "The receiver can only read part of the memory. Connect by cable to edit macros with a complete backup."));
            return;
        }
        let destination = self.target_selection();
        let binding = draft.key_code(destination.0, destination.1).ok();
        let binding_changed = self.last_binding.replace(binding) != binding;
        let bytes = draft.macros.clone();
        drop(session);
        if *self.buffer.borrow() == bytes {
            if target_changed || binding_changed {
                self.update_target();
            }
            return;
        }
        self.buffer.replace(bytes.clone());
        let Ok(buffer) = <&[u8; 4096]>::try_from(bytes.as_slice()) else {
            return;
        };
        match macros::macro_blobs(buffer) {
            Ok(blobs) => {
                let unprepared = self.has_unprepared();
                self.syncing.set(true);
                let index = self.slot.selected();
                let mut names: Vec<String> = blobs
                    .iter()
                    .enumerate()
                    .map(|(i, blob)| {
                        macros::decode_definition(blob)
                            .map(|definition| format!("{} · {}", i + 1, definition.name))
                            .unwrap_or_else(|_| {
                                tf!(
                                    "{} · outros eventos (preservada)",
                                    "{} · other events (preserved)",
                                    i + 1
                                )
                            })
                    })
                    .collect();
                names.push(t("Nova macro", "New macro").into());
                self.slot.set_model(Some(&gtk::StringList::new(
                    &names.iter().map(String::as_str).collect::<Vec<_>>(),
                )));
                self.slot.set_selected(index.min(blobs.len() as u32));
                let used = blobs.len() * 4 + blobs.iter().map(Vec::len).sum::<usize>();
                let noun = if blobs.len() == 1 { "macro" } else { "macros" };
                self.memory.set_text(&tf!(
                    "{} {noun} · {used} de 4096 bytes utilizados",
                    "{} {noun} · {used} of 4096 bytes used",
                    blobs.len()
                ));
                self.blobs.replace(blobs);
                self.syncing.set(false);
                if !unprepared {
                    self.select();
                } else {
                    self.warning.set_text(t("A edição local foi preservada. Prepare ou descarte antes de trocar de macro.", "The local edit was preserved. Prepare or discard it before switching macros."));
                }
                if target_changed || binding_changed {
                    self.update_target();
                }
            }
            Err(_) => {
                self.memory.set_text(t(
                    "Formato de macros não reconhecido. O conteúdo será preservado.",
                    "Unrecognized macro format. Its contents will be preserved.",
                ));
                self.prepare.set_sensitive(false);
            }
        }
    }

    fn select(self: &Rc<Self>) {
        let blob = self
            .blobs
            .borrow()
            .get(self.slot.selected() as usize)
            .cloned();
        match blob.as_deref().map(macros::decode_definition).transpose() {
            Ok(definition) => {
                let definition = definition.unwrap_or(MacroDefinition {
                    name: String::new(),
                    events: Vec::new(),
                });
                self.syncing.set(true);
                self.initial.replace(Some(definition.clone()));
                self.last_slot.set(self.slot.selected());
                self.name.set_text(&definition.name);
                self.events.replace(definition.events);
                self.warning.set_text("");
                self.prepare.set_sensitive(true);
                self.render();
                self.syncing.set(false);
            }
            Err(_) => {
                self.initial.replace(None);
                self.last_slot.set(self.slot.selected());
                self.warning.set_text(t("Esta macro contém eventos ainda não editáveis. Ela será mantida sem alterações.", "This macro contains events that cannot be edited yet. It will be kept unchanged."));
                self.prepare.set_sensitive(false);
            }
        }
    }

    fn add(self: &Rc<Self>) {
        let Some((_, key)) = key_choices().get(self.key.selected() as usize).copied() else {
            return;
        };
        if self.events.borrow().len() >= 999 {
            self.warning
                .set_text(t("Limite de eventos atingido.", "Event limit reached."));
            return;
        }
        let event = MacroEvent {
            key,
            pressed: self.action.selected() != 2,
            delay_ms: self.delay.value_as_int() as u32,
        };
        self.events.borrow_mut().push(event.clone());
        if self.action.selected() == 0 {
            self.events.borrow_mut().push(MacroEvent {
                pressed: false,
                ..event
            });
        }
        self.render();
        self.changed();
    }

    fn render(self: &Rc<Self>) {
        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }
        let choices = key_choices();
        for (i, event) in self.events.borrow().iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.append(&text(&format!("{:02}", i + 1), "muted"));
            let action =
                gtk::DropDown::from_strings(&[t("Pressionar", "Press"), t("Soltar", "Release")]);
            action.set_selected(u32::from(!event.pressed));
            action.update_property(&[gtk::accessible::Property::Label(t(
                "Ação do evento",
                "Event action",
            ))]);
            let key = gtk::DropDown::from_strings(
                &choices.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
            );
            key.set_selected(
                choices
                    .iter()
                    .position(|(_, hid)| *hid == event.key)
                    .map_or(gtk::INVALID_LIST_POSITION, |n| n as u32),
            );
            key.set_hexpand(true);
            key.update_property(&[gtk::accessible::Property::Label(t(
                "Tecla do evento",
                "Event key",
            ))]);
            let delay = gtk::SpinButton::with_range(0.0, 1_048_575.0, 1.0);
            delay.set_value(f64::from(event.delay_ms));
            delay.update_property(&[gtk::accessible::Property::Label(t(
                "Intervalo do evento em milissegundos",
                "Event delay in milliseconds",
            ))]);
            let m = self.clone();
            action.connect_selected_notify(move |value| {
                if let Some(event) = m.events.borrow_mut().get_mut(i) {
                    event.pressed = value.selected() == 0;
                }
                m.changed();
            });
            let m = self.clone();
            key.connect_selected_notify(move |value| {
                if let Some(event) = m.events.borrow_mut().get_mut(i)
                    && let Some((_, hid)) = key_choices().get(value.selected() as usize)
                {
                    event.key = *hid;
                }
                m.changed();
            });
            let m = self.clone();
            delay.connect_value_changed(move |value| {
                if let Some(event) = m.events.borrow_mut().get_mut(i) {
                    event.delay_ms = value.value_as_int() as u32;
                }
                m.changed();
            });
            row.append(&action);
            row.append(&key);
            row.append(&delay);
            for (icon, label, direction) in [
                (
                    "go-up-symbolic",
                    t("Mover evento para cima", "Move event up"),
                    -1,
                ),
                (
                    "go-down-symbolic",
                    t("Mover evento para baixo", "Move event down"),
                    1,
                ),
                (
                    "edit-delete-symbolic",
                    t("Remover evento", "Remove event"),
                    0,
                ),
            ] {
                let button = gtk::Button::from_icon_name(icon);
                button.set_tooltip_text(Some(label));
                button.update_property(&[gtk::accessible::Property::Label(label)]);
                let m = self.clone();
                button.connect_clicked(move |_| {
                    let mut events = m.events.borrow_mut();
                    if direction == 0 {
                        events.remove(i);
                    } else {
                        let next = i as isize + direction;
                        if next >= 0 && (next as usize) < events.len() {
                            events.swap(i, next as usize);
                        }
                    }
                    drop(events);
                    m.render();
                    m.changed();
                });
                row.append(&button);
            }
            self.list.append(&row);
        }
        if self.events.borrow().is_empty() {
            self.list.append(&text(
                t(
                    "A sequência está vazia. Adicione o primeiro evento acima.",
                    "The sequence is empty. Add the first event above.",
                ),
                "muted",
            ));
        }
    }

    fn prepare(&self) {
        let Some(editor) = self.editor.upgrade() else {
            return;
        };
        let result = (|| -> std::io::Result<()> {
            let mut session = editor.session.borrow_mut();
            let draft = session.draft.as_mut().ok_or_else(|| {
                std::io::Error::other(t("Leia o teclado primeiro.", "Read the keyboard first."))
            })?;
            let buffer = draft.macros.as_slice().try_into().map_err(|_| {
                std::io::Error::other(t("Buffer de macros inválido.", "Invalid macro buffer."))
            })?;
            let definition = MacroDefinition {
                name: self.name.text().to_string(),
                events: self.events.borrow().clone(),
            };
            let slot = self.slot.selected();
            let changed = macros::replace_macro(buffer, slot as usize, &definition)?;
            let mode = [1, 2, 4]
                .get(self.mode.selected() as usize)
                .copied()
                .unwrap_or(1);
            let binding = macros::macro_binding(
                slot as u8,
                mode,
                if mode == 1 {
                    self.repeat.value_as_int() as u8
                } else {
                    1
                },
            )?;
            if self.bind.is_active() {
                let (layer, key) = self.target_selection();
                draft.set_key_code(layer, key, binding)?;
            }
            draft.macros = changed.to_vec();
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.initial.replace(Some(MacroDefinition {
                    name: self.name.text().to_string(),
                    events: self.events.borrow().clone(),
                }));
                self.warning.set_text("");
                editor.message(t(
                    "Macro preparada no rascunho. Revise e aplique para gravar no teclado.",
                    "Macro prepared in the draft. Review and apply to write it to the keyboard.",
                ));
            }
            Err(error) => {
                self.warning.set_text(
                    if error.to_string().contains("paired") || error.to_string().contains("release")
                    {
                        t("Cada tecla pressionada precisa ser solta, sem pressionamentos duplicados.", "Every pressed key must be released, without duplicate presses.")
                    } else if error.to_string().contains("full") {
                        t("Memória de macros cheia. Encurte a sequência.", "Macro memory full. Shorten the sequence.")
                    } else {
                        t("Não foi possível preparar a macro. Verifique nome, eventos e intervalos.", "Could not prepare the macro. Check its name, events and delays.")
                    },
                );
                editor.error(error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unfinished_new_macro_and_existing_macro_edits_are_detected_and_reversible() {
        let empty = MacroDefinition {
            name: String::new(),
            events: Vec::new(),
        };
        assert!(!definition_changed(Some(&empty), "", &[]));
        assert!(definition_changed(Some(&empty), "Minha macro", &[]));
        let event = MacroEvent {
            key: 4,
            pressed: true,
            delay_ms: 30,
        };
        assert!(definition_changed(
            Some(&empty),
            "",
            std::slice::from_ref(&event)
        ));
        let saved = MacroDefinition {
            name: "A".into(),
            events: vec![event.clone()],
        };
        assert!(!definition_changed(Some(&saved), "A", &saved.events));
        let mut edited = event;
        edited.delay_ms = 40;
        assert!(definition_changed(Some(&saved), "A", &[edited]));
        assert!(!definition_changed(Some(&saved), "A", &saved.events));
        assert!(!definition_changed(None, "opaque data", &[]));
    }
}
