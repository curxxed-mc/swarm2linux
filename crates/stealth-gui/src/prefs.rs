//! The app's own preferences, in `~/.config/swarm2linux/gui.toml`, and starting with the session through
//! `~/.config/autostart`. Headset settings live on the headset.

use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Preferences {
    /// The equalizer's fine grid.
    pub show_grid: bool,
    /// The tray icon, and closing the window to it.
    pub tray: bool,
    /// The colour theme's name, or "Follow system".
    pub theme: String,
}

impl Default for Preferences {
    fn default() -> Self {
        Self { show_grid: false, tray: true, theme: crate::theme::DEFAULT.into() }
    }
}

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn path() -> Option<PathBuf> {
    Some(config_dir()?.join("swarm2linux").join("gui.toml"))
}

fn autostart_entry() -> Option<PathBuf> {
    Some(config_dir()?.join("autostart").join("io.github.swarm2linux.desktop"))
}

/// Whether the app starts with the session.
pub fn autostart() -> bool {
    autostart_entry().is_some_and(|p| p.exists())
}

/// Start the app (this binary, with `--hidden`) with the session, or stop doing so.
pub fn set_autostart(on: bool) -> io::Result<()> {
    let entry = autostart_entry().ok_or_else(|| io::Error::other("no home directory"))?;
    if !on {
        return match fs::remove_file(&entry) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe()?;
    let exe = exe.to_str().ok_or_else(|| io::Error::other("the program's path is not UTF-8"))?;
    let exec = exec_line(exe)?;
    if let Some(dir) = entry.parent() {
        fs::create_dir_all(dir)?;
    }
    let partial = entry.with_extension("desktop.new");
    fs::write(
        &partial,
        format!(
            "[Desktop Entry]\nType=Application\nName=swarm2linux\nComment=Stealth Pro II settings in the tray\n\
             Exec={exec}\nIcon=io.github.swarm2linux\nTerminal=false\nX-GNOME-Autostart-enabled=true\n"
        ),
    )?;
    fs::rename(partial, entry)
}

/// The autostart entry's `Exec=` value for the program at `exe`. Linux names a program that was replaced while
/// running "<path> (deleted)"; the entry should start the new one at the path.
fn exec_line(exe: &str) -> io::Result<String> {
    let exe = exe.strip_suffix(" (deleted)").unwrap_or(exe);
    // these would need escaping inside the quotes, which not every desktop reads the same way
    if exe.contains(['"', '`', '$', '\\']) || exe.chars().any(char::is_control) {
        return Err(io::Error::other(format!("cannot start {exe} from an autostart entry")));
    }
    // % starts a field code (%f, %u, ...) even inside quotes
    Ok(format!("\"{}\" --hidden", exe.replace('%', "%%")))
}

/// The saved preferences, or the defaults.
pub fn load() -> Preferences {
    let Some(path) = path() else { return Preferences::default() };
    match fs::read_to_string(&path) {
        Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
            eprintln!("stealth-gui: ignoring {}: {e}", path.display());
            Preferences::default()
        }),
        Err(_) => Preferences::default(),
    }
}

pub fn save(prefs: &Preferences) -> io::Result<()> {
    let path = path().ok_or_else(|| io::Error::other("no home directory"))?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let text = toml::to_string(prefs).map_err(io::Error::other)?;
    let partial = path.with_extension("toml.new");
    fs::write(&partial, text)?;
    fs::rename(partial, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_lines() {
        assert_eq!(exec_line("/home/a b/.local/bin/stealth-gui").unwrap(), "\"/home/a b/.local/bin/stealth-gui\" --hidden");
        assert_eq!(exec_line("/x/stealth-gui (deleted)").unwrap(), "\"/x/stealth-gui\" --hidden");
        assert_eq!(exec_line("/home/100%/stealth-gui").unwrap(), "\"/home/100%%/stealth-gui\" --hidden");
        assert!(exec_line("/home/a\"b/stealth-gui").is_err());
        assert!(exec_line("/home/a\rb/stealth-gui").is_err());
    }

    #[test]
    fn round_trips_and_fills_gaps() {
        let p = Preferences { show_grid: true, tray: false, theme: "Nord".into() };
        assert_eq!(toml::from_str::<Preferences>(&toml::to_string(&p).unwrap()).unwrap(), p);
        assert_eq!(
            toml::from_str::<Preferences>("show_grid = true").unwrap(),
            Preferences { show_grid: true, tray: true, theme: "Swarm".into() }
        );
    }
}
