//! `stealthd`: owns the Stealth Pro II and serves it on the session bus.
//!
//! One thread owns the device: it finds it, follows unplugging, keeps a session with every record subscribed and
//! carries out writes. D-Bus calls reach it through a queue; changes go out as the `Changed` signal.
//!
//! Bus name `io.github.swarm2linux.Stealthd`, object `/io/github/swarm2linux/Headset`, interface
//! `io.github.swarm2linux.Headset1`. Value names are those of `tb_proto::values`.
//!
//! Its own settings are kept in `~/.config/swarm2linux/stealthd.toml`, profiles in `~/.config/swarm2linux/profiles/`
//! (see `profiles`).
//!
//! Besides the headset's values it serves `sink_volume`: the headset's volume in PipeWire, which it reads and sets
//! through the `sink-volume` crate, and `profiles` (the saved profiles' names), `active_profile` (the selected
//! profile, or "": the one last applied or saved, kept across restarts) and `profile_modified` (whether the headset's
//! settings differ from the selected profile).

mod osd;
mod profiles;

use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::sleep;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use race_hid::{HidrawDevice, discover, known_product};
use serde::{Deserialize, Serialize};
use sink_volume::Sink;
use stealth_dbus::{BUS_NAME, INTERFACE, PATH, from_value, to_owned_value, to_value};
use tb_proto::keys::{self, EqKeys};
use tb_proto::profile::{self, Profile, Step};
use tb_proto::values::{self, Typed};
use tb_proto::{Client, State};
use zbus::blocking::Connection;
use zbus::zvariant::{OwnedValue, Value};
use zbus::{fdo, interface};


/// How long the device thread waits for a request before polling the device again.
const IDLE_POLL: Duration = Duration::from_millis(200);
/// How often to look for a device while none is connected.
const SCAN_INTERVAL: Duration = Duration::from_secs(2);
/// After the headset's link came back: how long to wait before subscribing again, and how often to try.
const RESUBSCRIBE_DELAY: Duration = Duration::from_millis(1500);
const RESUBSCRIBE_TRIES: u32 = 5;
/// How long to wait for the headset's pushes after a write, and the quiet time that ends them early.
const PUSH_WAIT: Duration = Duration::from_millis(800);
const PUSH_QUIET: Duration = Duration::from_millis(150);
/// Battery levels that trigger a notification while discharging.
const LOW_BATTERY: [i64; 2] = [15, 5];
/// How often to read the volume from PipeWire. The headset's own `volume` changes with it and triggers a read
/// too, but not for changes smaller than its 1 dB steps.
const SINK_REFRESH: Duration = Duration::from_secs(5);

/// What the D-Bus side asks the device thread to do.
enum Op {
    Set(String, Typed),
    SetBand { eq: &'static EqKeys, band: usize, tenths: i64 },
    SavePreset { eq: &'static EqKeys, name: String },
    DeletePreset { eq: &'static EqKeys, name: String },
    /// The headset's volume in PipeWire, in percent.
    SinkVolume(i64),
    ApplyProfile(Profile),
}

struct Request {
    op: Op,
    reply: async_channel::Sender<Result<(), String>>,
}

/// What the D-Bus side reads.
#[derive(Default)]
struct Shared {
    device: Option<String>,
    values: BTreeMap<&'static str, Typed>,
    settings: Settings,
    profiles: BTreeMap<String, Profile>,
    /// Whether the settings differ from the selected profile, as last signalled.
    modified: bool,
}

impl Shared {
    fn profile_names(&self) -> Typed {
        Typed::Texts(self.profiles.keys().cloned().collect())
    }

    /// Whether the headset is there to be read and written: a transmitter, and the headset linked to it.
    fn headset_available(&self) -> bool {
        self.device.is_some() && self.values.get("headset_link") == Some(&Typed::Bool(true))
    }

    /// Whether the headset's settings differ from the selected profile (never while the headset is away, when its
    /// settings are not known).
    fn differs_from_profile(&self) -> bool {
        self.headset_available()
            && self.profiles.get(&self.settings.profile).is_some_and(|p| !p.plan(&self.values).is_empty())
    }

    /// Bring `modified` up to date, noting a change in `changed`.
    fn update_modified(&mut self, changed: &mut HashMap<String, Value<'static>>) {
        let modified = self.differs_from_profile();
        if modified != self.modified {
            self.modified = modified;
            changed.insert("profile_modified".into(), Value::from(modified));
        }
    }

    /// Select a profile ("" for none) and keep the choice, noting a change in `changed`. A name that is not a saved
    /// profile selects none.
    fn select(&mut self, name: &str, changed: &mut HashMap<String, Value<'static>>) {
        let name = if self.profiles.contains_key(name) { name } else { "" };
        let previous = std::mem::replace(&mut self.settings.profile, name.to_string());
        if previous != name {
            changed.insert("active_profile".into(), Value::from(name.to_string()));
            if let Err(e) = self.settings.save() {
                eprintln!("stealthd: saving the selected profile: {e}");
            }
        }
        self.update_modified(changed);
    }
}

/// `~/.config/swarm2linux`, or its place under `$XDG_CONFIG_HOME`.
fn config_dir() -> Option<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(config.join("swarm2linux"))
}

/// stealthd's own settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
struct Settings {
    /// Warn about a low battery with a desktop notification.
    battery_notifications: bool,
    /// The selected profile, or "".
    profile: String,
    /// Show the level on screen when the wheel on the headset turns mic monitoring or the noise gate.
    level_popups: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { battery_notifications: true, profile: String::new(), level_popups: true }
    }
}

impl Settings {
    fn path() -> Option<PathBuf> {
        Some(config_dir()?.join("stealthd.toml"))
    }

    fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default() };
        match fs::read_to_string(&path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                eprintln!("stealthd: ignoring {}: {e}", path.display());
                Self::default()
            }),
            Err(_) => Self::default(),
        }
    }

    fn save(&self) -> io::Result<()> {
        let path = Self::path().ok_or_else(|| io::Error::other("no home directory"))?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let partial = path.with_extension("toml.new");
        fs::write(&partial, toml::to_string(self).map_err(io::Error::other)?)?;
        fs::rename(partial, path)
    }
}

struct Headset {
    shared: Arc<Mutex<Shared>>,
    requests: mpsc::Sender<Request>,
    /// One token: saving, applying and deleting profiles happen one at a time.
    profile_token: (async_channel::Sender<()>, async_channel::Receiver<()>),
}

/// Holds the profile token; gives it back when dropped.
struct ProfileTurn(async_channel::Sender<()>);

impl Drop for ProfileTurn {
    fn drop(&mut self) {
        let _ = self.0.try_send(());
    }
}

impl Headset {
    fn new(shared: Arc<Mutex<Shared>>, requests: mpsc::Sender<Request>) -> Self {
        let profile_token = async_channel::bounded(1);
        let _ = profile_token.0.try_send(());
        Self { shared, requests, profile_token }
    }

    /// Wait for the turn to change profiles.
    async fn profile_turn(&self) -> ProfileTurn {
        let _ = self.profile_token.1.recv().await;
        ProfileTurn(self.profile_token.0.clone())
    }

    async fn submit(&self, op: Op) -> fdo::Result<()> {
        let (reply, answer) = async_channel::bounded(1);
        self.requests.send(Request { op, reply }).map_err(|_| fdo::Error::Failed("device thread stopped".into()))?;
        match answer.recv().await {
            Ok(Ok(())) => Ok(()),
            Ok(Err(e)) => Err(fdo::Error::Failed(e)),
            Err(_) => Err(fdo::Error::Failed("no answer from the device thread".into())),
        }
    }

    /// Read the profiles from disk again and signal what changed. A selected profile that is gone is unselected.
    async fn reload_profiles(&self, conn: &zbus::Connection) {
        let changed = {
            let loaded = profiles::load();
            let mut shared = self.shared.lock().unwrap();
            let mut changed: HashMap<String, Value<'static>> = HashMap::new();
            if loaded != shared.profiles {
                shared.profiles = loaded;
                changed.insert("profiles".into(), to_value(&shared.profile_names()));
            }
            if !shared.profiles.contains_key(&shared.settings.profile) {
                shared.select("", &mut changed);
            }
            shared.update_modified(&mut changed);
            changed
        };
        emit_changed(conn, changed).await;
    }

