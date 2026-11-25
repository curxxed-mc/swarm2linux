//! On-screen level pop-ups for settings turned on the headset itself: the assignable wheel moves mic monitoring
//! or the noise gate level, and the desktop shows the new percentage the way it shows the volume.
//!
//! On KDE Plasma this is the volume pop-up (`org.kde.osdService.mediaPlayerVolumeChanged`, which shows a label,
//! an icon and a bar); elsewhere a desktop notification with a progress value that replaces the previous one. A
//! thread of its own shows them, always the latest, so turning the wheel quickly does not queue up pop-ups.

use std::collections::HashMap;
use std::sync::mpsc;
use std::thread;

use tb_proto::State;
use tb_proto::keys;
use zbus::blocking::Connection;
use zbus::zvariant::Value;

/// A level the wheel can turn: its key, the pop-up's label and icon.
const LEVELS: [(u16, &str, &str); 2] = [
    (keys::MONITORING_LEVEL, "Mic monitoring", "audio-input-microphone"),
    (keys::NOISE_GATE_LEVEL, "Noise gate", "audio-input-microphone"),
];

/// A pop-up to show: label, icon, percent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Popup {
    pub label: &'static str,
    pub icon: &'static str,
    pub percent: i64,
}

/// The levels as they are now, to compare after the headset pushed changes.
pub fn levels(state: &State) -> [Option<i64>; 2] {
    LEVELS.map(|(key, _, _)| state.int(key))
}

/// The pop-ups for levels that changed between `before` and `state`. A level seen for the first time (after
/// connecting) is not a change.
pub fn changes(before: &[Option<i64>; 2], state: &State) -> Vec<Popup> {
    LEVELS
        .iter()
        .zip(before)
        .filter_map(|(&(key, label, icon), old)| {
            let new = state.int(key)?;
            (old.is_some_and(|old| old != new)).then_some(Popup { label, icon, percent: new.clamp(0, 100) })
        })
        .collect()
}

/// Shows pop-ups from its own thread.
pub struct Osd {
    popups: mpsc::Sender<Popup>,
}

impl Osd {
    pub fn start(conn: Connection) -> Self {
        let (popups, queue) = mpsc::channel::<Popup>();
        let spawned = thread::Builder::new().name("osd".into()).spawn(move || {
            // the notification the next one replaces, where there is no Plasma pop-up
            let mut notification = 0u32;
            while let Ok(mut popup) = queue.recv() {
                // only the latest of a burst
                while let Ok(newer) = queue.try_recv() {
                    popup = newer;
                }
                if plasma(&conn, &popup).is_err() {
                    notification = notify(&conn, &popup, notification).unwrap_or(notification);
                }
            }
        });
        if let Err(e) = spawned {
            eprintln!("stealthd: no level pop-ups: {e}");
        }
        Self { popups }
    }

    pub fn show(&self, popup: Popup) {
        let _ = self.popups.send(popup);
    }
}

/// Plasma's own volume pop-up, with the label and icon given.
fn plasma(conn: &Connection, p: &Popup) -> zbus::Result<()> {
    conn.call_method(
        Some("org.kde.plasmashell"),
        "/org/kde/osdService",
        Some("org.kde.osdService"),
        "mediaPlayerVolumeChanged",
        &(p.percent as i32, p.label, p.icon),
    )
    .map(drop)
}

/// A short-lived notification with a progress value, replacing the previous one; its id.
fn notify(conn: &Connection, p: &Popup, replaces: u32) -> zbus::Result<u32> {
    let hints: HashMap<&str, Value> = HashMap::from([
        ("value", Value::from(p.percent as i32)),
        ("transient", Value::from(true)),
        ("x-canonical-private-synchronous", Value::from("swarm2linux-level")),
    ]);
    let reply = conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &("Stealth Pro II", replaces, p.icon, p.label, format!("{}%", p.percent), Vec::<&str>::new(), hints, 1500i32),
    )?;
    reply.body().deserialize()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tb_proto::Document;

    fn state(kvp: serde_json::Value) -> State {
        let mut s = State::new();
        s.apply(&Document::from_json(&json!({"OR": "Mic", "KVP": kvp})).unwrap());
        s
    }

    #[test]
    fn only_changes_pop_up() {
        let s = state(json!({"620": "65"}));
        assert_eq!(changes(&[None, None], &s), [], "first seen after connecting");
        let before = levels(&s);
        assert_eq!(changes(&before, &s), [], "unchanged");
        let mut s2 = s.clone();
        s2.apply(&Document::from_json(&json!({"UP": "Mic", "KVP": {"620": "70"}})).unwrap());
        assert_eq!(changes(&before, &s2), [Popup { label: "Mic monitoring", icon: "audio-input-microphone", percent: 70 }]);
    }
}
