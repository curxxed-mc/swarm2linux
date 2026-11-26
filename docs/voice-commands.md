# Voice commands (experimental)

Turn on **Voice commands** on the Device page of either app (or `stealthctl voice on`) and say "Stealth" followed by
a command:

| Say | Does |
|-----|------|
| "Stealth, skip this song" / "next" | next track in the playing media player |
| "Stealth, go back" / "previous song" | previous track |
| "Stealth, pause this video" / "stop" | pause |
| "Stealth, resume" / "play" / "continue" | play (the player paused last) |
| "Stealth, volume up" / "louder", "volume down" / "quieter" | the default output's volume, 5 % a step |
| "Stealth, noise cancelling on" / "off" | the headset's noise cancellation |

Media players are controlled over MPRIS, which Spotify, VLC, mpv (with `mpv-mpris`), and Firefox and Chromium for
web video support. Each command shows a short pop-up.

## Microphone

It listens on the system's default microphone unless you choose another one under **Microphone** in the desktop
app's Voice commands card. The choice applies only to voice commands. From a terminal:

```sh
stealthctl voice              # on or off, what it is doing, and the microphone
stealthctl voice on           # or off
stealthctl voice mic          # list the microphones; * marks the chosen one
stealthctl voice mic 1        # choose by number, by (part of) its name, or `default`
```

## How it works

`stealthd` runs the separate program `stealth-voice` while voice commands are on, and restarts it if it stops.
`stealth-voice` records with `pw-record`, cuts what is said out of the background by loudness, and recognises it on
this computer with [whisper.cpp](https://github.com/ggml-org/whisper.cpp): nothing is sent anywhere. A command takes
about half a second to recognise on a laptop CPU.

On first use it downloads the 57 MB English model `ggml-base.en-q5_1.bin` from Hugging Face into
`~/.local/share/swarm2linux/` and checks its SHA-256. Building `stealth-voice` needs cmake and a C++ compiler;
`install.sh` skips it without them, and the switch then says it is not installed.

## When it misses commands

- **Check the microphone.** If it stays silent for 30 seconds, the status says so: it may be muted, or (for the
  headset's own mic) its sensitivity may be at 0. Make sure the right microphone is chosen.
- **See what it heard.** Everything heard but not acted on goes to stealthd's log:
  `journalctl --user -u stealthd -f`. Or turn the switch off and run `stealth-voice --dry-run`, which prints what it
  hears without acting on it.
- **A noisy microphone.** A laptop's built-in microphone with its boost turned all the way up records mostly noise.
  Turning "Internal Mic Boost" down (in `alsamixer`, `F4` for capture) helps recognition, and calls too.

The model sometimes writes "Stealth" as "Third" or "Steal"; those count as the wake word when they come first.
