# Installing swarm2linux

You need Linux with a systemd user session and PipeWire, such as Fedora, Ubuntu, Debian, Arch or openSUSE with
KDE Plasma or GNOME, and the headset's USB transmitter (or its USB-C cable).

There are two ways to install. Both put the same files in the same places, so you can update or uninstall either
way:

- [**With the script**](#with-the-script): `install.sh` runs every step for you.
- [**Step by step**](#step-by-step): you run each command yourself and see what it does.

## With the script

```sh
git clone https://github.com/curxxed-mc/swarm2linux.git
cd swarm2linux
./install.sh
```

The script installs everything for your user and asks before it changes anything outside your home folder:

1. **Build tools.** If they are missing, it installs a C compiler, pkg-config and the fontconfig headers with dnf,
   apt, pacman or zypper.
2. **Rust.** If Rust 1.92 or newer is missing, it installs it into `~/.cargo` with [rustup](https://rustup.rs).
3. **Headset access.** It installs the udev rule `/etc/udev/rules.d/70-swarm2linux.rules`, so your user can open
   the headset. This step needs your sudo password.
4. **Build.** It builds everything, which takes a few minutes the first time. When cmake and a C++ compiler are
   present it also builds `stealth-voice` for the [voice commands](voice-commands.md); without them it skips it.
5. **Install.** It puts the programs into `~/.local/bin`: `stealth-gui` (the desktop app), `stealth-tui` (the
   terminal app), `stealthd` (the background service), `stealthctl` (the command line tool) and `stealth-voice`.
   It adds the **swarm2linux** entry to the app menu and starts `stealthd` now and with every login.

Then open **swarm2linux** from the app menu, or run `stealth-tui` in a terminal. For the tray icon after login,
turn on **Start with the session** on the desktop app's Device page.

| Task | Command |
|---|---|
| Update | `git pull && ./install.sh` |
| Uninstall | `./install.sh --uninstall` |
| Answer yes to every question | `./install.sh --yes` |
| Leave the udev rule alone | `./install.sh --no-udev` |

Uninstalling removes the programs, the menu entry, the service, the autostart entry, the voice commands' speech
model and the udev rule. It keeps your app preferences in `~/.config/swarm2linux` and leaves Rust installed. The
headset keeps its settings, because they are stored on the headset.

The script was tested on Fedora 44 and Ubuntu 26.04 with KDE Plasma, and on clean Fedora 44 and Ubuntu 24.04
containers. It does not support immutable systems such as Bazzite, Fedora Kinoite or Silverblue, and SteamOS yet:
those can't install the build tools with a normal package manager.

## Step by step

These are the steps the script takes. Run them as your normal user; `sudo` asks for your password where a step
needs root.

### 1. Build tools and Rust

| Distribution | Command |
|---|---|
| Fedora | `sudo dnf install git gcc pkgconf-pkg-config fontconfig-devel rust cargo` |
| Ubuntu, Debian, Linux Mint, Pop!_OS | `sudo apt update && sudo apt install git curl build-essential pkg-config libfontconfig-dev` |
| Arch Linux, Manjaro, EndeavourOS | `sudo pacman -S --needed git base-devel fontconfig rust` |
| openSUSE Tumbleweed | `sudo zypper install git gcc pkg-config fontconfig-devel rust cargo` |
| openSUSE Leap | `sudo zypper install git gcc pkg-config fontconfig-devel` |

On Ubuntu, Debian, Mint, Pop!_OS and openSUSE Leap the distribution's Rust is too old, so install it with rustup.
It asks before it installs, and its default answer is fine. The second line makes `cargo` work in this terminal;
new terminals find it by themselves.

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
. "$HOME/.cargo/env"
```

For the optional voice commands, also install cmake and a C++ compiler: `cmake gcc-c++` (Fedora, openSUSE),
`cmake g++` (Ubuntu and relatives) or `cmake` (Arch, where `base-devel` has the compiler).

### 2. Source and build

```sh
git clone https://github.com/curxxed-mc/swarm2linux.git
cd swarm2linux
cargo build --release --locked
cargo build --release --locked -p stealth-voice    # optional: voice commands
```

### 3. Headset access

```sh
sudo install -Dm644 packaging/70-swarm2linux.rules /etc/udev/rules.d/70-swarm2linux.rules
sudo udevadm control --reload
sudo udevadm trigger --subsystem-match=hidraw --action=change
```

### 4. Programs, service, menu entry and icon

```sh
install -Dm755 -t ~/.local/bin target/release/{stealthctl,stealthd,stealth-gui,stealth-tui}
install -Dm755 -t ~/.local/bin target/release/stealth-voice    # if you built it
install -Dm644 packaging/stealthd.service ~/.config/systemd/user/stealthd.service
systemctl --user daemon-reload
systemctl --user enable --now stealthd
mkdir -p ~/.local/share/applications
sed "s|^Exec=stealth-gui|Exec=\"$HOME/.local/bin/stealth-gui\"|" packaging/io.github.swarm2linux.desktop \
    > ~/.local/share/applications/io.github.swarm2linux.desktop
install -Dm644 packaging/icons/io.github.swarm2linux.svg \
    ~/.local/share/icons/hicolor/scalable/apps/io.github.swarm2linux.svg
```

### 5. Check it

```sh
systemctl --user is-active stealthd    # prints "active"
```

Then open **swarm2linux** from the app menu, or run `stealth-tui` in a terminal; `stealthctl status` prints what the
headset reports. If the shell can't find these commands, `~/.local/bin` is not on your PATH: run them as
`~/.local/bin/stealth-tui` and so on, or add the folder to your PATH.

## Try it without installing

After steps 1 to 3, run the programs from the build folder, each in its own terminal:

```sh
./target/release/stealthd
```

```sh
./target/release/stealth-gui    # or ./target/release/stealth-tui
```

`Ctrl-C` in the first terminal stops `stealthd`.

## Update by hand

```sh
git pull
cargo build --release --locked
install -Dm755 -t ~/.local/bin target/release/{stealthctl,stealthd,stealth-gui,stealth-tui}
systemctl --user restart stealthd
```

Then quit the desktop app from its tray menu and start it again. `./install.sh` does the same, and also brings the
service, the menu entry and the udev rule up to date in case they changed.

## Uninstall by hand

Quit the desktop app from its tray menu, then:

```sh
systemctl --user disable --now stealthd
rm -f ~/.local/bin/{stealthctl,stealthd,stealth-gui,stealth-tui,stealth-voice}
rm -f ~/.config/systemd/user/stealthd.service ~/.config/autostart/io.github.swarm2linux.desktop
rm -f ~/.local/share/applications/io.github.swarm2linux.desktop
rm -f ~/.local/share/icons/hicolor/scalable/apps/io.github.swarm2linux.svg
rm -f ~/.local/share/swarm2linux/ggml-base.en-q5_1.bin
systemctl --user daemon-reload
sudo rm -f /etc/udev/rules.d/70-swarm2linux.rules
sudo udevadm control --reload
```

This keeps your app preferences in `~/.config/swarm2linux` and Rust; `rustup self uninstall` removes a rustup
install.

## What it needs while running

The master volume uses PipeWire's `pw-dump` and `wpctl`, and voice commands use `pw-record`. Desktop installs have
them; without them the app shows the volume but can't change it. They come in these packages:

| Distribution | Packages |
|---|---|
| Fedora | `pipewire-utils wireplumber` |
| Ubuntu, Debian, Mint, Pop!_OS | `pipewire-bin wireplumber` |
| Arch, Manjaro, EndeavourOS | `pipewire wireplumber` |
| openSUSE | `pipewire-tools wireplumber` |

## Troubleshooting

Start with the self-diagnostic: `stealthctl doctor` in a terminal, or **Run checks** in the Diagnostics card at the
bottom of the desktop app's Device page. It checks the device, the udev rule and permissions, the `stealthd`
service (including one left running an old version after an update), the headset, PipeWire and the app's files, and
says how to fix each problem. Its report names no addresses or serial numbers and writes your home folder as `~`, so
you can paste it into a bug report (**Copy report** in the app).

- **The app says "stealthd is not running".** Check the service with `systemctl --user status stealthd`; its log
  is `journalctl --user -u stealthd`.
- **The headset is not found.** `stealthctl devices` lists what is plugged in. After installing the udev rule,
  unplug the transmitter and plug it in again.
- **No tray icon on GNOME.** GNOME needs the "AppIndicator and KStatusNotifierItem Support" extension.
- **Voice commands miss what you say.** See [voice commands](voice-commands.md#when-it-misses-commands).
