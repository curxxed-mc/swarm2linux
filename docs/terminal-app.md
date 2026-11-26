# stealth-tui (terminal app)

| Settings | Equalizer |
|---|---|
| ![The terminal app's Settings page: master volume, Superhuman Hearing, noise cancellation and the microphone](screenshots/tui-settings.png) | ![The terminal app's Equalizer page: a braille curve over ten bands, and the presets](screenshots/tui-equalizer.png) |
| **Controls** | **Device** |
| ![The terminal app's Controls page: the left earcup's controls and the mode button's functions](screenshots/tui-controls.png) | ![The terminal app's Device page: battery, transmitter, power options, charging hub, notifications](screenshots/tui-device.png) |

The same four pages as the [desktop app](desktop-app.md) in a terminal, built with [ratatui](https://ratatui.rs).
Like the desktop app it is a client of [`stealthd`](stealthd.md) and needs it running. It works with vim keys, the
arrow keys and the mouse; `?` shows every key.

| Keys | Action |
|---|---|
| `j` `k` / `↓` `↑` | next / previous control; with a count, `3j` |
| `h` `l` / `←` `→` | change by one: a slider, the option, off / on (`5l` changes by five) |
| `Ctrl-←` `Ctrl-→` | change by ten (with vim keys, a count: `10l`) |
| `0` `$` | minimum / maximum (`$` does not apply to the master volume) |
| `gg` `G` / `Home` `End` | first / last control |
| `Enter` `Space` | toggle, or open the options |
| `Tab` `Shift-Tab` | next / previous card |
| `H` `L` / `Shift-←` `Shift-→` | previous / next page (also `gT` `gt`, `[` `]`; `2gt` goes to page 2) |
| `q` `:q` `Ctrl-c` | quit |

## Equalizer

On the graph, `h` `l` choose a band and `k` `j` raise and lower it by 0.5 dB (`K` `J` by 3 dB); `=` sets it to 0 dB,
`t` switches between the game and mic EQ, and `u` undoes the changes. `s` saves the bands as a new preset; after
changing a custom preset's bands, `w` updates that preset instead. In the preset list, `Enter` chooses a preset and
`dd` deletes a custom one.

## Device page

`Enter` on Profile lists the saved [profiles](profiles.md) (`Enter` applies one, `dd` deletes it), and `Enter` on
Save current settings saves a new one. The Voice commands card switches [voice commands](voice-commands.md) on and
off; choose their microphone in the desktop app or with `stealthctl voice mic`.

## Mouse and terminal

Click tabs, controls, options and presets, click or drag sliders and EQ bands, and turn the wheel over a slider or
band. Sliders and bands are written to the headset once they have rested for 300 ms, so holding a key down does not
flood it. The terminal needs to be at least 80×24; from 100 columns on, the cards sit in two columns. Colours use
truecolor when `COLORTERM` says the terminal has it, otherwise 256 colours.
