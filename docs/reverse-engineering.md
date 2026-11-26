# Reverse-engineering tools

On Linux, audio already works through `snd-usb-audio`: the transmitter is a standard USB Audio Class 1 device. All
headset settings (EQ, ANC, mic, sidetone, buttons) live on the headset and the transmitter and are changed over a
vendor HID collection (usage page `0xFF13`). That collection carries Airoha RACE packets, and Turtle Beach tunnels a
JSON key/value protocol through them. [protocol.md](protocol.md) describes what is known of it, and
[PLAN.md](PLAN.md) how it was worked out.

`tools/re/` holds dependency-free Python scripts that talk to `/dev/hidrawN` directly:

```sh
python3 tools/re/probe.py            # read-only: open a session and dump the device state
python3 tools/re/monitor.py --session  # log every packet the device pushes (survives replugging)
python3 tools/re/decode_log.py --changes captures/linux/monitor-*.jsonl  # replay a saved log
python3 tools/re/pcap2jsonl.py capture.pcapng -o capture.jsonl  # Windows USBPcap capture -> same log format
python3 tools/re/kvp.py dump --save    # read all 26 records, save a snapshot
python3 tools/re/kvp.py watch 60       # subscribe to everything and print pushed changes
python3 tools/re/kvp.py set 0x1210 2   # write a key (only keys/values Swarm II was seen writing)
```

The hidraw node must be readable and writable by your user; `install.sh` installs `packaging/70-swarm2linux.rules`
for that. Stop `stealthd` first (`systemctl --user stop stealthd`), since it holds the session.

The Windows capture checklists ([windows-harvest-checklist.md](windows-harvest-checklist.md),
[round 2](windows-harvest-2.md), [round 3](windows-harvest-3.md)) list what was recorded from Swarm II.
