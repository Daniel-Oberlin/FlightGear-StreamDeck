# FlightGear-StreamDeck

This repository contains the profiles, helper scripts, and command-line tooling I use to control FlightGear from illuminated hardware buttons instead of relying on a keyboard in a dark room.

My daughter and I have been flying the Cessna 172P Skyhawk in FlightGear using yoke and pedal controls with a projector. That setup is great for immersion, but it makes the keyboard awkward to use. In a dark room, the keys are hard to see, and even when you can find them, you still have to remember which specific keyboard shortcut controls a given simulator function. The goal of this project is to replace that experience with clearly labeled, backlit buttons and a simple menu structure that is easy to navigate while flying.

## Project History

This project started on Windows with an Elgato Stream Deck. The first version used an Elgato profile built in the Stream Deck software and depended on the API Request plugin. Each button sent an HTTP request to FlightGear's built-in HTTP server.

That worked well for direct simulator properties, but some actions needed more than a simple property write. To handle those cases, I wrote a Nasal script that adds custom triggerable properties for more complex behavior such as autostart. It is now packaged as a FlightGear add-on in [addon](addon).

While developing that first version, I added some VS Code tasks with hard-coded local copy paths so I could quickly move the profile assets and Nasal script into my own Windows and Linux installations. Those tasks are still useful for my setup, but anyone else using this repository should expect to modify the paths for their own machines.

Later, while moving the simulator from Windows to Linux, I continued using the Elgato profile on a Windows machine to control FlightGear running on Linux. That kept the project usable while I rebuilt the profile for a Linux-friendly workflow.

Because Elgato does not provide its Stream Deck software for Linux, I eventually switched to Open Deck. The Open Deck profiles in this repository are the result of that transition. They do not require plugins, and Open Deck can be used on Linux, macOS, and Windows.

One design change during that migration was especially important. The original Elgato profile used multi-action key presses for some controls so that pressing a button twice quickly would make larger adjustments to frequencies, headings, and similar values. That approach did not work especially well in practice. In the Open Deck version, I replaced it with a keyboard-repeat style interaction: when a button is held down, the command fires once, pauses briefly, and then repeats rapidly until the button is released. That feels much closer to how cockpit controls should behave.

To support that behavior, I wrote [fgcmd/src/main.rs](fgcmd/src/main.rs), a small Rust command-line tool named `fgcmd`. It reads commands from [fgcmd/commands.csv](fgcmd/commands.csv), supports key-down and key-up actions, and uses a lock file to coordinate the repeat behavior. This also cleanly separates the HTTP URLs and request bodies from profile design, which makes the Open Deck configuration easier to maintain.

Originally, the Nasal script had to be copied into FlightGear's own `$FG_ROOT/Nasal` directory, which meant modifying the FlightGear installation and redoing the copy after every upgrade. It is now packaged as a FlightGear add-on, which FlightGear loads directly from this repository with `--addon`. The [fglaunch](fglaunch) launcher passes that option along with the HTTP server setting, so the whole setup is recorded in one place.

## What Is In This Repository

- [addon](addon): FlightGear add-on. [addon/addon-main.nas](addon/addon-main.nas) exposes custom triggerable properties and simulator actions.
- [fglaunch](fglaunch): Terminal menu for launching side-by-side FlightGear installs, with the HTTP server, the add-on and the Stream Deck handled automatically.
- [config/Input/Joysticks](config/Input/Joysticks): Yoke and pedal bindings shared by all installs.
- [fgcmd/commands.csv](fgcmd/commands.csv): CSV command catalog mapping command names to HTTP URL paths and optional request bodies.
- [fgcmd/src/main.rs](fgcmd/src/main.rs): Rust implementation of `fgcmd`, including command lookup, HTTP dispatch, and hold-to-repeat behavior.
- [Elgato/StreamDeck](Elgato/StreamDeck): Stream Deck profile assets from the original Windows-based implementation.
- [opendeck](opendeck): Open Deck profiles and images for the current cross-platform setup.

## How The Current Open Deck Setup Works

