//! The self-diagnostic behind `stealthctl doctor` and the desktop app's Diagnostics card: it checks what the app
//! needs, from the USB device to the files in `~/.config/swarm2linux`, and says how to fix what is wrong.
//!
//! It never writes to the headset. With `read_device` and no stealthd running, it opens the device and reads its
//! records (the same read-only requests stealthd makes when it starts).

use std::collections::BTreeSet;
use std::fs::{self, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use race_hid::{DeviceInfo, KNOWN_PIDS, discover, known_product};
use stealth_dbus::DaemonClient;
use tb_proto::profile::{self, Profile};
use tb_proto::values::{self, Typed};

/// The version the checks expect stealthd to run.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Where the udev rule can be installed.
const RULE_PATHS: [&str; 3] =
    ["/etc/udev/rules.d/70-swarm2linux.rules", "/usr/lib/udev/rules.d/70-swarm2linux.rules", "/lib/udev/rules.d/70-swarm2linux.rules"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    /// Something worth knowing (firmware, battery).
    Info,
    Ok,
    Warn,
    Fail,
}

impl Level {
    pub fn symbol(self) -> &'static str {
        match self {
            Level::Info => "·",
            Level::Ok => "✓",
            Level::Warn => "!",
            Level::Fail => "✗",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    pub group: &'static str,
    pub level: Level,
    pub title: String,
    /// What to do about it, or more detail.
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    pub checks: Vec<Check>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Open the device and read it when stealthd is not running.
    pub read_device: bool,
}

impl Report {
    fn add(&mut self, group: &'static str, level: Level, title: impl Into<String>, hint: Option<String>) {
        self.checks.push(Check { group, level, title: hide_home(&title.into()), hint: hint.map(|h| hide_home(&h)) });
    }

    /// The worst level of all checks.
    pub fn worst(&self) -> Level {
        self.checks.iter().map(|c| c.level).max().unwrap_or(Level::Ok)
    }

    /// A plain-text report for a bug report: the checks by group, with fixes. It names no addresses or serial
    /// numbers, and the home folder as `~`.
    pub fn text(&self, program: &str) -> String {
        let mut out = format!("swarm2linux doctor ({program} {VERSION})\n");
        let mut group = "";
        for c in &self.checks {
            if c.group != group {
                group = c.group;
                out.push_str(&format!("\n{group}\n"));
            }
            out.push_str(&format!("  {} {}\n", c.level.symbol(), c.title));
            if let Some(hint) = &c.hint {
                for line in hint.lines() {
                    out.push_str(&format!("      {line}\n"));
                }
            }
        }
        let (fails, warns) = (self.count(Level::Fail), self.count(Level::Warn));
        out.push_str(&match (fails, warns) {
            (0, 0) => "\nEverything looks fine.\n".to_string(),
            _ => format!("\n{fails} problem(s), {warns} warning(s).\n"),
        });
        out
    }

    fn count(&self, level: Level) -> usize {
        self.checks.iter().filter(|c| c.level == level).count()
    }
}

/// Run every check.
pub fn run(options: Options) -> Report {
    let mut r = Report::default();
    let usable = device(&mut r);
    let openable = permissions(&mut r, &usable);
    let daemon = service(&mut r, !usable.is_empty());
    match &daemon {
        Some(d) => headset_via_daemon(&mut r, d),
        None if options.read_device => match openable.first() {
            Some(dev) => headset_direct(&mut r, dev),
            None => r.add("Headset", Level::Info, "Not checked: no device that can be opened", None),
        },
        None => r.add("Headset", Level::Info, "Not checked: stealthd is not running", None),
    }
    volume(&mut r, daemon.as_ref(), openable.first());
    files(&mut r);
    r
}

// ---- device ----

/// Turtle Beach USB devices: (product id, product name).
fn usb_devices() -> Vec<(u16, String)> {
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else { return Vec::new() };
    let read = |dir: &Path, file: &str| fs::read_to_string(dir.join(file)).ok().map(|s| s.trim().to_string());
    let mut out: Vec<(u16, String)> = entries
        .flatten()
        .filter_map(|e| {
            let dir = e.path();
            if read(&dir, "idVendor")?.to_lowercase() != "10f5" {
                return None;
            }
            let pid = u16::from_str_radix(&read(&dir, "idProduct")?, 16).ok()?;
            Some((pid, read(&dir, "product").unwrap_or_default()))
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// The devices the app can use: known products with the vendor collection.
fn device(r: &mut Report) -> Vec<DeviceInfo> {
    const G: &str = "Device";
    let usb = usb_devices();
    let hid = discover().unwrap_or_default();
    if usb.is_empty() && hid.is_empty() {
        r.add(G, Level::Fail, "No Turtle Beach device is plugged in", Some("Plug in the USB transmitter or the headset's USB-C cable.".into()));
        return Vec::new();
    }
    for (pid, name) in &usb {
        match known_product(*pid) {
            Some(product) => r.add(G, Level::Ok, format!("10f5:{pid:04x} {product}"), None),
            None => r.add(
                G,
                Level::Warn,
                format!("10f5:{pid:04x} {name} is not a known Stealth Pro II"),
                Some(format!(
                    "The app leaves it alone. If it is a Stealth Pro II, report its ID with the output of `lsusb -v -d 10f5:{pid:04x}`."
                )),
            ),
        }
    }
    let usable: Vec<DeviceInfo> =
        hid.iter().filter(|d| d.vendor_collection && known_product(d.pid).is_some()).cloned().collect();
    for (pid, _) in usb.iter().filter(|(pid, _)| known_product(*pid).is_some()) {
        if !usable.iter().any(|d| d.pid == *pid) {
            r.add(
                G,
                Level::Warn,
                format!("10f5:{pid:04x} shows no control interface"),
                Some("Its HID interface is missing from /sys/class/hidraw. Unplug it and plug it in again.".into()),
            );
        }
    }
    usable
}

// ---- permissions ----

/// The product ids a udev rule file grants access to.
fn rule_pids(text: &str) -> BTreeSet<u16> {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#') && l.contains("10f5"))
        .filter_map(|l| {
            let at = l.find("ATTRS{idProduct}==\"")? + "ATTRS{idProduct}==\"".len();
            u16::from_str_radix(l.get(at..at + 4)?, 16).ok()
        })
        .collect()
}

/// The devices that can be opened.
fn permissions(r: &mut Report, usable: &[DeviceInfo]) -> Vec<DeviceInfo> {
    const G: &str = "Permissions";
    let install = "sudo install -Dm644 packaging/70-swarm2linux.rules /etc/udev/rules.d/70-swarm2linux.rules\n\
                   sudo udevadm control --reload && sudo udevadm trigger --subsystem-match=hidraw --action=change";
    match RULE_PATHS.iter().find(|p| Path::new(p).exists()) {
        None => r.add(G, Level::Warn, "The udev rule is not installed", Some(format!("Without it only root can use the device. From the source folder:\n{install}"))),
        Some(path) => {
            let granted = fs::read_to_string(path).map(|t| rule_pids(&t)).unwrap_or_default();
            let missing: Vec<u16> = KNOWN_PIDS.iter().copied().filter(|p| !granted.contains(p)).collect();
            if missing.is_empty() {
                r.add(G, Level::Ok, format!("The udev rule covers every known device ({path})"), None);
            } else {
                let lines: Vec<String> = missing
                    .iter()
                    .map(|p| format!("KERNEL==\"hidraw*\", ATTRS{{idVendor}}==\"10f5\", ATTRS{{idProduct}}==\"{p:04x}\", TAG+=\"uaccess\""))
                    .collect();
                let ids: Vec<String> = missing.iter().map(|p| format!("10f5:{p:04x}")).collect();
                r.add(
                    G,
                    Level::Warn,
                    format!("The udev rule in {path} is missing {}", ids.join(", ")),
                    Some(format!("Add these lines to it, or install the current rule:\n{}\n{install}", lines.join("\n"))),
                );
            }
        }
    }
    let mut openable = Vec::new();
    for d in usable {
        // opening without reading or writing is harmless, even while stealthd has it open
        match OpenOptions::new().read(true).write(true).open(&d.path) {
            Ok(_) => {
                r.add(G, Level::Ok, format!("You can open {}", d.path.display()), None);
                openable.push(d.clone());
            }
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => r.add(
                G,
                Level::Fail,
                format!("You cannot open {}: permission denied", d.path.display()),
                Some(format!("Install the udev rule, then unplug the device and plug it in again:\n{install}")),
            ),
            Err(e) => r.add(G, Level::Fail, format!("Cannot open {}: {e}", d.path.display()), None),
        }
    }
    openable
}

// ---- service ----

/// Run a program, waiting at most a few seconds; its output, or `None` if it is missing or hangs.
fn output(program: &str, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program).args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let mut out = String::new();
    io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
    Some(out.trim().to_string())
}

fn service(r: &mut Report, device_present: bool) -> Option<DaemonClient> {
    const G: &str = "Service";
    let why = "journalctl --user -u stealthd -n 30 shows why it stopped.";
    if let Some(active) = output("systemctl", &["--user", "is-active", "stealthd"]) {
        let enabled = output("systemctl", &["--user", "is-enabled", "stealthd"]).unwrap_or_default();
        match active.as_str() {
            "active" => r.add(G, Level::Ok, "The stealthd service is running", None),
            "failed" => r.add(G, Level::Fail, "The stealthd service has failed", Some(format!("systemctl --user restart stealthd\n{why}"))),
            _ if enabled.is_empty() || enabled == "not-found" => r.add(
                G,
                Level::Warn,
                "The stealthd service is not installed",
                Some("install.sh installs it; or start stealthd by hand.".into()),
            ),
            _ => r.add(
                G,
                if device_present { Level::Fail } else { Level::Warn },
                format!("The stealthd service is {active}"),
                Some(format!("systemctl --user enable --now stealthd\n{why}")),
            ),
        }
        if enabled == "disabled" {
            r.add(G, Level::Warn, "The stealthd service does not start with the session", Some("systemctl --user enable stealthd".into()));
        }
    }
    let client = match DaemonClient::connect() {
        Ok(Some(c)) => c,
        Ok(None) => {
            r.add(G, Level::Fail, "stealthd is not on the session bus", Some("The apps need it: systemctl --user start stealthd".into()));
            return None;
        }
        Err(e) => {
            r.add(G, Level::Fail, format!("Cannot reach the session bus: {e}"), None);
            return None;
        }
    };
    match client.version() {
        Ok(v) if v == VERSION => r.add(G, Level::Ok, format!("stealthd {v} answers"), None),
        Ok(v) => r.add(
            G,
            Level::Warn,
            format!("stealthd is version {v}, this program {VERSION}"),
            Some("Restart it after updating: systemctl --user restart stealthd".into()),
        ),
        Err(_) => r.add(
            G,
            Level::Warn,
            format!("stealthd is older than {VERSION}"),
            Some("Restart it after updating: systemctl --user restart stealthd".into()),
        ),
    }
    // Linux marks the program of a process whose file was replaced since it started
    if let Ok(pid) = client.pid()
        && fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|p| p.to_string_lossy().ends_with(" (deleted)"))
    {
        r.add(
            G,
            Level::Warn,
            "stealthd was updated but is still running the old program",
            Some("systemctl --user restart stealthd".into()),
        );
    }
    Some(client)
}

// ---- headset ----

fn int(v: &std::collections::BTreeMap<String, Typed>, name: &str) -> Option<i64> {
    match v.get(name) {
        Some(Typed::Int(n)) => Some(*n),
        _ => None,
    }
}

fn text<'a>(v: &'a std::collections::BTreeMap<String, Typed>, name: &str) -> Option<&'a str> {
    match v.get(name) {
        Some(Typed::Text(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

/// The headset checks from its values, by name as `tb_proto::values` gives them.
fn headset_values(r: &mut Report, v: &std::collections::BTreeMap<String, Typed>, link: Option<bool>, how: &str) {
    const G: &str = "Headset";
    if link == Some(false) {
        r.add(G, Level::Warn, format!("The headset is off or out of range{how}"), Some("Switch it on; its link to the transmitter comes up within a few seconds.".into()));
        return;
    }
    r.add(G, Level::Ok, format!("The headset answers{how}"), None);
    if let Some(fw) = text(v, "firmware") {
        r.add(G, Level::Info, format!("Firmware {fw}"), None);
    }
    let charging = v.get("charging") == Some(&Typed::Bool(true));
    match int(v, "battery") {
        Some(b) if b < 0 => r.add(G, Level::Info, "Battery level not known yet", None),
        Some(b) if b <= 15 && !charging => r.add(G, Level::Warn, format!("Battery {b}%"), Some("Charge the headset or swap the battery.".into())),
        Some(b) => r.add(G, Level::Info, format!("Battery {b}%{}", if charging { ", charging" } else { "" }), None),
        None => {}
    }
    if let Some(Typed::Texts(tx)) = v.get("transmitters") {
        let lines: Vec<String> = tx.iter().map(|t| format!("TX{t}")).collect();
        if !lines.is_empty() {
            r.add(G, Level::Info, format!("CrossPlay: {}", lines.join(", ")), None);
        }
    }
}

fn headset_via_daemon(r: &mut Report, d: &DaemonClient) {
    const G: &str = "Headset";
    let v = match d.values() {
        Ok(v) => v,
        Err(e) => return r.add(G, Level::Fail, format!("stealthd did not answer: {}", stealth_dbus::link::describe(&e)), None),
    };
    if v.get("connected") != Some(&Typed::Bool(true)) {
        return r.add(G, Level::Fail, "stealthd has no device", Some("Plug in the transmitter; stealthd picks it up within a few seconds.".into()));
    }
    let link = match v.get("headset_link") {
        Some(Typed::Bool(b)) => Some(*b),
        _ => None,
    };
    headset_values(r, &v, link, "");
    if let Some(profile) = text(&v, "active_profile") {
        let modified = v.get("profile_modified") == Some(&Typed::Bool(true));
        r.add(G, Level::Info, format!("Profile {profile}{}", if modified { " (modified)" } else { "" }), None);
    }
}

fn headset_direct(r: &mut Report, dev: &DeviceInfo) {
    const G: &str = "Headset";
    let how = " (read directly, as stealthd is not running)";
    let mut client = match tb_proto::open(Some(&dev.path)) {
        Ok(c) => c,
        Err(e) => return r.add(G, Level::Fail, format!("The device did not answer: {e}"), Some("Unplug it and plug it in again; close any other program using it.".into())),
    };
    let state = match client.subscribe_all() {
        Ok((state, _)) => state,
        Err(e) => return r.add(G, Level::Fail, format!("The device did not answer: {e}"), None),
    };
    let v: std::collections::BTreeMap<String, Typed> =
        values::values(&state).into_iter().map(|(k, v)| (k.to_string(), v)).collect();
    let link = client.headset_link().or_else(|| int(&v, "active_transmitter").map(|t| t > 0));
    headset_values(r, &v, link, how);
}

// ---- volume ----

fn on_path(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

fn volume(r: &mut Report, daemon: Option<&DaemonClient>, device: Option<&DeviceInfo>) {
    const G: &str = "Volume";
    let missing: Vec<&str> = ["pw-dump", "wpctl"].into_iter().filter(|p| !on_path(p)).collect();
    if !missing.is_empty() {
        r.add(G, Level::Warn, format!("{} not found", missing.join(" and ")), Some("They come with PipeWire (on Debian and Ubuntu: wireplumber and pipewire-bin). Without them the volume is shown but can't be changed.".into()));
        return;
    }
    r.add(G, Level::Ok, "pw-dump and wpctl are installed", None);
    let found = match (daemon, device) {
        (Some(d), _) => d.values().ok().and_then(|v| int(&v, "sink_volume")).map(|p| p >= 0),
        (None, Some(dev)) => sink_volume::sound_card(&dev.path).map(|card| sink_volume::find(card).ok().flatten().is_some()),
        (None, None) => None,
    };
    match found {
        Some(true) => r.add(G, Level::Ok, "PipeWire has an output for the headset", None),
        Some(false) => r.add(G, Level::Warn, "PipeWire has no output for the headset", Some("Check that the headset shows up in the system's sound settings.".into())),
        None => {}
    }
}

// ---- files ----

fn config_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
}

fn files(r: &mut Report) {
    const G: &str = "Files";
    let Some(config) = config_dir() else { return };
    let dir = config.join("swarm2linux");
    for name in ["stealthd.toml", "gui.toml"] {
        let path = dir.join(name);
        if let Ok(text) = fs::read_to_string(&path)
            && let Err(e) = toml::from_str::<toml::Table>(&text)
        {
            r.add(G, Level::Warn, format!("{} cannot be read; its defaults are used", path.display()), Some(e.message().to_string()));
        }
    }
    let profiles = dir.join("profiles");
    let mut good = 0;
    if let Ok(entries) = fs::read_dir(&profiles) {
        for path in entries.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "toml")) {
            let name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
            let problem = if name.is_empty() || !profile::name_problem(name).is_empty() {
                Some("not a usable profile name".to_string())
            } else {
                match fs::read_to_string(&path).map_err(|e| e.to_string()).and_then(|t| toml::from_str::<Profile>(&t).map_err(|e| e.message().to_string())) {
                    Ok(p) => p.problem(),
                    Err(e) => Some(e),
                }
            };
            match problem {
                Some(why) => r.add(G, Level::Warn, format!("The profile {} is skipped", path.display()), Some(why)),
                None => good += 1,
            }
        }
    }
    r.add(G, Level::Ok, format!("{good} profile(s) in {}", profiles.display()), None);
    let autostart = config.join("autostart").join("io.github.swarm2linux.desktop");
    if let Ok(entry) = fs::read_to_string(&autostart) {
        let exec = entry.lines().find_map(|l| l.strip_prefix("Exec=")).and_then(|e| e.strip_prefix('"')?.split('"').next());
        match exec {
            Some(program) if Path::new(&program.replace("%%", "%")).is_file() => r.add(G, Level::Ok, "The app starts with the session", None),
            Some(program) => r.add(
                G,
                Level::Warn,
                format!("The autostart entry starts {program}, which does not exist"),
                Some("Turn Start with the session off and on again on the Device page.".into()),
            ),
            None => {}
        }
    }
}

/// `text` with the home folder written as `~`.
fn hide_home(text: &str) -> String {
    match std::env::var("HOME") {
        Ok(home) if home.len() > 1 => text.replace(&home, "~"),
        _ => text.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_udev_rule() {
        let rule = include_str!("../../../packaging/70-swarm2linux.rules");
        assert_eq!(rule_pids(rule), KNOWN_PIDS.iter().copied().collect(), "the shipped rule covers every known device");
        assert!(rule_pids("# KERNEL==\"hidraw*\", ATTRS{idVendor}==\"10f5\", ATTRS{idProduct}==\"2287\"").is_empty());
    }

    #[test]
    fn known_pids_are_the_known_products() {
        assert!(KNOWN_PIDS.iter().all(|&p| known_product(p).is_some()));
        assert_eq!((0..=u16::MAX).filter(|&p| known_product(p).is_some()).count(), KNOWN_PIDS.len());
    }

    #[test]
    fn reports() {
        let mut r = Report::default();
        r.add("Device", Level::Ok, "10f5:2288 Stealth Pro II transmitter", None);
        r.add("Service", Level::Fail, "stealthd is not on the session bus", Some("systemctl --user start stealthd".into()));
        assert_eq!(r.worst(), Level::Fail);
        let text = r.text("stealthctl");
        assert!(text.contains("\nDevice\n  ✓ 10f5:2288"));
        assert!(text.contains("      systemctl --user start stealthd\n"));
        assert!(text.ends_with("1 problem(s), 0 warning(s).\n"));
    }
}
