# Cosmic Applet Hotspot

A COSMIC panel applet that turns a Wi-Fi hotspot on and off, talking to
NetworkManager directly over D-Bus — no `nmcli` shell-outs.

## Features

- **One-click toggle** in the panel, with a distinct icon for on and off.
- **Confirmation before disconnecting you**: if there's no wired uplink and
  you're on Wi-Fi, turning the hotspot on warns you first, since it will
  drop your current connection.
- **Settings tab**: SSID, password (with a show/hide toggle), 2.4/5 GHz band,
  and an auto-off timer after N minutes with no clients connected.
- **Connected-clients list**, read from `iw`, not from a stale ARP cache.
- **Follows the system language** live (English and Portuguese today),
  including a language switch made while the popup is open.
- A single, stable NetworkManager profile (`COSMIC Hotspot`) that's reused
  and updated in place — it never accumulates duplicate `Hotspot-1`,
  `Hotspot-2`, ... profiles.
- **No built-in network name or password.** You choose both on first use;
  nothing is shipped that someone else could already know.

## Requirements

- Linux with the **COSMIC** desktop (built and tested on Pop!_OS; anywhere
  else `cosmic-panel` is installed should work).
- A Wi-Fi adapter that supports AP mode (`nmcli device show <iface>` shows
  `WIFI-PROPERTIES.AP: yes`).
- NetworkManager (the default network stack on COSMIC).

## Install

### Manually

```bash
# System libraries (apt-based distros)
sudo apt install -y build-essential pkg-config libxkbcommon-dev libinput-dev \
  libudev-dev libfontconfig-dev libwayland-dev libssl-dev just cmake iw

# Rust, if you don't already have it
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
source "$HOME/.cargo/env"

# Build and install to ~/.local (no sudo)
git clone https://github.com/Samuel-S-N/cosmic-applet-hotspot.git
cd cosmic-applet-hotspot
just install-user
```

Then add it to your panel: **Settings → Desktop → Panel → Configure panel
applets → add "Hotspot"**. If it doesn't show up right away, run
`pkill cosmic-panel` (it restarts itself) and check again.

On first use, open the applet's **Settings** tab and set a network name and
a password before turning the hotspot on. Use a long random password: anyone
who joins your hotspot is on the same network as your computer and can reach
any service listening on all interfaces.

### With an AI coding agent

This repo ships an [`AGENTS.md`](AGENTS.md) with the exact build and install
steps, already followed automatically by agents that support that
convention (Codex CLI, Cursor, Aider, OpenCode, and others). For any other
assistant with terminal access — Claude Code included — paste this:

```text
Clone https://github.com/Samuel-S-N/cosmic-applet-hotspot, read its
AGENTS.md, and follow it to build and install the applet on this machine.
```

## Development

```bash
cargo build
cargo test    # 36 tests
just check    # clippy, pedantic lints
```

## License

[MPL-2.0](https://www.mozilla.org/en-US/MPL/2.0/)