1. A button in Open Deck runs `fgcmd down COMMAND` when pressed.
2. `fgcmd` looks up `COMMAND` in [fgcmd/commands.csv](fgcmd/commands.csv).
3. It sends the corresponding HTTP request to FlightGear.
4. If the button is held, `fgcmd` uses a lock file to continue repeating the command after a short initial delay.
5. When the button is released, Open Deck runs `fgcmd up COMMAND`, which removes the lock file and stops the repeat loop.
6. On the FlightGear side, the add-on ([addon/addon-main.nas](addon/addon-main.nas)) handles the incoming requests and applies the requested simulator changes.

This arrangement keeps the profile UI relatively simple. The Open Deck profile mainly refers to symbolic command names, while the actual HTTP details stay centralized in [fgcmd/commands.csv](fgcmd/commands.csv).

## Elgato Version

The Elgato profile in [Elgato/StreamDeck](Elgato/StreamDeck) reflects the original Windows implementation.

- It was built with the Elgato Stream Deck software.
- It requires the API Request plugin.
- It sends HTTP requests directly to FlightGear.
- Some actions used multi-action button logic to simulate faster adjustment when buttons were pressed repeatedly.

That version is still included for reference and for anyone who wants to use the original Stream Deck workflow on Windows.

## Open Deck Version

The Open Deck assets in [opendeck](opendeck) are the newer cross-platform implementation.

- No plugins are required.
- Open Deck can be used on Linux, macOS, and Windows.
- Repeating controls use `fgcmd` with explicit key-down and key-up handling.
- Command definitions are kept outside the profile UI in [fgcmd/commands.csv](fgcmd/commands.csv).

## Requirements

