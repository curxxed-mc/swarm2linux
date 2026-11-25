//! Profiles on disk: one TOML file per profile in `~/.config/swarm2linux/profiles/`, named after it
//! (`Gaming.toml`). They are read again whenever a profile is saved, applied or deleted, so a file edited by
//! hand counts from then on.

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::PathBuf;

use tb_proto::profile::{self, Profile};

use crate::config_dir;

const HEADER: &str = "# A swarm2linux profile: apply it from the app, the tray or `stealthctl profile apply <name>`.\n\
                      # Settings use the value names of `stealthctl status`; EQ bands are in tenths of a dB.\n\n";

fn dir() -> io::Result<PathBuf> {
    config_dir().map(|c| c.join("profiles")).ok_or_else(|| io::Error::other("no home directory"))
}

fn path(name: &str) -> io::Result<PathBuf> {
    if name.is_empty() || !profile::name_problem(name).is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{name:?} cannot name a profile")));
    }
    Ok(dir()?.join(format!("{name}.toml")))
}

/// Every readable profile by name. Files that don't parse or hold values the headset doesn't take are skipped
/// with a message.
pub fn load() -> BTreeMap<String, Profile> {
    let mut out = BTreeMap::new();
    let Ok(entries) = dir().and_then(fs::read_dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "toml") {
            continue;
        }
        let Some(name) = path.file_stem().and_then(|s| s.to_str()).map(str::to_string) else { continue };
        if name.is_empty() || !profile::name_problem(&name).is_empty() {
            eprintln!("stealthd: ignoring {}: not a usable profile name", path.display());
            continue;
        }
        let parsed = fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| toml::from_str::<Profile>(&text).map_err(|e| e.to_string()))
            .and_then(|p| match p.problem() {
                Some(problem) => Err(problem),
                None => Ok(p),
            });
        match parsed {
            Ok(profile) => {
                out.insert(name, profile);
            }
            Err(e) => eprintln!("stealthd: ignoring the profile {}: {e}", path.display()),
        }
    }
    out
}

/// Save a profile, replacing one of the same name.
pub fn save(name: &str, profile: &Profile) -> io::Result<()> {
    fs::create_dir_all(dir()?)?;
    let path = path(name)?;
    let text = toml::to_string(profile).map_err(io::Error::other)?;
    let partial = path.with_extension("toml.new");
    fs::write(&partial, format!("{HEADER}{text}"))?;
    fs::rename(partial, path)
}

pub fn delete(name: &str) -> io::Result<()> {
    fs::remove_file(path(name)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tb_proto::profile::{EqSetting, Setting};

    #[test]
    fn round_trips_through_toml() {
        let mut p = Profile::default();
        p.settings.insert("anc".into(), Setting::Bool(true));
        p.settings.insert("anc_level".into(), Setting::Int(60));
        p.game_eq = Some(EqSetting { preset: Some("Bass Boost".into()), bands: vec![50, 50, 35, 0, 0, 0, 0, 0, 0, 0] });
        p.mic_eq = Some(EqSetting { preset: None, bands: vec![0; 10] });
        let text = format!("{HEADER}{}", toml::to_string(&p).unwrap());
        assert!(text.contains("anc = true"), "{text}");
        assert!(text.contains("[game_eq]"), "{text}");
        assert_eq!(toml::from_str::<Profile>(&text).unwrap(), p);
    }

    #[test]
    fn reads_a_hand_written_file() {
        let p: Profile = toml::from_str("anc = false\nnoise_gate_level = 40\n").unwrap();
        assert_eq!(p.settings["anc"], Setting::Bool(false));
        assert_eq!(p.settings["noise_gate_level"], Setting::Int(40));
        assert_eq!(p.game_eq, None);
        assert_eq!(p.problem(), None);
    }
}
