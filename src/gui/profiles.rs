use aula_f87pro::state::Snapshot;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub name: String,
    pub state: Snapshot,
}

fn validate(name: &str, state: &Snapshot) -> io::Result<()> {
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(char::is_control) {
        return Err(io::Error::other(
            "Use um nome de perfil com 1 a 80 caracteres, sem quebras de linha.",
        ));
    }
    state.validate()
}

pub fn read_bytes(path: &Path) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    File::open(path)?
        .take(96 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 96 * 1024 {
        return Err(io::Error::other(
            "Arquivo grande demais para um perfil F87 Pro.",
        ));
    }
    Ok(bytes)
}

pub fn read(path: &Path) -> io::Result<Profile> {
    let profile: Profile = serde_json::from_slice(&read_bytes(path)?)?;
    validate(&profile.name, &profile.state)?;
    Ok(profile)
}

pub fn entries(dir: &Path) -> io::Result<Vec<(String, PathBuf)>> {
    fs::create_dir_all(dir)?;
    let mut profiles = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if entry.path().extension().is_some_and(|s| s == "json")
            && let Ok(profile) = read(&entry.path())
        {
            profiles.push((profile.name, entry.path()));
        }
    }
    profiles.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(profiles)
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("Pasta de destino inválida."))?;
    fs::create_dir_all(parent)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let temporary = parent.join(format!(".aula-{stamp}.tmp"));
    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub fn export(path: &Path, name: &str, state: &Snapshot) -> io::Result<()> {
    validate(name, state)?;
    let profile = Profile {
        name: name.trim().into(),
        state: state.clone(),
    };
    atomic_write(path, &serde_json::to_vec_pretty(&profile)?)
}

pub fn save(dir: &Path, name: &str, state: &Snapshot) -> io::Result<PathBuf> {
    validate(name, state)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(io::Error::other)?
        .as_nanos();
    let path = entries(dir)?
        .into_iter()
        .find(|(n, _)| n == name.trim())
        .map(|(_, path)| path)
        .unwrap_or_else(|| dir.join(format!("profile-{stamp}.json")));
    export(&path, name, state)?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn state() -> Snapshot {
        Snapshot {
            version: 1,
            identity: vec![3, 0, 0, 0, 1, 11],
            leds: include_bytes!("../../tests/fixtures/f87pro-led-read.bin").to_vec(),
            layers: vec![vec![0; 512]; 4],
            macros: vec![0; 4096],
        }
    }

    #[test]
    fn names_cannot_escape_the_store_and_updates_preserve_state() {
        let dir = std::env::temp_dir().join(format!(
            "aula-profiles-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let original = state();
        let path = save(&dir, "Trabalho / Jogos", &original).unwrap();
        assert_eq!(path.parent(), Some(dir.as_path()));
        assert_eq!(read(&path).unwrap().state, original);
        let mut changed = original.clone();
        changed.layers[2][12] = 17;
        assert_eq!(save(&dir, "Trabalho / Jogos", &changed).unwrap(), path);
        assert_eq!(entries(&dir).unwrap().len(), 1);
        assert_eq!(read(&path).unwrap().state, changed);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn rejects_invalid_profile_data_and_names() {
        let mut invalid = state();
        invalid.identity[0] = 9;
        assert!(validate("Perfil", &invalid).is_err());
        assert!(validate(" ", &state()).is_err());
        assert!(validate("Nome\ninválido", &state()).is_err());
        assert!(validate(&"x".repeat(81), &state()).is_err());
    }
}
