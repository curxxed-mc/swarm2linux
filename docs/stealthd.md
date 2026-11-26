# stealthd (background service)

`stealthd` owns the headset: it finds it (preferring the transmitter over the headset's own cable), keeps a session
with every record subscribed, follows unplugging, and serves everything on the session bus. The apps and
`stealthctl` go through it. `install.sh` runs it as the systemd user service `stealthd`; its log is
`journalctl --user -u stealthd`.

## What it does besides serving the headset

- **Battery warnings.** A notification when the battery drops to 15 % and 5 % while not charging.
- **Level pop-ups.** When the assignable wheel on the headset turns mic monitoring or the noise gate level, it shows
  the new percentage on screen like the volume (KDE Plasma's volume pop-up, a notification elsewhere).
- **Volume.** The headset's volume belongs to PipeWire: the headset reports it but takes no writes for it. So
  `stealthd` finds the headset's sound output in PipeWire (with `pw-dump`), serves its volume in percent as
  `sink_volume`, and sets it with `wpctl`. Without those two tools, the volume is shown but can't be changed.
- **AI noise reduction.** After a restart the headset can keep AI noise reduction working while it reports it off.
  Whenever `stealthd` connects and finds it reported off, it switches it on and off once, which ends that.
- **Profiles** ([profiles](profiles.md)) and **voice commands** ([voice commands](voice-commands.md)).

## Settings

Kept in `~/.config/swarm2linux/stealthd.toml`:

| Key | Default | Meaning |
|---|---|---|
| `battery_notifications` | `true` | battery warnings (also the `BatteryNotifications` property) |
| `level_popups` | `true` | the wheel's level pop-ups |
| `profile` | `""` | the selected profile |
| `voice_commands` | `false` | voice commands on or off |
| `voice_microphone` | `""` | the voice commands' microphone (a PipeWire node name; `""` for the default) |

## D-Bus

Bus name `io.github.swarm2linux.Stealthd`, object `/io/github/swarm2linux/Headset`, interface
`io.github.swarm2linux.Headset1`:

| Member | |
|---|---|
| `Values() -> a{sv}` | every value by name |
| `Set(s name, v value)` | set a boolean or integer value |
| `SetBand(s eq, u band, i tenths)` | one band (1–10) of `"game"` or `"mic"`, in tenths of a dB |
| `SavePreset(s eq, s name)`, `UpdatePreset(s eq, s name)`, `DeletePreset(s eq, s name)` | custom EQ presets |
| `SaveProfile(s)`, `ApplyProfile(s)`, `DeleteProfile(s)` | profiles |
| `SetVoiceMicrophone(s name)` | the voice commands' microphone |
| signal `Changed(a{sv})` | the values that changed |
| properties `Connected` (b), `BatteryNotifications` (b, writable), `Version` (s) | |

Value names are listed in `crates/tb-proto/src/values.rs`, plus these from the daemon: `connected`, `device`,
`headset_link`, `sink_volume`, `profiles`, `active_profile`, `profile_modified`, `voice_commands`, `voice_status`,
`voice_microphone` and `voice_microphones`.

```sh
busctl --user call io.github.swarm2linux.Stealthd /io/github/swarm2linux/Headset \
    io.github.swarm2linux.Headset1 Set sv anc b true
```

When `stealthd` runs, `stealthctl`'s raw commands (`dump`, `get`, `set`, `snapshot`, `restore`) need direct access
and ask you to stop the service first.