    /// Select a profile ("" for none) and signal it.
    async fn select(&self, name: &str, conn: &zbus::Connection) {
        let mut changed = HashMap::new();
        self.shared.lock().unwrap().select(name, &mut changed);
        emit_changed(conn, changed).await;
    }
}

async fn emit_changed(conn: &zbus::Connection, changed: HashMap<String, Value<'static>>) {
    if !changed.is_empty()
        && let Err(e) = conn.emit_signal(None::<&str>, PATH, INTERFACE, "Changed", &(changed,)).await
    {
        eprintln!("stealthd: emitting Changed: {e}");
    }
}

#[interface(name = "io.github.swarm2linux.Headset1")]
impl Headset {
    /// Every known value by name, plus `connected` and `device`.
    async fn values(&self) -> HashMap<String, OwnedValue> {
        let shared = self.shared.lock().unwrap();
        let mut out: HashMap<String, OwnedValue> =
            shared.values.iter().map(|(k, v)| (k.to_string(), to_owned_value(v))).collect();
        out.insert("connected".into(), to_owned_value(&Typed::Bool(shared.device.is_some())));
        out.insert("device".into(), to_owned_value(&Typed::Text(shared.device.clone().unwrap_or_default())));
        out.insert("profiles".into(), to_owned_value(&shared.profile_names()));
        out.insert("active_profile".into(), to_owned_value(&Typed::Text(shared.settings.profile.clone())));
        out.insert("profile_modified".into(), to_owned_value(&Typed::Bool(shared.modified)));
        out
    }

    /// Set one value by name, e.g. ("anc", true), ("anc_level", 60) or ("sink_volume", 50).
    async fn set(&self, name: String, value: OwnedValue) -> fdo::Result<()> {
        let typed = match from_value(&value) {
            Some(t @ (Typed::Bool(_) | Typed::Int(_))) => t,
            _ => return Err(fdo::Error::InvalidArgs("expected a boolean or an integer".into())),
        };
        if name == "sink_volume" {
            return match typed {
                Typed::Int(n @ 0..=100) => self.submit(Op::SinkVolume(n)).await,
                _ => Err(fdo::Error::InvalidArgs("sink_volume is a percentage from 0 to 100".into())),
            };
        }
        let Some((key, number)) = values::to_write(&name, &typed) else {
            return Err(fdo::Error::InvalidArgs(format!("{name} cannot be set this way")));
        };
        if !keys::writable(key).is_some_and(|s| s.values.accepts(number)) {
            return Err(fdo::Error::InvalidArgs(format!("{name} cannot be {number}")));
        }
        self.submit(Op::Set(name, typed)).await
    }

    /// Set one band (1–10) of the "game" or "mic" equalizer, in tenths of a dB (-90 to 90).
    async fn set_band(&self, eq: String, band: u32, tenths: i32) -> fdo::Result<()> {
        let eq = which_eq(&eq)?;
        if !(1..=10).contains(&band) {
            return Err(fdo::Error::InvalidArgs("band is 1 to 10".into()));
        }
        if !(-90..=90).contains(&tenths) {
            return Err(fdo::Error::InvalidArgs("a band is -90 to 90 tenths of a dB".into()));
        }
        self.submit(Op::SetBand { eq, band: band as usize - 1, tenths: i64::from(tenths) }).await
    }

    /// Save the current bands of the "game" or "mic" equalizer as a custom preset.
    async fn save_preset(&self, eq: String, name: String) -> fdo::Result<()> {
        let eq = which_eq(&eq)?;
        self.submit(Op::SavePreset { eq, name }).await
    }

    /// Delete a custom preset of the "game" or "mic" equalizer by name.
    async fn delete_preset(&self, eq: String, name: String) -> fdo::Result<()> {
        let eq = which_eq(&eq)?;
        self.submit(Op::DeletePreset { eq, name }).await
    }

    /// Save the headset's current settings as a profile, replacing one of the same name, and select it.
    async fn save_profile(&self, name: String, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        let problem = profile::name_problem(&name);
        if name.is_empty() || !problem.is_empty() {
            let why = if name.is_empty() { "a profile needs a name" } else { problem };
            return Err(fdo::Error::InvalidArgs(why.into()));
        }
        let _turn = self.profile_turn().await;
        let captured = {
            let shared = self.shared.lock().unwrap();
            if !shared.headset_available() {
                return Err(fdo::Error::Failed("the headset is not connected".into()));
            }
            Profile::capture(&shared.values)
        };
        if captured.is_empty() {
            return Err(fdo::Error::Failed("the headset's settings are not known yet".into()));
        }
        profiles::save(&name, &captured).map_err(|e| fdo::Error::Failed(format!("saving the profile: {e}")))?;
        self.reload_profiles(conn).await;
        self.select(&name, conn).await;
        Ok(())
    }

