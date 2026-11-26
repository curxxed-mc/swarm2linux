# Profiles

A profile is a named set of settings, such as "Gaming" or "Calls", that you can switch to in one go: from the
Device page of either app, the tray menu, or `stealthctl profile apply Gaming`.

It holds noise cancellation, Superhuman Hearing, the microphone settings (sensitivity, monitoring, noise gate, AI
noise reduction), both equalizers, and the button and wheel functions. It leaves out the volume, auto shut-off,
wake on motion and the charging hub's LEDs. Applying writes only the settings that differ. An equalizer is kept as
its preset's name and its bands: if the headset still has a preset by that name it is selected, otherwise the
bands are written.

The profile you choose stays selected (also across restarts). When you change a setting afterwards, it is marked
modified, in the app's header, on the Device page and in the tray: **Save** keeps the changes in the profile, and
**Revert** puts the profile's settings back. In the terminal, `stealthctl profile save` (without a name) and
`stealthctl profile revert` do the same.

## Files

`stealthd` keeps profiles as TOML files in `~/.config/swarm2linux/profiles/`, one per profile, named after it. You
can edit them by hand or copy them to another computer. A profile can also hold just a few settings:

```toml
# ~/.config/swarm2linux/profiles/Quiet.toml
anc = true
anc_level = 100
superhuman = false
```