- FlightGear 2020.3 or later (developed and tested on 2024.1), started with its built-in HTTP server enabled (`--httpd=8080`).
- The add-on in [addon](addon) loaded into FlightGear. See [Installing The FlightGear Add-on](#installing-the-flightgear-add-on).
- Rust, to build `fgcmd` and `fglaunch`.
- Either Open Deck or the Elgato Stream Deck software, depending on which profile set you want to use.

## Building And Installing

[install.sh](install.sh) builds `fglaunch` and `fgcmd` in release mode and copies them into `~/flightgear` (or a directory you pass as the first argument), which should be on your `PATH`:

```bash
./install.sh
```

The binaries are copied rather than linked, so cleaning the build directories never breaks the Open Deck buttons. Rerun it after changing either program or [fgcmd/commands.csv](fgcmd/commands.csv).

## fgcmd Usage

```text
fgcmd [--pretend] <down|up> <COMMAND>
```

- `down` sends the initial command and, when applicable, starts repeat behavior.
- `up` stops repeat behavior for that command.
- `--pretend` prints the resolved request instead of sending it.

Examples:

```bash
fgcmd down DME_NAV1
fgcmd up DME_NAV1
fgcmd --pretend down NAV1_FRQ_.1
```

## fgcmd Configuration

`fgcmd` uses [fgcmd/commands.csv](fgcmd/commands.csv) as a data-driven command table. The base URL defaults to `http://localhost:8080`, but it can be overridden with `FGCMD_BASE_URL`.

Other environment variables are available to tune the repeat behavior:

- `FGCMD_LOCK_DIR`
- `FGCMD_INITIAL_DELAY_MS`
- `FGCMD_REPEAT_DELAY_MS`
- `FGCMD_MAX_DURATION_MS`

Example:

```bash
FGCMD_BASE_URL=http://192.168.1.10:8080 \
FGCMD_INITIAL_DELAY_MS=300 \
FGCMD_REPEAT_DELAY_MS=40 \
FGCMD_MAX_DURATION_MS=7000 \
fgcmd down NAV1_FRQ_.1
```

## Installing The FlightGear Add-on

The [addon](addon) directory is a standard FlightGear add-on. Nothing needs to be copied into the FlightGear installation. Load it in one of these ways:

- **Launcher:** open *Add-ons*, click `+` under *Add-on Module folders*, and select this repository's `addon` directory.
- **Command line:** pass `--addon=/path/to/FlightGear-StreamDeck/addon` to `fgfs`.

FlightGear must also run its HTTP server on the port `fgcmd` uses (8080 by default), so pass `--httpd=8080` too.

To confirm that the add-on loaded, look in FlightGear's log (`~/.fgfs/fgfs.log` on Linux) for:

```text
[OK] 'FlightGear Stream Deck' (V. 1.0.0) loaded.
```

If you previously copied `http-control.nas` into `$FG_ROOT/Nasal`, delete that copy. Otherwise every command is handled twice: adjustments double and toggles cancel themselves out.

## Launching FlightGear: Side-by-Side Installs

`fglaunch` is a terminal menu for several FlightGear versions installed side by side, each with matching data. Run it with no arguments:

```text
 FlightGear                                              VR ○ OFF
 installed ──────────────────────────────────────────────────────
> 2024.1.7        stable   last flown
  2024.1.5        stable
  -- available
  next-20260930   nightly  install  0.4 GB + fgdata  experimental

 checked 22:50
 Stream Deck: starts with FlightGear
 ↑↓ select   ⏎ launch   v VR on/off   r check now   q quit
```

- **↑/↓** select an install; the one you flew last is selected when the menu opens.
- **Enter** on an *available* version installs it, with a progress bar in the status line. The menu stays usable meanwhile, including launching; quitting asks first, then cancels and cleans up.
- **d** removes the selected install after confirming. Shared scenery, joystick bindings, fgdata and aircraft are never removed.
- **r** checks for new versions now (see below).
- **v** toggles VR. The badge in the corner shows the mode, and the Enter hint changes to *launch in VR*.
- **Enter** launches the selected install. FlightGear's own launcher window still opens for choosing the aircraft and airport.
- While FlightGear runs, the menu shows how long you've been flying. **s** stops FlightGear after confirming. When FlightGear exits, the menu comes back and shows how it exited.

For each launch, `fglaunch`:

- starts Open Deck hidden in the tray (`--hide`) if it isn't already running, opening on the FlightGear page (`FlightGear/Main` by default), and stops it again when FlightGear exits. An Open Deck that was already running is left alone, on whatever page it shows. The page is set only at startup, so switching virtual desktops or apps never moves the Stream Deck off the page you're on;
- sets `FG_HOME` and `--fg-root` for the chosen install, so versions never share settings or data;
- points `--terrasync-dir` at the shared scenery, which works with every version (it's selected by scenery service, not FlightGear version);
- adds `--httpd=8080` and `--addon` for this repository's add-on. Setting these here, rather than in the launcher's *Additional Settings* box, keeps them in effect for every version, because the launcher saves its settings separately for each FlightGear version;
- adds `--fg-aircraft` for the install's `aircraft/` folder, if there is one;
- in VR mode, adds `--enable-vr` and sets `XR_RUNTIME_JSON`;
- sends FlightGear's console output to `home/launch.log` so it can't draw over the menu, and warns if `home/fgfs.log` grows past 1 GB, which usually means FlightGear is stuck in a crash loop.

### New Versions

While the menu is open, `fglaunch` checks in the background for versions you don't have, and lists them under *available*:

- **Stable releases** from the FlightGear download mirror: those newer than your newest installed stable release, or just the newest release if none is installed.
- **Nightly builds** of FlightGear's development branch (`next`) from GitLab CI: the latest nightly, if it is newer than any nightly you have. GitLab keeps nightly builds for 30 days.

Installing builds the version in its final `installs/` folder and creates the `fgfs` link last, so a failed or cancelled install never appears in the menu; its folder is removed instead.

- **Stable:** downloads the AppImage and the data package (checking each against the size the server reports), extracts the data, checks its version file, and deletes the package.
- **Nightly:** downloads the AppImage. FGData comes from one shared, shallow clone of FGData's `next` branch (`shared/fgdata.git`); each nightly gets a git worktree of it at the last commit before its build started, because the nightly build uses the tip of `next` at that time. The C172P, which isn't part of `next`, is one shared clone in `shared/aircraft/c172p`, linked as the install's `aircraft/` folder and updated with each nightly installed.
- **Home folder:** launcher settings are copied from your most recent install of the same release series, without paths into that install, and saved aircraft state (`aircraft-data/`) from the install you flew last.

Results are cached in `~/.cache/fglaunch/updates.json` and refreshed when they are more than 6 hours old, or when you press **r**. If the servers can't be reached, the last results stay listed.

Only one FlightGear can run at a time, because they would share the HTTP port and the scenery folder; `fglaunch` refuses to start a second one.

### Install Layout

Each install is a folder under `installs/` in the FlightGear base directory (default `/mnt/nocow/doberlin/flightgear`, a btrfs subvolume left out of snapshots so the large data isn't snapshotted):

```text
installs/<version>/
  fgfs       symlink to the FlightGear AppImage to run
  fgdata/    the FGData release matching that AppImage
  home/      FG_HOME for this install (created on first launch)
  aircraft/  optional extra aircraft not in that version's catalog
shared/TerraSync/   scenery, shared by every install
shared/fgdata.git/  FGData repository for nightlies (one worktree per nightly)
shared/aircraft/    aircraft shared by nightlies (the C172P)
```

Versions are normally installed from the menu. To add one by hand, create its folder, extract that release's data package into `fgdata/` (for example `FlightGear-2024.1.7-data.txz` from the FlightGear download mirror), and link `fgfs` to its AppImage. Folders whose name starts with `next` are listed as nightlies.

### Configuration

Settings are read from `~/.config/fglaunch/config.toml`. Every setting is optional:

```toml
fg_base = "/mnt/nocow/doberlin/flightgear"   # holds installs/ and shared/
repo = "/path/to/FlightGear-StreamDeck"     # default: the checkout fglaunch was built from
opendeck = "/path/to/opendeck.AppImage"     # default: newest opendeck_*.AppImage in ~/flightgear
opendeck_profile = "FlightGear/Main"        # Open Deck page to start on
opendeck_config = "~/.config/opendeck"      # Open Deck's settings folder (write the full path)
vr_runtime_json = "/path/to/openxr_runtime.json"   # default: my WiVRn flatpak runtime
httpd_port = 8080
log_limit_mb = 1024
```

The last install flown and the VR setting are remembered in `~/.local/state/fglaunch/state.json`.

### Shared Joystick Configuration

My yoke and pedal bindings are in [config/Input/Joysticks](config/Input/Joysticks). On first launch, `fglaunch` links each install's `home/Input/Joysticks` to that folder, so every version uses the same calibration. Saving from FlightGear's Joystick Configuration dialog writes through the link into the repository, where the change can be reviewed and committed.

## VS Code Copy Tasks

The VS Code project includes tasks for copying profiles and images between this repository and local application directories on Windows, Linux, and macOS.

- Windows task for copying the Elgato profile into the repository.
- Linux and macOS tasks for syncing Open Deck profiles and images.

These tasks are configured with machine-specific paths from my development environment. Review and modify them before running them on your own system.

## Repository Layout

- [fgcmd](fgcmd): Rust command dispatcher and CSV command catalog.
- [addon](addon): FlightGear add-on with the Nasal support code.
- [fglaunch](fglaunch): Terminal menu for side-by-side FlightGear installs.
- [install.sh](install.sh): Builds and installs `fglaunch` and `fgcmd`.
- [config](config): Shared FlightGear configuration (joystick bindings).
- [Elgato/StreamDeck](Elgato/StreamDeck): Original Stream Deck profiles and icon assets.
- [opendeck](opendeck): Current Open Deck profiles and images.

## Future Improvements

- Change certain button images or labels according to simulator state, such as pause versus play and parking brake on versus off.
- Expand the profiles and command sets for additional aircraft, including the Cessna 172S and Cessna 182 Skylane.

## Notes

- `fgcmd` embeds [fgcmd/commands.csv](fgcmd/commands.csv) at compile time, so rebuild after changing that file.
- If you are setting this up on a different machine, expect to adjust copy paths and application locations in the VS Code tasks and in `~/.config/fglaunch/config.toml`.

## License

This repository is licensed under the MIT License. See [LICENSE](LICENSE).
