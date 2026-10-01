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

Originally, the Nasal script had to be copied into FlightGear's own `$FG_ROOT/Nasal` directory, which meant modifying the FlightGear installation and redoing the copy after every upgrade. It is now packaged as a FlightGear add-on, which FlightGear loads directly from this repository with `--addon`. The [fglaunch](launch/fglaunch) launcher passes that option along with the HTTP server setting, so the whole setup is recorded in one place.

## What Is In This Repository

- [addon](addon): FlightGear add-on. [addon/addon-main.nas](addon/addon-main.nas) exposes custom triggerable properties and simulator actions.
- [launch/fglaunch](launch/fglaunch): Launcher for side-by-side FlightGear installs, with the HTTP server and the add-on enabled.
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
- Rust, if you want to build `fgcmd` yourself.
- Either Open Deck or the Elgato Stream Deck software, depending on which profile set you want to use.

## Building fgcmd

From the [fgcmd](fgcmd) directory:

```bash
cargo build --release
```

The resulting binary will be at `fgcmd/target/release/fgcmd`.

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

[launch/fglaunch](launch/fglaunch) runs several FlightGear versions side by side, each with matching data, and starts any of them with a named configuration:

```bash
fglaunch 2024.1.7            # desktop configuration (the default)
fglaunch 2024.1.5 vr
fglaunch 2024.1.7 desktop --airport=KSFO   # extra fgfs options pass through
```

Run it with no arguments to list the available installs and configurations.

Each install is a folder under `$FG_BASE/installs/` (default `/mnt/nocow/doberlin/flightgear`, a btrfs subvolume left out of snapshots so the large data isn't snapshotted):

```text
installs/<version>/
  fgfs       symlink to the FlightGear AppImage to run
  fgdata/    the FGData release matching that AppImage
  home/      FG_HOME for this install (created on first launch)
  aircraft/  optional extra aircraft not in that version's catalog
shared/TerraSync/   scenery, shared by every install
```

For each launch, `fglaunch`:

- sets `FG_HOME` and `--fg-root` for the chosen install, so versions never share settings or data;
- points `--terrasync-dir` at the shared scenery, which works with every version (it's selected by scenery service, not FlightGear version);
- adds `--httpd=8080` and `--addon` for this repository's add-on. Setting these here, rather than in the launcher's *Additional Settings* box, keeps them in effect for every version, because the launcher saves its settings separately for each FlightGear version;
- adds `--fg-aircraft` for the install's `aircraft/` folder, if there is one;
- applies the configuration: `desktop` or `vr` (which sets `XR_RUNTIME_JSON` for my WiVRn install; change it for your own VR runtime).

To add a version, create its folder, extract that release's data package into `fgdata/` (for example `FlightGear-2024.1.7-data.txz` from the FlightGear download mirror), and link `fgfs` to its AppImage.

`fglaunch` finds the repository relative to its own location, so it can be symlinked onto your `PATH`:

```bash
ln -s ~/Development/FlightGear-StreamDeck/launch/fglaunch ~/flightgear/fglaunch
```

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
- [launch](launch): `fglaunch`, the side-by-side FlightGear launcher.
- [config](config): Shared FlightGear configuration (joystick bindings).
- [Elgato/StreamDeck](Elgato/StreamDeck): Original Stream Deck profiles and icon assets.
- [opendeck](opendeck): Current Open Deck profiles and images.

## Future Improvements

- Change certain button images or labels according to simulator state, such as pause versus play and parking brake on versus off.
- Expand the profiles and command sets for additional aircraft, including the Cessna 172S and Cessna 182 Skylane.

## Notes

- `fgcmd` embeds [fgcmd/commands.csv](fgcmd/commands.csv) at compile time, so rebuild after changing that file.
- If you are setting this up on a different machine, expect to adjust copy paths and application locations in the VS Code tasks and in `fglaunch` (`FG_BASE` and the VR runtime path).

## License

This repository is licensed under the MIT License. See [LICENSE](LICENSE).
