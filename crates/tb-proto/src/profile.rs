//! Profiles: named sets of settings ("Gaming", "Calls") that can be saved from the headset and applied to it
//! again in one go.
//!
//! A profile holds the settings of [`SETTINGS`] and both equalizers, by the value names of [`crate::values`]. It
//! leaves out what belongs to the headset rather than to an activity: the volume, auto shut-off, wake on motion
//! and the charging hub's LEDs. An equalizer is kept as its preset's name and its bands: applying selects the
//! preset when the headset still has one by that name, and otherwise writes the bands.
//!
//! Applying writes only what differs, through the same keys and values as the single settings (see
//! [`crate::keys::writable`]).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::keys::{self, EqKeys};
use crate::values::{self, Typed};

/// The settings a profile holds besides the equalizers, in the order they are applied. Superhuman Hearing comes
/// last: selecting an EQ preset switches it off.
pub const SETTINGS: [&str; 12] = [
    "anc",
    "anc_level",
    "mic_sensitivity",
    "monitoring_level",
    "noise_gate",
    "noise_gate_level",
    "ai_noise_reduction",
    "button_function",
    "wheel_function",
    "superhuman_mode",
    "superhuman_level",
    "superhuman",
];

/// The longest profile name.
pub const NAME_MAX: usize = 32;

/// A saved set of settings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Profile {
    /// Settings by value name; on/off settings are booleans, the others integers.
    #[serde(flatten)]
    pub settings: BTreeMap<String, Setting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub game_eq: Option<EqSetting>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mic_eq: Option<EqSetting>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Setting {
    Bool(bool),
    Int(i64),
}

impl Setting {
    fn number(self) -> i64 {
        match self {
            Setting::Bool(b) => i64::from(b),
            Setting::Int(n) => n,
        }
    }
}

/// One equalizer in a profile.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EqSetting {
    /// The preset's name: an onboard preset ("Bass Boost") or a saved custom one. Left out for unsaved bands.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preset: Option<String>,
    /// The 10 bands in tenths of a dB.
    pub bands: Vec<i64>,
}

