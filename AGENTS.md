# AGENTS.md

Instructions for AI coding agents (Claude Code, Cursor, Codex CLI, Gemini CLI,
OpenCode, or any assistant with a shell) building and installing this project.

## What this is

A COSMIC desktop panel applet, written in Rust with [libcosmic], that lets the
user turn a Wi-Fi hotspot on and off from the panel, backed directly by
NetworkManager over D-Bus (no `nmcli` shelling out). It only runs on Linux,
under the COSMIC desktop (Pop!_OS, or any distro with `cosmic-panel`
installed).

[libcosmic]: https://github.com/pop-os/libcosmic

## Setup

Run these in order. Each one is safe to re-run.

1. **System libraries** (`apt`-based distros — Pop!_OS, Ubuntu, Debian). Ask
   the user before running this: it needs `sudo` and installs packages.

   ```bash
   sudo apt install -y build-essential pkg-config libxkbcommon-dev libinput-dev \
     libudev-dev libfontconfig-dev libwayland-dev libssl-dev just cmake iw
   ```

   On a non-`apt` distro, install the equivalent packages with the system's
   package manager and tell the user which ones you substituted.

2. **Rust**, if `cargo` is not already on `PATH`:

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
   source "$HOME/.cargo/env"
   ```

3. **Clone**, if you are not already inside a checkout of this repo:

   ```bash
   git clone https://github.com/Samuel-S-N/cosmic-applet-hotspot.git
   cd cosmic-applet-hotspot
   ```

## Build and test

```bash
cargo build
cargo test
```

The first build compiles `libcosmic` from source and takes several minutes.
All tests must pass (36 at the time of writing) before you install anything.

## Install

```bash
just install-user
```

This builds a release binary and installs it under `~/.local` — no `sudo`
needed. It does **not** touch the running panel.

## Finish (tell the user to do this — you cannot do it for them)

1. Open **Settings → Desktop → Panel → Configure panel applets** and add
   **Hotspot** to the panel.
2. If it doesn't show up, run `pkill cosmic-panel` (the panel restarts
   itself) and check again.
3. Open the applet's **Settings** tab and choose a network name and a long
   password. There are no defaults — don't invent credentials for the user.

## Rules for you, the agent

- **Never activate the hotspot** (`nm::up`, clicking the panel toggle, or
  `nmcli connection up id "COSMIC Hotspot"`) on a machine with only one Wi-Fi radio and no
  wired uplink — it will drop the user's network, including your own shell
  session if it's remote. Ask first if you're unsure whether a wired
  connection is present.
- **Never run `sudo` without telling the user first** and getting a clear go
  ahead — step 1 above is the only place this repo needs it.
- `cargo`/`just` are not always on `PATH` in a fresh shell after installing
  Rust: `export PATH="$HOME/.cargo/bin:$PATH"` if a command isn't found.
- Follow TDD for any code change: write the failing test first, watch it
  fail, then implement.
- Commit messages here end with a `Co-Authored-By:` trailer identifying the
  model that made the change (see recent `git log` for the exact format);
  keep doing that for consistency, adjusted to name yourself.
