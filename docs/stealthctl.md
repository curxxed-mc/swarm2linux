# stealthctl (command line)

`stealthctl` goes through [`stealthd`](stealthd.md) when it runs, and opens the headset directly otherwise.
`stealthctl --help` and `stealthctl <command> --help` list everything.

```sh
stealthctl doctor                 # check the setup and say how to fix problems (works without stealthd)
stealthctl status                 # battery, transmitters, volume, both EQs, Superhuman Hearing, ANC, mic
stealthctl volume 40              # the headset's volume in the system's sound settings, 0-100
stealthctl eq list                # game EQ: bands with frequencies, built-in and custom presets
stealthctl eq preset bass         # signature | bass | bass-treble | vocal | <custom preset name>
stealthctl eq band 1 -3.5         # dB in steps of 0.5
stealthctl eq save MyPreset       # save the current bands as a custom preset (5 slots); eq delete MyPreset
stealthctl eq update MyPreset     # replace a custom preset's bands with the current bands
stealthctl mic-eq preset clarity  # signature | full | clarity | smooth; band/save/update/delete as for eq
stealthctl shh on                 # Superhuman Hearing; off, mode legacy|footsteps|gunshots, level 0-100
stealthctl anc on                 # off, level 0-100 (0 = ambient, 100 = full noise cancellation)
stealthctl monitor level 60       # how loud you hear your own voice
stealthctl mic gate on            # mic: sensitivity N, gate on|off, gate-level N, ai on|off
stealthctl button cycle-eq        # top left button: anc | cycle-eq | noise-gate
stealthctl wheel chat-mix         # assignable wheel: monitoring | chat-mix | bass | treble | noise-gate
stealthctl power auto-off 20      # never | 5 | 10 | 20 | 30; power wake-on-motion on|off
stealthctl hub led1 30            # charging hub LED brightness: led1 | led2 0-100
stealthctl profile save Gaming    # save the current settings as a profile; list | apply NAME | revert | delete NAME
stealthctl voice on               # voice commands: on | off | mic [N|NAME|default]; status without arguments
stealthctl watch                  # print changes as they happen (buttons, wheel, battery, CrossPlay)
stealthctl snapshot my.json       # save everything; `restore my.json` writes the settings back
stealthctl dump                   # every record as JSON
```

Set `RACE_HID_TRACE=1` to print every HID report.