/// One write that applying a profile takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// A value by name, as `Set` takes it.
    Set(&'static str, i64),
    /// One band (0–9) of an equalizer, in tenths of a dB.
    Band(&'static EqKeys, usize, i64),
}

/// The equalizers and the value names of their preset, bands and custom slots.
const EQUALIZERS: [(&EqKeys, &str, &str, &str); 2] = [
    (&keys::GAME_EQ, "game_eq_preset", "game_eq_bands", "game_eq_slots"),
    (&keys::MIC_EQ, "mic_eq_preset", "mic_eq_bands", "mic_eq_slots"),
];

impl Profile {
    /// The headset's current settings as a profile, from the values of [`values::values`].
    pub fn capture(current: &BTreeMap<&str, Typed>) -> Profile {
        let mut profile = Profile::default();
        for name in SETTINGS {
            let setting = match current.get(name) {
                Some(Typed::Bool(b)) => Setting::Bool(*b),
                Some(Typed::Int(n)) => Setting::Int(*n),
                _ => continue,
            };
            profile.settings.insert(name.to_string(), setting);
        }
        for (i, (eq, preset, bands, slots)) in EQUALIZERS.into_iter().enumerate() {
            let Some(Typed::Ints(bands)) = current.get(bands) else { continue };
            let number = match current.get(preset) {
                Some(Typed::Int(n)) => *n,
                _ => 0,
            };
            let slot_names = match current.get(slots) {
                Some(Typed::Texts(names)) => names.as_slice(),
                _ => &[],
            };
            let setting = EqSetting { preset: preset_name(eq, slot_names, number), bands: bands.clone() };
            if i == 0 {
                profile.game_eq = Some(setting);
            } else {
                profile.mic_eq = Some(setting);
            }
        }
        profile
    }

    /// Whether the profile holds no setting at all.
    pub fn is_empty(&self) -> bool {
        self.settings.is_empty() && self.game_eq.is_none() && self.mic_eq.is_none()
    }

    /// What is wrong with a profile read from a file: an unknown setting, or a value the headset does not take.
    pub fn problem(&self) -> Option<String> {
        for (name, setting) in &self.settings {
            if !SETTINGS.contains(&name.as_str()) {
                return Some(format!("unknown setting {name:?}"));
            }
            match (values::is_flag(name), setting) {
                (true, Setting::Int(_)) => return Some(format!("{name} is true or false")),
                (false, Setting::Bool(_)) => return Some(format!("{name} is a number")),
                _ => {}
            }
            let accepted = values::key_of(name)
                .and_then(|key| keys::writable(key).map(|s| s.values.accepts(setting.number())))
                .unwrap_or(false);
            if !accepted {
                return Some(format!("{name} cannot be {}", setting.number()));
            }
        }
        for (what, eq) in [("game_eq", &self.game_eq), ("mic_eq", &self.mic_eq)] {
            if let Some(eq) = eq
                && (eq.bands.len() != keys::BAND_COUNT || eq.bands.iter().any(|b| !(-90..=90).contains(b)))
            {
                return Some(format!("{what} needs 10 bands from -90 to 90"));
            }
        }
        None
    }

    /// The writes that make the headset match the profile, given its current values; none when it matches.
    pub fn plan(&self, current: &BTreeMap<&str, Typed>) -> Vec<Step> {
        let mut steps = Vec::new();
        for (i, (eq, preset, bands, slots)) in EQUALIZERS.into_iter().enumerate() {
            let Some(wanted) = (if i == 0 { &self.game_eq } else { &self.mic_eq }) else { continue };
            let slot_names = match current.get(slots) {
                Some(Typed::Texts(names)) => names.as_slice(),
                _ => &[],
            };
            let now_preset = match current.get(preset) {
                Some(Typed::Int(n)) => Some(*n),
                _ => None,
            };
            let now_bands = match current.get(bands) {
                Some(Typed::Ints(b)) => b.as_slice(),
                _ => &[],
            };
            match wanted.preset.as_deref().and_then(|name| preset_number(eq, slot_names, name)) {
                Some(number) if now_preset != Some(number) => steps.push(Step::Set(preset, number)),
                Some(_) => {}
                // unsaved bands, or a custom preset that is gone: write the bands
                None => {
                    for (band, &tenths) in wanted.bands.iter().enumerate().take(keys::BAND_COUNT) {
                        if now_bands.get(band) != Some(&tenths) {
                            steps.push(Step::Band(eq, band, tenths));
                        }
                    }
                }
            }
        }
        let eq_changes = !steps.is_empty();
        for name in SETTINGS {
            let Some(&wanted) = self.settings.get(name) else { continue };
            let now = match current.get(name) {
                Some(Typed::Bool(b)) => Some(i64::from(*b)),
                Some(Typed::Int(n)) => Some(*n),
                _ => None,
            };
            // a preset change switches Superhuman Hearing off, so it is switched on again afterwards
            let forced = name == "superhuman" && eq_changes && wanted.number() == 1;
            if now != Some(wanted.number()) || forced {
                steps.push(Step::Set(name, wanted.number()));
            }
        }
        steps
    }
}

/// The name of preset `number` of an equalizer: an onboard one, or the custom slot's. `None` for unsaved bands.
fn preset_name(eq: &EqKeys, slots: &[String], number: i64) -> Option<String> {
    if let Some((_, label)) = eq.presets.iter().find(|(n, _)| *n == number) {
        return Some(label.to_string());
    }
    let slot = usize::try_from(number.checked_sub(keys::FIRST_CUSTOM_PRESET)?).ok()?;
    slots.get(slot).filter(|name| !name.is_empty()).cloned()
}

/// The number of the preset called `name`: an onboard one first, then the saved custom ones.
fn preset_number(eq: &EqKeys, slots: &[String], name: &str) -> Option<i64> {
    if let Some((n, _)) = eq.presets.iter().find(|(_, label)| *label == name) {
        return Some(*n);
    }
    let slot = slots.iter().position(|s| !s.is_empty() && s == name)?;
    Some(keys::FIRST_CUSTOM_PRESET + slot as i64)
}

/// Why `name` cannot name a profile, or "" if it can (or is empty). Profiles are kept as `<name>.toml`.
pub fn name_problem(name: &str) -> &'static str {
    if name.chars().count() > NAME_MAX {
        "Use at most 32 characters."
    } else if name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        "A name can't contain / or \\."
    } else if name.starts_with('.') {
        "A name can't start with a dot."
    } else if name != name.trim() {
        "A name can't start or end with a space."
    } else {
        ""
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn current() -> BTreeMap<&'static str, Typed> {
        BTreeMap::from([
            ("anc", Typed::Bool(true)),
            ("anc_level", Typed::Int(100)),
            ("superhuman", Typed::Bool(false)),
            ("superhuman_mode", Typed::Int(1)),
            ("superhuman_level", Typed::Int(60)),
            ("mic_sensitivity", Typed::Int(84)),
            ("monitoring_level", Typed::Int(65)),
            ("noise_gate", Typed::Bool(false)),
            ("noise_gate_level", Typed::Int(100)),
            ("ai_noise_reduction", Typed::Bool(false)),
            ("button_function", Typed::Int(0)),
            ("wheel_function", Typed::Int(1)),
            ("volume", Typed::Int(42)),
            ("game_eq_preset", Typed::Int(2)),
            ("game_eq_bands", Typed::Ints(vec![50, 50, 35, 0, 0, 0, 0, 0, 0, 0])),
            ("game_eq_slots", Typed::Texts(vec!["".into(), "FPS".into(), "".into(), "".into(), "".into()])),
            ("mic_eq_preset", Typed::Int(0)),
            ("mic_eq_bands", Typed::Ints(vec![5, 0, 0, 0, 0, 0, 0, 0, 0, -5])),
            ("mic_eq_slots", Typed::Texts(vec!["".into(); 5])),
        ])
    }

    #[test]
    fn captures_settings_and_equalizers() {
        let p = Profile::capture(&current());
        assert_eq!(p.settings.len(), SETTINGS.len());
        assert_eq!(p.settings["anc"], Setting::Bool(true));
        assert_eq!(p.settings["anc_level"], Setting::Int(100));
        assert!(!p.settings.contains_key("volume"), "the volume belongs to the headset, not the profile");
        assert_eq!(p.game_eq.as_ref().unwrap().preset.as_deref(), Some("Bass Boost"));
        assert_eq!(p.mic_eq.as_ref().unwrap().preset, None, "unsaved bands have no preset name");
        assert_eq!(p.problem(), None);
    }

    #[test]
    fn matching_state_needs_no_writes() {
        let now = current();
        let p = Profile::capture(&now);
        assert_eq!(p.plan(&now), []);
        assert!(!p.plan(&BTreeMap::new()).is_empty(), "nothing matches while no headset is there");
    }

    #[test]
    fn plans_only_what_differs() {
        let mut p = Profile::capture(&current());
        p.settings.insert("anc".into(), Setting::Bool(false));
        p.settings.insert("noise_gate_level".into(), Setting::Int(40));
        p.mic_eq.as_mut().unwrap().bands[9] = 0;
        assert_eq!(
            p.plan(&current()),
            [Step::Band(&keys::MIC_EQ, 9, 0), Step::Set("anc", 0), Step::Set("noise_gate_level", 40)]
        );
    }

    #[test]
    fn selects_presets_by_name() {
        let mut p = Profile::capture(&current());
        p.game_eq = Some(EqSetting { preset: Some("FPS".into()), bands: vec![0; 10] });
        assert_eq!(p.plan(&current()), [Step::Set("game_eq_preset", 17)], "the custom preset in slot 2");

        // a custom preset that was deleted since: its bands are written instead
        let mut gone = current();
        gone.insert("game_eq_slots", Typed::Texts(vec!["".into(); 5]));
        let steps = p.plan(&gone);
        assert_eq!(steps[..3], [Step::Band(&keys::GAME_EQ, 0, 0), Step::Band(&keys::GAME_EQ, 1, 0), Step::Band(&keys::GAME_EQ, 2, 0)]);
        assert_eq!(steps.len(), 3);
    }

    #[test]
    fn superhuman_hearing_comes_back_after_a_preset_change() {
        let mut now = current();
        now.insert("superhuman", Typed::Bool(true));
        let mut p = Profile::capture(&now);
        p.game_eq.as_mut().unwrap().preset = Some("Vocal Boost".into());
        assert_eq!(p.plan(&now), [Step::Set("game_eq_preset", 4), Step::Set("superhuman", 1)]);
    }

    #[test]
    fn checks_files() {
        let mut p = Profile::default();
        p.settings.insert("volume".into(), Setting::Int(10));
        assert_eq!(p.problem().as_deref(), Some("unknown setting \"volume\""));
        let mut p = Profile::default();
        p.settings.insert("anc_level".into(), Setting::Int(101));
        assert_eq!(p.problem().as_deref(), Some("anc_level cannot be 101"));
        let mut p = Profile::default();
        p.settings.insert("superhuman".into(), Setting::Int(1));
        assert_eq!(p.problem().as_deref(), Some("superhuman is true or false"));
        let mut p = Profile::default();
        p.settings.insert("anc_level".into(), Setting::Bool(true));
        assert_eq!(p.problem().as_deref(), Some("anc_level is a number"));
        // more than 10 bands never reach the keys after the last band
        let p = Profile { game_eq: Some(EqSetting { preset: None, bands: vec![5; 17] }), ..Profile::default() };
        assert!(p.problem().is_some());
        assert_eq!(p.plan(&BTreeMap::new()).len(), keys::BAND_COUNT);
        let p = Profile { game_eq: Some(EqSetting { preset: None, bands: vec![0; 9] }), ..Profile::default() };
        assert!(p.problem().is_some());
    }

    #[test]
    fn names() {
        assert_eq!(name_problem("Gaming"), "");
        assert_eq!(name_problem("Late-night calls"), "");
        assert_ne!(name_problem("a/b"), "");
        assert_ne!(name_problem(".hidden"), "");
        assert_ne!(name_problem(" padded"), "");
        assert_ne!(name_problem(&"x".repeat(33)), "");
    }
}