    /// Select a saved profile and apply it: write the settings that differ from it. Applying the selected profile
    /// again undoes changes made since. If a write fails, the profile stays selected and shows as modified, so
    /// applying it again finishes the job.
    async fn apply_profile(&self, name: String, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        let _turn = self.profile_turn().await;
        self.reload_profiles(conn).await;
        let (found, available) = {
            let shared = self.shared.lock().unwrap();
            (shared.profiles.get(&name).cloned(), shared.headset_available())
        };
        let profile = found.ok_or_else(|| fdo::Error::InvalidArgs(format!("there is no profile called {name:?}")))?;
        if !available {
            return Err(fdo::Error::Failed("the headset is not connected".into()));
        }
        // selected first, so the values the writes bring are compared with this profile
        self.select(&name, conn).await;
        self.submit(Op::ApplyProfile(profile)).await
    }

    async fn delete_profile(&self, name: String, #[zbus(connection)] conn: &zbus::Connection) -> fdo::Result<()> {
        if name.is_empty() || !profile::name_problem(&name).is_empty() {
            return Err(fdo::Error::InvalidArgs(format!("there is no profile called {name:?}")));
        }
        let _turn = self.profile_turn().await;
        let result = profiles::delete(&name);
        self.reload_profiles(conn).await;
        match result {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                Err(fdo::Error::InvalidArgs(format!("there is no profile called {name:?}")))
            }
            Err(e) => Err(fdo::Error::Failed(format!("deleting the profile: {e}"))),
        }
    }

    /// This stealthd's version.
    #[zbus(property)]
    async fn version(&self) -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Whether a headset is connected.
    #[zbus(property)]
    async fn connected(&self) -> bool {
        self.shared.lock().unwrap().device.is_some()
    }

    /// Whether to warn about a low battery with a desktop notification.
    #[zbus(property)]
    async fn battery_notifications(&self) -> bool {
        self.shared.lock().unwrap().settings.battery_notifications
    }

    #[zbus(property)]
    async fn set_battery_notifications(&mut self, on: bool) -> fdo::Result<()> {
        let mut shared = self.shared.lock().unwrap();
        let mut settings = shared.settings.clone();
        settings.battery_notifications = on;
        settings.save().map_err(|e| fdo::Error::Failed(format!("saving the setting: {e}")))?;
        shared.settings = settings;
        Ok(())
    }
}

fn which_eq(name: &str) -> fdo::Result<&'static EqKeys> {
    match name {
        "game" => Ok(&keys::GAME_EQ),
        "mic" => Ok(&keys::MIC_EQ),
        _ => Err(fdo::Error::InvalidArgs("equalizer is \"game\" or \"mic\"".into())),
    }
}

fn main() -> Result<()> {
    let mut shared = Shared { settings: Settings::load(), profiles: profiles::load(), ..Shared::default() };
    if !shared.profiles.contains_key(&shared.settings.profile) {
        shared.settings.profile.clear();
    }
    let shared = Arc::new(Mutex::new(shared));
    let (requests, queue) = mpsc::channel();
    let conn = zbus::blocking::connection::Builder::session()?
        .name(BUS_NAME)
        .context("is another stealthd already running?")?
        .serve_at(PATH, Headset::new(shared.clone(), requests))?
        .build()?;
    eprintln!("stealthd: serving {INTERFACE} at {PATH} as {BUS_NAME}");

    let osd = osd::Osd::start(conn.clone());
    let mut daemon = Daemon { conn, shared, queue, notified_low: None, card: None, sink: None, sink_problem: None, osd };
    loop {
        match pick_device() {
            Some((path, label)) => {
                eprintln!("stealthd: using {} ({label})", path.display());
                if let Err(e) = daemon.run(&path, &label) {
                    eprintln!("stealthd: {} lost: {e:#}", path.display());
                }
                daemon.publish(None, &State::new(), None);
                daemon.fail_pending("the headset disconnected");
            }
            None => {
                daemon.fail_pending("no headset connected");
                sleep(SCAN_INTERVAL);
            }
        }
    }
}

