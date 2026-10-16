use super::profiles::atomic_write;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File},
    io::{self, Read},
    path::Path,
};

pub const AUTOSTART_FILE: &str = "io.github.aula_f87pro.Control.desktop";

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct Preferences {
    pub english: bool,
    pub minimize_on_close: bool,
}

fn locale_is_english(locale: &str) -> bool {
    !locale
        .split(['_', '-', '.', ':'])
        .next()
        .is_some_and(|language| language.eq_ignore_ascii_case("pt"))
}

impl Default for Preferences {
    fn default() -> Self {
        let locale = ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
            .iter()
            .find_map(|key| std::env::var(key).ok().filter(|value| !value.is_empty()))
            .unwrap_or_default();
        Self {
            english: locale_is_english(&locale),
            minimize_on_close: false,
        }
    }
}

impl Preferences {
    pub fn load(path: &Path) -> io::Result<Self> {
        let file = match File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e),
        };
        let mut bytes = Vec::new();
        file.take(4097).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "settings file exceeds 4096 bytes",
            ));
        }
        Ok(serde_json::from_slice(&bytes)?)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        atomic_write(path, &serde_json::to_vec_pretty(self)?)
    }
}

pub fn autostart_enabled(config: &Path) -> bool {
    fs::read_to_string(config.join("autostart").join(AUTOSTART_FILE))
        .is_ok_and(|text| !text.lines().any(|line| line.trim() == "Hidden=true"))
}

pub fn set_autostart(config: &Path, executable: &Path, enabled: bool) -> io::Result<()> {
    let path = config.join("autostart").join(AUTOSTART_FILE);
    if !enabled {
        return match fs::remove_file(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    }
    let executable = executable
        .to_str()
        .filter(|s| !s.contains(['\n', '\r', '\0', '=']))
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid desktop executable path",
            )
        })?;
    if !Path::new(executable).is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "desktop executable path must be absolute",
        ));
    }
    // Desktop entries unescape string values before unquoting Exec arguments.
    let quoted = executable
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('`', "\\`")
        .replace('$', "\\$")
        .replace('%', "%%")
        .replace('\\', "\\\\");
    atomic_write(&path, format!("[Desktop Entry]\nType=Application\nName=AULA F87 Pro\nExec=\"{quoted}\"\nIcon=aula-f87pro\nTerminal=false\n").as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn preferences_and_autostart_are_local_reversible_and_validate_input() {
        let root = std::env::temp_dir().join(format!(
            "aula-settings-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = root.join("aula-f87pro/settings.json");
        assert_eq!(
            Preferences::load(&file).unwrap().english,
            Preferences::default().english
        );
        assert!(!locale_is_english("pt_BR.UTF-8"));
        assert!(!locale_is_english("pt-PT:en"));
        assert!(locale_is_english("en_US.UTF-8"));
        assert!(locale_is_english("C.UTF-8"));
        let settings = Preferences {
            english: true,
            minimize_on_close: true,
        };
        settings.save(&file).unwrap();
        let read = Preferences::load(&file).unwrap();
        assert!(read.english && read.minimize_on_close);
        fs::write(&file, br#"{"future":true}"#).unwrap();
        assert!(Preferences::load(&file).is_err());
        let exe = Path::new("/tmp/aula app/$special%\\\"/aula-f87pro-gui");
        set_autostart(&root, exe, true).unwrap();
        assert!(autostart_enabled(&root));
        let desktop = root.join("autostart").join(AUTOSTART_FILE);
        let entry = gtk::glib::KeyFile::new();
        entry
            .load_from_file(&desktop, gtk::glib::KeyFileFlags::NONE)
            .unwrap();
        let command = entry.string("Desktop Entry", "Exec").unwrap();
        let arguments = gtk::glib::shell_parse_argv(&command).unwrap();
        assert_eq!(arguments.len(), 1);
        assert_eq!(
            arguments[0].to_str().unwrap().replace("%%", "%"),
            exe.to_str().unwrap()
        );
        set_autostart(&root, exe, false).unwrap();
        assert!(!autostart_enabled(&root));
        assert!(set_autostart(&root, Path::new("relative"), true).is_err());
        assert!(set_autostart(&root, Path::new("/tmp/bad\nName=Injected"), true).is_err());
        fs::remove_dir_all(root).unwrap();
    }
}
