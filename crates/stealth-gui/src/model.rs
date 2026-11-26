//! Values from stealthd into the `Headset` global, and the option lists from `tb_proto::keys`.

use std::collections::BTreeMap;

use slint::{Model, ModelRc, SharedString, VecModel};
use stealth_dbus::view::{self, EqEdit, capitalized};
use tb_proto::keys;
use tb_proto::values::Typed;

use crate::{Choice, Headset};

/// Where a value goes in the `Headset` global.
#[derive(Clone, Copy)]
enum Target {
    Bool(fn(&Headset<'_>, bool)),
    Int(fn(&Headset<'_>, i32)),
    Text(fn(&Headset<'_>, SharedString)),
    Ints(fn(&Headset<'_>, ModelRc<i32>)),
    Texts(fn(&Headset<'_>, ModelRc<SharedString>)),
    /// Shown in its own way.
    Other(fn(&Headset<'_>, &Typed)),
}

fn target(name: &str) -> Option<Target> {
    use Target::*;
    Some(match name {
        "connected" => Bool(|h, v| h.set_connected(v)),
        "device" => Text(|h, v| h.set_device(v)),
        "headset_link" => Bool(|h, v| h.set_headset_link(v)),
        "name" => Text(|h, v| h.set_name(v)),
        "firmware" => Text(|h, v| h.set_firmware(v)),
        "battery" => Int(|h, v| h.set_battery(v)),
        "charging" => Bool(|h, v| h.set_charging(v)),
        "volume" => Int(|h, v| h.set_volume(v)),
        "sink_volume" => Int(|h, v| h.set_sink_volume(v)),
        "mic_muted" => Bool(|h, v| h.set_mic_muted(v)),
        "transmitters" => Other(show_transmitters),
        "active_transmitter" => Int(|h, v| h.set_active_transmitter(v)),
        "anc" => Bool(|h, v| h.set_anc(v)),
        "anc_level" => Int(|h, v| h.set_anc_level(v)),
        "superhuman" => Bool(|h, v| h.set_superhuman(v)),
        "superhuman_mode" => Int(|h, v| h.set_superhuman_mode(v)),
        "superhuman_level" => Int(|h, v| h.set_superhuman_level(v)),
        "mic_sensitivity" => Int(|h, v| h.set_mic_sensitivity(v)),
        "monitoring_level" => Int(|h, v| h.set_monitoring_level(v)),
        "monitoring_enabled" => Bool(|h, v| h.set_monitoring_enabled(v)),
        "noise_gate" => Bool(|h, v| h.set_noise_gate(v)),
        "noise_gate_level" => Int(|h, v| h.set_noise_gate_level(v)),
        "ai_noise_reduction" => Bool(|h, v| h.set_ai_noise_reduction(v)),
        "button_function" => Int(|h, v| h.set_button_function(v)),
        "wheel_function" => Int(|h, v| h.set_wheel_function(v)),
        "auto_shutoff" => Int(|h, v| h.set_auto_shutoff(v)),
        "wake_on_motion" => Bool(|h, v| h.set_wake_on_motion(v)),
        "hub_led_1" => Int(|h, v| h.set_hub_led_1(v)),
        "hub_led_2" => Int(|h, v| h.set_hub_led_2(v)),
        "game_eq_preset" => Int(|h, v| h.set_game_eq_preset(v)),
        "game_eq_name" => Text(|h, v| h.set_game_eq_name(v)),
        "game_eq_bands" => Ints(|h, v| h.set_game_eq_bands(v)),
        "game_eq_slots" => Texts(|h, v| h.set_game_eq_slots(v)),
        "mic_eq_preset" => Int(|h, v| h.set_mic_eq_preset(v)),
        "mic_eq_name" => Text(|h, v| h.set_mic_eq_name(v)),
        "mic_eq_bands" => Ints(|h, v| h.set_mic_eq_bands(v)),
        "mic_eq_slots" => Texts(|h, v| h.set_mic_eq_slots(v)),
        "profiles" => Texts(|h, v| h.set_profiles(v)),
        "active_profile" => Text(|h, v| h.set_active_profile(v)),
        "profile_modified" => Bool(|h, v| h.set_profile_modified(v)),
        "voice_commands" => Bool(|h, v| h.set_voice_commands(v)),
        "voice_status" => Text(|h, v| h.set_voice_status(v)),
        "voice_microphone" => Other(|h, v| {
            if let Typed::Text(name) = v {
                h.set_voice_microphone(name.into());
            }
            show_microphones(h);
        }),
        "voice_microphones" => Other(|h, v| {
            match v {
                Typed::Texts(list) => h.set_voice_microphones(texts(list)),
                _ => h.set_voice_microphones(texts::<&str>(&[])),
            }
            show_microphones(h);
        }),
        _ => return None,
    })
}

/// Show values from stealthd: all of them, or the changed ones.
pub fn apply(h: &Headset<'_>, values: &BTreeMap<String, Typed>) {
    for (name, value) in values {
        match (target(name), value) {
            (Some(Target::Bool(set)), Typed::Bool(v)) => set(h, *v),
            (Some(Target::Int(set)), Typed::Int(v)) => set(h, *v as i32),
            (Some(Target::Text(set)), Typed::Text(v)) => set(h, v.into()),
            (Some(Target::Ints(set)), Typed::Ints(v)) => set(h, ints(v)),
            (Some(Target::Texts(set)), Typed::Texts(v)) => set(h, texts(v)),
            (Some(Target::Other(show)), v) => show(h, v),
            // an empty list cannot say what it holds and arrives as integers
            (Some(Target::Texts(set)), Typed::Ints(v)) if v.is_empty() => set(h, texts::<&str>(&[])),
            _ => eprintln!("stealth-gui: ignoring {name} = {value:?}"),
        }
    }
}

fn show_transmitters(h: &Headset<'_>, value: &Typed) {
    let entries: &[String] = match value {
        Typed::Texts(list) => list,
        _ => &[],
    };
    let (lines, hub) = view::transmitter_lines(entries);
    h.set_transmitters(texts(&lines));
    h.set_hub_status(hub.into());
}

/// The microphone choices for voice commands, from stealthd's list and choice.
fn show_microphones(h: &Headset<'_>) {
    let list: Vec<SharedString> = h.get_voice_microphones().iter().collect();
    let (choices, at) = view::microphone_choices(&list, &h.get_voice_microphone());
    let items: Vec<Choice> =
        choices.iter().enumerate().map(|(i, (_, label))| Choice { value: i as i32, label: label.into() }).collect();
    h.set_voice_mic_choices(ModelRc::new(VecModel::from(items)));
    h.set_voice_mic_names(texts(&choices.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>()));
    h.set_voice_mic_index(at as i32);
}

/// Fill the option lists.
pub fn init(h: &Headset<'_>) {
    let modes = keys::SUPERHUMAN_MODES.iter().enumerate().map(|(i, &label)| (i as i64, label));
    h.set_superhuman_modes(choices(modes));
    h.set_button_functions(choices(keys::BUTTON_FUNCTIONS));
    h.set_wheel_functions(choices(keys::WHEEL_FUNCTIONS));
    h.set_auto_shutoff_options(choices(keys::AUTO_SHUTOFF_OPTIONS));
    h.set_game_eq_presets(choices(keys::GAME_EQ.presets));
    h.set_mic_eq_presets(choices(keys::MIC_EQ.presets));
    h.set_game_eq_frequencies(texts(&keys::GAME_EQ.frequencies));
    h.set_mic_eq_frequencies(texts(&keys::MIC_EQ.frequencies));
}

/// Why `name` cannot be used for a new preset of the "game" or "mic" equalizer, or "" if it can (or is empty).
pub fn preset_name_problem(h: &Headset<'_>, eq: &str, name: &str) -> &'static str {
    let slots = if eq == "game" { h.get_game_eq_slots() } else { h.get_mic_eq_slots() };
    view::preset_name_problem(&slots.iter().collect::<Vec<_>>(), name)
}

/// The preset and bands shown for the "game" or "mic" equalizer.
pub fn equalizer(h: &Headset<'_>, eq: &str) -> EqEdit {
    if eq == "game" {
        EqEdit { preset: h.get_game_eq_preset(), bands: h.get_game_eq_bands().iter().collect() }
    } else {
        EqEdit { preset: h.get_mic_eq_preset(), bands: h.get_mic_eq_bands().iter().collect() }
    }
}

/// Show a preset and bands for the "game" or "mic" equalizer.
pub fn show_equalizer(h: &Headset<'_>, eq: &str, preset: i32, bands: &[i32]) {
    let bands = ModelRc::new(VecModel::from(bands.to_vec()));
    if eq == "game" {
        h.set_game_eq_preset(preset);
        h.set_game_eq_bands(bands);
    } else {
        h.set_mic_eq_preset(preset);
        h.set_mic_eq_bands(bands);
    }
}

fn choices(list: impl IntoIterator<Item = (i64, &'static str)>) -> ModelRc<Choice> {
    let items: Vec<Choice> =
        list.into_iter().map(|(value, label)| Choice { value: value as i32, label: capitalized(label).into() }).collect();
    ModelRc::new(VecModel::from(items))
}

fn ints(list: &[i64]) -> ModelRc<i32> {
    ModelRc::new(VecModel::from(list.iter().map(|&n| n as i32).collect::<Vec<_>>()))
}

fn texts<S: AsRef<str>>(list: &[S]) -> ModelRc<SharedString> {
    ModelRc::new(VecModel::from(list.iter().map(|s| SharedString::from(s.as_ref())).collect::<Vec<_>>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_value_has_a_place() {
        for name in tb_proto::values::NAMES.iter().chain(stealth_dbus::DAEMON_NAMES.iter()) {
            assert!(target(name).is_some(), "{name} is not shown");
        }
    }
}
