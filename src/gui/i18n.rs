use std::sync::atomic::{AtomicBool, Ordering};

static ENGLISH: AtomicBool = AtomicBool::new(false);

pub fn init(english: bool) {
    ENGLISH.store(english, Ordering::Relaxed);
}

pub(super) fn english() -> bool {
    ENGLISH.load(Ordering::Relaxed)
}

pub fn t(pt: &'static str, en: &'static str) -> &'static str {
    if english() { en } else { pt }
}

macro_rules! tf {
    ($pt:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::i18n::english() {
            format!($en $(, $arg)*)
        } else {
            format!($pt $(, $arg)*)
        }
    };
}
pub(super) use tf;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_and_portuguese_keep_values_and_format_arguments() {
        if std::env::var_os("F87_TEST_I18N_ENGLISH").is_some() {
            init(true);
            assert_eq!(t("Tecla", "Key"), "Key");
            let number = 3;
            assert_eq!(tf!("Camada {number}", "Layer {number}"), "Layer 3");
            assert_eq!(tf!("Tecla {}", "Key {}", "A"), "Key A");
            assert_eq!(crate::assignments::describe([0; 4]), "Disabled");
            assert_eq!(crate::assignments::describe([2, 0, 0, 0xe2]), "Mute");
            assert!(
                crate::session::error_status("Permission denied")
                    .0
                    .starts_with("No permission")
            );
        } else {
            assert_eq!(t("Tecla", "Key"), "Tecla");
            assert_eq!(tf!("Tecla {}", "Key {}", "A"), "Tecla A");
            // Each language gets its own process; concurrent tests keep the default Portuguese UI.
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "i18n::tests::english_and_portuguese_keep_values_and_format_arguments",
                ])
                .env("F87_TEST_I18N_ENGLISH", "1")
                .status()
                .unwrap();
            assert!(status.success());
        }
    }
}
