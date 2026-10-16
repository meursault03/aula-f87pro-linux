use super::i18n::t;
use aula_f87pro::state::Snapshot;

pub fn error_status(message: &str) -> (&'static str, bool) {
    if message.contains("ROLLBACK FAILED") {
        (
            t(
                "Falha ao aplicar e recuperar o teclado. Conecte por USB, recarregue e restaure o backup indicado nos detalhes.",
                "Failed to apply and recover the keyboard. Connect over USB, reload and restore the backup shown in the details.",
            ),
            true,
        )
    } else if message.contains("original state restored") {
        (
            t(
                "Não foi possível aplicar; o estado anterior foi restaurado. Suas edições locais foram mantidas.",
                "Could not apply; the previous state was restored. Your local edits were preserved.",
            ),
            false,
        )
    } else if message.contains("changed since") {
        (
            t(
                "O teclado mudou fora do app. Salve o rascunho como perfil e recarregue.",
                "The keyboard changed outside the app. Save the draft as a profile and reload.",
            ),
            true,
        )
    } else if message.contains("Permission denied") {
        (
            t(
                "Sem permissão para acessar o teclado. Consulte os detalhes.",
                "No permission to access the keyboard. See details.",
            ),
            false,
        )
    } else {
        (
            t(
                "Não foi possível concluir. Nenhuma edição local foi descartada. Consulte os detalhes.",
                "Could not complete the operation. No local edits were discarded. See details.",
            ),
            false,
        )
    }
}

#[derive(Default)]
pub struct Session {
    pub baseline: Option<Snapshot>,
    pub draft: Option<Snapshot>,
    pub undo: Option<Snapshot>,
    pub conflict: bool,
    pub connected: bool,
}

impl Session {
    pub fn dirty(&self) -> bool {
        self.draft.is_some() && self.draft != self.baseline
    }

    pub fn load(&mut self, state: Snapshot, discard: bool) {
        self.connected = true;
        if self.baseline.is_none() && self.draft.is_some() && !discard {
            self.baseline = Some(state);
            self.conflict = false;
        } else if self.dirty() && !discard {
            self.conflict = self.baseline.as_ref() != Some(&state);
        } else {
            self.baseline = Some(state.clone());
            self.draft = Some(state);
            self.conflict = false;
        }
    }

    pub fn applied(&mut self, state: Snapshot, undoing: bool) {
        self.undo = if undoing { None } else { self.baseline.clone() };
        self.baseline = Some(state.clone());
        self.draft = Some(state);
        self.conflict = false;
        self.connected = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn snapshot() -> Snapshot {
        Snapshot {
            version: 1,
            identity: vec![3, 0, 0, 0, 1, 11],
            leds: vec![0; 128],
            layers: vec![vec![0; 512]; 4],
            macros: vec![0; 4096],
        }
    }
    #[test]
    fn reconnect_preserves_edits_and_detects_external_changes() {
        let original = snapshot();
        let mut session = Session::default();
        session.load(original.clone(), false);
        session.draft.as_mut().unwrap().leds[10] = 2;
        session.load(original.clone(), false);
        assert!(session.dirty());
        assert!(!session.conflict);
        let mut external = original;
        external.layers[0][0] = 7;
        session.load(external.clone(), false);
        assert!(session.conflict);
        assert_eq!(session.draft.as_ref().unwrap().leds[10], 2);
        session.load(external.clone(), true);
        assert!(!session.dirty());
        assert!(!session.conflict);
        assert_eq!(session.draft, Some(external));
    }
    #[test]
    fn only_successful_changes_produce_an_undo_state() {
        let original = snapshot();
        let mut session = Session::default();
        session.load(original.clone(), false);
        let mut changed = original.clone();
        changed.leds[10] = 1;
        session.applied(changed.clone(), false);
        assert_eq!(session.undo, Some(original));
        assert!(!session.dirty());
        let undo = session.undo.clone().unwrap();
        session.applied(undo, true);
        assert!(session.undo.is_none());
        assert_eq!(session.baseline, session.draft);
    }

    #[test]
    fn a_profile_prepared_offline_is_kept_when_the_keyboard_connects() {
        let mut session = Session::default();
        let mut profile = snapshot();
        profile.leds[10] = 3;
        session.draft = Some(profile.clone());
        session.load(snapshot(), false);
        assert_eq!(session.draft, Some(profile));
        assert!(session.baseline.is_some());
        assert!(session.dirty());
        assert!(!session.conflict);
    }

    #[test]
    fn rollback_failures_are_distinct_from_failures_that_were_recovered() {
        let (failed, conflict) =
            error_status("write failed; ROLLBACK FAILED; restore backup: test.json");
        assert!(failed.contains("recuperar"));
        assert!(conflict);
        let (restored, conflict) =
            error_status("write failed; original state restored; backup: test.json");
        assert!(restored.contains("restaurado"));
        assert!(!conflict);
        assert!(error_status("keyboard changed since it was loaded").1);
        assert!(!error_status("Permission denied").1);
    }
}