/// The best device to use: a transmitter before the headset's own cable, then by path.
fn pick_device() -> Option<(PathBuf, String)> {
    // only products this protocol was checked on; other Turtle Beach devices may share the vendor collection
    let mut found: Vec<_> =
        discover().ok()?.into_iter().filter(|d| d.vendor_collection && known_product(d.pid).is_some()).collect();
    found.sort_by_key(|d| (known_product(d.pid).is_none_or(|p| p.contains("headset")), d.path.clone()));
    let d = found.into_iter().next()?;
    let label = format!("10f5:{:04x} {}", d.pid, known_product(d.pid).unwrap_or(&d.name));
    Some((d.path, label))
}

struct Daemon {
    conn: Connection,
    shared: Arc<Mutex<Shared>>,
    queue: mpsc::Receiver<Request>,
    /// The lowest battery threshold already notified about while discharging.
    notified_low: Option<i64>,
    /// The ALSA card of the device in use, and its sink in PipeWire as last read.
    card: Option<u32>,
    sink: Option<Sink>,
    /// The last error reading the sink, so it is logged once.
    sink_problem: Option<String>,
    osd: osd::Osd,
}

impl Daemon {
    /// Serve one device until it goes away.
    fn run(&mut self, path: &Path, label: &str) -> Result<()> {
        let mut client = tb_proto::open(Some(path))?;
        let (mut state, problems) = client.subscribe_all()?;
        for p in problems {
            eprintln!("stealthd: {p}");
        }
        settle_noise_reduction(&mut client, &mut state)?;
        self.card = sink_volume::sound_card(path);
        if self.card.is_none() {
            eprintln!("stealthd: no sound card found for {}; the volume can't be set", path.display());
        }
        self.refresh_sink();
        let mut next_sink_refresh = Instant::now() + SINK_REFRESH;
        self.publish(Some(label.to_string()), &state, client.headset_link());
        // when to subscribe again after the headset came back, and how many tries are left
        let mut resubscribe: Option<(Instant, u32)> = None;

        loop {
            match self.queue.recv_timeout(IDLE_POLL) {
                Ok(req) => {
                    let (result, to_headset) = match req.op {
                        Op::SinkVolume(percent) => (self.set_sink_volume(percent), false),
                        op => (execute(&mut client, &state, op).map_err(|e| format!("{e:#}")), true),
                    };
                    if to_headset {
                        // answer only once the headset has pushed the new values, so a read right after a write
                        // sees them; also after a failure, as earlier writes of the same request may have landed
                        match await_pushes(&mut client) {
                            Ok(docs) => {
                                for doc in &docs {
                                    state.apply(doc);
                                }
                            }
                            Err(e) => {
                                let _ = req.reply.send_blocking(Err("the headset disconnected".into()));
                                return Err(e);
                            }
                        }
                    }
                    self.publish(Some(label.to_string()), &state, client.headset_link());
                    let _ = req.reply.send_blocking(result);
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            }
            let link = client.headset_link();
            let docs = client.poll()?;
            if !docs.is_empty() || client.headset_link() != link {
                // pushes that arrive here were not asked for by a write: the headset's own controls
                let levels = osd::levels(&state);
                for doc in &docs {
                    state.apply(doc);
                }
                self.publish(Some(label.to_string()), &state, client.headset_link());
                if self.shared.lock().unwrap().settings.level_popups {
                    for popup in osd::changes(&levels, &state) {
                        self.osd.show(popup);
                    }
                }
            }
            if Instant::now() >= next_sink_refresh {
                next_sink_refresh = Instant::now() + SINK_REFRESH;
                if self.refresh_sink() {
                    self.publish(Some(label.to_string()), &state, client.headset_link());
                }
            }
            if client.take_link_up() {
                // the headset restarted and forgot every subscription; give it a moment to settle
                resubscribe = Some((Instant::now() + RESUBSCRIBE_DELAY, RESUBSCRIBE_TRIES));
            }
            if let Some((when, tries)) = resubscribe
                && Instant::now() >= when
            {
                match client.subscribe_all() {
                    Ok((fresh, problems)) if problems.is_empty() || tries <= 1 => {
                        eprintln!("stealthd: headset back, subscribed again");
                        state = fresh;
                        resubscribe = None;
                        settle_noise_reduction(&mut client, &mut state)?;
                        self.publish(Some(label.to_string()), &state, client.headset_link());
                    }
                    Ok((_, problems)) => {
                        eprintln!("stealthd: headset not ready yet ({}), trying again", problems.join("; "));
                        resubscribe = Some((Instant::now() + RESUBSCRIBE_DELAY, tries - 1));
                    }
                    Err(e @ tb_proto::Error::Timeout(_)) if tries > 1 => {
                        eprintln!("stealthd: headset not ready yet ({e}), trying again");
                        resubscribe = Some((Instant::now() + RESUBSCRIBE_DELAY, tries - 1));
                    }
                    Err(e) => return Err(e.into()),
                }
            }
        }
    }

    /// Store the new values and signal the ones that changed.
    fn publish(&mut self, device: Option<String>, state: &State, link: Option<bool>) {
        let mut new = values::values(state);
        // the link indication when one arrived, else whether the transmitter reports an active link
        if let Some(up) = link.or_else(|| state.int(keys::ACTIVE_TRANSMITTER).map(|t| t > 0)) {
            new.insert("headset_link", Typed::Bool(up));
        }
        if device.is_some() {
            // the headset's own volume moves with PipeWire's: read that again
            let moved = self.shared.lock().unwrap().values.get("volume") != new.get("volume");
            if moved {
                self.refresh_sink();
            }
            new.insert("sink_volume", Typed::Int(self.sink.map_or(-1, |s| s.percent)));
        } else {
            self.card = None;
            self.sink = None;
        }
        let mut changed: HashMap<String, Value<'static>> = HashMap::new();
        {
            let mut shared = self.shared.lock().unwrap();
            for (name, v) in &new {
                if shared.values.get(name) != Some(v) {
                    changed.insert(name.to_string(), to_value(v));
                }
            }
            if shared.device != device {
                changed.insert("connected".into(), Value::from(device.is_some()));
                changed.insert("device".into(), Value::from(device.clone().unwrap_or_default()));
            }
            shared.device = device;
            shared.values = new.clone();
            shared.update_modified(&mut changed);
        }
        if changed.is_empty() {
            return;
        }
        if let Err(e) = self.conn.emit_signal(None::<&str>, PATH, INTERFACE, "Changed", &(changed,)) {
            eprintln!("stealthd: emitting Changed: {e}");
        }
        self.check_battery(&new);
    }

    fn check_battery(&mut self, v: &BTreeMap<&'static str, Typed>) {
        let charging = v.get("charging") == Some(&Typed::Bool(true));
        let Some(&Typed::Int(level)) = v.get("battery") else { return };
        if charging || level < 0 {
            self.notified_low = None;
            return;
        }
        let Some(threshold) = LOW_BATTERY.iter().copied().filter(|&t| level <= t).min() else {
            self.notified_low = None;
            return;
        };
        if self.notified_low.is_some_and(|n| n <= threshold) {
            return;
        }
        // checked first, so a threshold passed while they were off is still announced once they are on
        if !self.shared.lock().unwrap().settings.battery_notifications {
            return;
        }
        self.notified_low = Some(threshold);
        notify(&self.conn, "Headset battery low", &format!("Stealth Pro II battery at {level}%"));
    }

    /// Read the volume from PipeWire again; true if it changed.
    fn refresh_sink(&mut self) -> bool {
        let found = match self.card {
            Some(card) => sink_volume::find(card),
            None => Ok(None),
        };
        let sink = found.unwrap_or_else(|e| {
            let problem = e.to_string();
            if self.sink_problem.as_ref() != Some(&problem) {
                eprintln!("stealthd: reading the volume from PipeWire: {problem}");
                self.sink_problem = Some(problem);
            }
            None
        });
        if sink.is_some() {
            self.sink_problem = None;
        }
        let changed = sink != self.sink;
        self.sink = sink;
        changed
    }

    fn set_sink_volume(&mut self, percent: i64) -> Result<(), String> {
        let card = self.card.ok_or("the headset's sound card was not found")?;
        let sink = sink_volume::find(card)
            .map_err(|e| e.to_string())?
            .ok_or("PipeWire has no output for the headset")?;
        sink_volume::set_percent(sink.id, percent).map_err(|e| e.to_string())?;
        self.refresh_sink();
        Ok(())
    }

    /// Answer queued requests that cannot be served now.
    fn fail_pending(&self, why: &str) {
        while let Ok(req) = self.queue.try_recv() {
            let _ = req.reply.send_blocking(Err(why.to_string()));
        }
    }
}

/// The documents the headset pushes after a write. They follow the ack within about 200 ms and can come in
/// several batches (a saved preset: the preset change first, the slot record a little later), so this returns
/// once some arrived and nothing more came for `PUSH_QUIET`, or after `PUSH_WAIT`.
fn await_pushes(client: &mut Client<HidrawDevice>) -> Result<Vec<tb_proto::Document>> {
    let start = Instant::now();
    let mut last = None;
    let mut docs = client.take_updates();
    if !docs.is_empty() {
        last = Some(start);
    }
    loop {
        let got = client.poll()?;
        let now = Instant::now();
        // the overall limit holds even if the device never stops pushing
        if now - start >= PUSH_WAIT {
            docs.extend(got);
            return Ok(docs);
        }
        if !got.is_empty() {
            docs.extend(got);
            last = Some(now);
        } else if last.is_some_and(|t| now - t >= PUSH_QUIET) {
            return Ok(docs);
        } else {
            sleep(Duration::from_millis(30));
        }
    }
}

/// The headset can come back from a restart with AI noise reduction at work while it reports it off: in recordings
/// the background noise is about 20 dB lower, as with it on, until it is switched on and off again. Writing `0`
/// alone does not end it. So whenever the headset connects with it reported off, switch it on and off (both writes
/// Swarm II makes), before the values are published.
fn settle_noise_reduction(client: &mut Client<HidrawDevice>, state: &mut State) -> Result<()> {
    if state.int(keys::AI_NOISE_REDUCTION) != Some(0) {
        return Ok(());
    }
    for value in [1, 0] {
        match client.set(keys::AI_NOISE_REDUCTION, value) {
            Ok(()) => {}
            Err(e @ tb_proto::Error::Timeout(_)) => {
                eprintln!("stealthd: switching AI noise reduction off again: {e}");
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        }
        for doc in await_pushes(client)? {
            state.apply(&doc);
        }
    }
    eprintln!("stealthd: switched AI noise reduction on and off, so it is really off");
    Ok(())
}

fn execute(client: &mut Client<HidrawDevice>, state: &State, op: Op) -> Result<()> {
    match op {
        Op::Set(name, value) => {
            let (key, number) = values::to_write(&name, &value).context("not writable")?;
            client.set(key, number)?;
        }
        Op::SetBand { eq, band, tenths } => client.set(eq.band(band), tenths)?,
        Op::SavePreset { eq, name } => client.save_new_preset(state, eq, &name)?,
        Op::DeletePreset { eq, name } => client.delete_named_preset(state, eq, &name)?,
        Op::ApplyProfile(profile) => {
            for step in profile.plan(&values::values(state)) {
                match step {
                    Step::Set(name, n) => {
                        let key = values::key_of(name).context("not writable")?;
                        client.set(key, n).with_context(|| format!("setting {name} to {n}"))?;
                    }
                    Step::Band(eq, band, tenths) => client.set(eq.band(band), tenths)?,
                }
            }
        }
        Op::SinkVolume(_) => unreachable!("handled by the daemon"),
    }
    Ok(())
}

/// Show a desktop notification, from its own thread: a slow notification service must not hold up the device.
fn notify(conn: &Connection, summary: &str, body: &str) {
    let (conn, summary, body) = (conn.clone(), summary.to_string(), body.to_string());
    std::thread::spawn(move || {
        let hints: HashMap<&str, Value> = HashMap::from([("urgency", Value::from(2u8))]);
        let result = conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("Stealth Pro II", 0u32, "audio-headset", summary.as_str(), body.as_str(), Vec::<&str>::new(), hints, -1i32),
        );
        if let Err(e) = result {
            eprintln!("stealthd: notification failed: {e}");
        }
    });
}
