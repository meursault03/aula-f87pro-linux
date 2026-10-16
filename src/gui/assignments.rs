use super::i18n::t;
pub fn media() -> [(&'static str, [u8; 4]); 7] {
    [
        (t("Reproduzir / pausar", "Play / pause"), [2, 0, 0, 0xcd]),
        (t("Parar", "Stop"), [2, 0, 0, 0xb7]),
        (t("Faixa anterior", "Previous track"), [2, 0, 0, 0xb6]),
        (t("Próxima faixa", "Next track"), [2, 0, 0, 0xb5]),
        (t("Aumentar volume", "Volume up"), [2, 0, 0, 0xe9]),
        (t("Diminuir volume", "Volume down"), [2, 0, 0, 0xea]),
        (t("Silenciar", "Mute"), [2, 0, 0, 0xe2]),
    ]
}

pub fn standard(name: &str) -> Option<[u8; 4]> {
    let modifier = match name {
        "LCtrl" => 1,
        "LShift" => 2,
        "LAlt" => 4,
        "LWin" => 8,
        "RCtrl" => 16,
        "RShift" => 32,
        "RAlt" => 64,
        _ => 0,
    };
    if modifier != 0 {
        return Some([0, modifier, 0, 0]);
    }
    let hid = match name {
        "Esc" => 0x29,
        "Enter" => 0x28,
        "Tab" => 0x2b,
        "Backspace" => 0x2a,
        "Space" => 0x2c,
        "CapsLock" => 0x39,
        "`" => 0x35,
        "-" => 0x2d,
        "=" => 0x2e,
        "[" => 0x2f,
        "]" => 0x30,
        "\\" => 0x31,
        ";" => 0x33,
        "'" => 0x34,
        "," => 0x36,
        "." => 0x37,
        "/" => 0x38,
        "0" => 0x27,
        "PrtSc" => 0x46,
        "SrcLock" => 0x47,
        "Pause" => 0x48,
        "Insert" => 0x49,
        "Home" => 0x4a,
        "PgUp" => 0x4b,
        "Delete" => 0x4c,
        "End" => 0x4d,
        "PgDn" => 0x4e,
        "Right" => 0x4f,
        "Left" => 0x50,
        "Down" => 0x51,
        "Up" => 0x52,
        "App" => 0x65,
        n if n.len() == 1 && n.as_bytes()[0].is_ascii_uppercase() => n.as_bytes()[0] - b'A' + 4,
        n if n.len() == 1 && (b'1'..=b'9').contains(&n.as_bytes()[0]) => {
            n.as_bytes()[0] - b'1' + 0x1e
        }
        n if n.starts_with('F') => {
            let number: u8 = n[1..].parse().ok()?;
            if !(1..=12).contains(&number) {
                return None;
            }
            number + 0x39
        }
        _ => return None,
    };
    Some([0, 0, 0, hid])
}

pub fn keyboard_choices() -> Vec<(&'static str, [u8; 4])> {
    aula_f87pro::KEYS
        .iter()
        .filter_map(|(name, _)| standard(name).map(|code| (*name, code)))
        .collect()
}

pub fn describe(code: [u8; 4]) -> String {
    if code == [0; 4] {
        return t("Desativada", "Disabled").into();
    }
    if code == [13, 0, 0, 0] {
        return t("Função Fn", "Fn function").into();
    }
    if code[0] == 3 {
        return format!("Macro {}", u16::from(code[3]) + 1);
    }
    if let Some((name, _)) = media().iter().find(|(_, value)| *value == code) {
        return (*name).into();
    }
    if code[0] != 0 {
        return t("Função específica do teclado", "Keyboard-specific function").into();
    }
    let mut parts: Vec<String> = [
        (1, "Ctrl"),
        (2, "Shift"),
        (4, "Alt"),
        (8, "Super"),
        (16, t("Ctrl direito", "Right Ctrl")),
        (32, t("Shift direito", "Right Shift")),
        (64, t("Alt direito", "Right Alt")),
        (128, t("Super direito", "Right Super")),
    ]
    .iter()
    .filter(|(bit, _)| code[1] & bit != 0)
    .map(|(_, name)| (*name).to_owned())
    .collect();
    for hid in [code[2], code[3]].into_iter().filter(|&n| n != 0) {
        parts.push(
            aula_f87pro::KEYS
                .iter()
                .find(|(name, _)| standard(name) == Some([0, 0, 0, hid]))
                .map(|(name, _)| (*name).into())
                .unwrap_or_else(|| t("Tecla especial", "Special key").into()),
        );
    }
    if parts.is_empty() {
        t("Função específica do teclado", "Keyboard-specific function").into()
    } else {
        parts.join(" + ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modifiers_and_standard_keys_match_the_real_matrix() {
        let matrix = include_bytes!("../../tests/fixtures/f87pro-matrix-layer0.bin");
        for (name, slot) in aula_f87pro::KEYS {
            if let Some(code) = standard(name) {
                assert_eq!(code, matrix[slot * 4..slot * 4 + 4], "{name}");
            }
        }
        assert_eq!(describe([0, 3, 0, 4]), "Ctrl + Shift + A");
        assert_eq!(describe([2, 0, 0, 0xe2]), "Silenciar");
        assert_eq!(describe([3, 1, 1, 0]), "Macro 1");
        assert_eq!(describe([0; 4]), "Desativada");
    }
}
