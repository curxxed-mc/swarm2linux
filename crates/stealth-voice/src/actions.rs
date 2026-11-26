//! Carrying out commands: media players over MPRIS, the default output's volume with `wpctl`, the headset through
//! stealthd, and a short pop-up saying what was done.

use std::collections::HashMap;
use std::process::Command as Process;

use anyhow::{Context, Result, bail};
use stealth_dbus::DaemonClient;
use tb_proto::values::Typed;
use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::Value;

use crate::commands::Command;

const MPRIS_PREFIX: &str = "org.mpris.MediaPlayer2.";
const MPRIS_PATH: &str = "/org/mpris/MediaPlayer2";
const MPRIS_PLAYER: &str = "org.mpris.MediaPlayer2.Player";

pub struct Actions {
    conn: Connection,
    /// The player the last command went to, so "resume" finds what "pause" paused.
    last_player: Option<String>,
    /// The pop-up shown last, replaced by the next.
    popup: u32,
}

impl Actions {
    pub fn new() -> Result<Self> {
        Ok(Self { conn: Connection::session().context("no session bus")?, last_player: None, popup: 0 })
    }

    pub fn run(&mut self, command: Command) -> Result<()> {
        match command {
            Command::Next => self.player_call(command, "Next"),
            Command::Previous => self.player_call(command, "Previous"),
            Command::Pause => self.player_call(command, "Pause"),
            Command::Play => self.player_call(command, "Play"),
            Command::PlayPause => self.player_call(command, "PlayPause"),
            Command::VolumeUp => volume("5%+"),
            Command::VolumeDown => volume("5%-"),
            Command::Anc(on) => {
                let client = DaemonClient::connect()?.context("stealthd is not running")?;
                Ok(client.set("anc", &Typed::Bool(on))?)
            }
        }
    }

    fn players(&self) -> Result<Vec<(String, String)>> {
        let dbus = zbus::blocking::fdo::DBusProxy::new(&self.conn)?;
        let mut out = Vec::new();
        for name in dbus.list_names()? {
            let name = name.to_string();
            if !name.starts_with(MPRIS_PREFIX) {
                continue;
            }
            let status = Proxy::new(&self.conn, name.as_str(), MPRIS_PATH, MPRIS_PLAYER)
                .and_then(|p| p.get_property::<String>("PlaybackStatus"))
                .unwrap_or_default();
            out.push((name, status));
        }
        Ok(out)
    }

    /// The player a command is for: a playing one, except that resuming prefers the player paused last.
    fn pick(&self, command: Command) -> Result<String> {
        let players = self.players()?;
        let with = |status: &str| players.iter().find(|(_, s)| s == status).map(|(n, _)| n.clone());
        let last = self.last_player.clone().filter(|l| players.iter().any(|(n, _)| n == l));
        let pick = match command {
            Command::Play => last.filter(|l| players.iter().any(|(n, s)| n == l && s == "Paused")).or_else(|| with("Paused")),
            _ => with("Playing").or(last).or_else(|| with("Paused")),
        };
        match pick.or_else(|| players.first().map(|(n, _)| n.clone())) {
            Some(p) => Ok(p),
            None => bail!("no media player found"),
        }
    }

    fn player_call(&mut self, command: Command, method: &str) -> Result<()> {
        let player = self.pick(command)?;
        Proxy::new(&self.conn, player.as_str(), MPRIS_PATH, MPRIS_PLAYER)?
            .call_method(method, &())
            .with_context(|| format!("{player} refused {method}"))?;
        self.last_player = Some(player);
        Ok(())
    }

    /// A short pop-up, replacing the previous one.
    pub fn popup(&mut self, summary: &str, body: &str) {
        let hints: HashMap<&str, Value> =
            HashMap::from([("transient", Value::from(true)), ("x-kde-display-appname", Value::from("Voice commands"))]);
        let reply = self.conn.call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &("Voice commands", self.popup, "audio-input-microphone", summary, body, Vec::<&str>::new(), hints, 2000i32),
        );
        match reply.and_then(|m| m.body().deserialize::<u32>()) {
            Ok(id) => self.popup = id,
            Err(e) => eprintln!("stealth-voice: pop-up failed: {e}"),
        }
    }
}

fn volume(step: &str) -> Result<()> {
    let status = Process::new("wpctl").args(["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", step]).status()?;
    if !status.success() {
        bail!("wpctl set-volume failed");
    }
    Ok(())
}
