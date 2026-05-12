# FlightGear-StreamDeck

This repository contains the profiles, helper scripts, and command-line tooling I use to control FlightGear from illuminated hardware buttons instead of relying on a keyboard in a dark room.

My daughter and I have been flying the Cessna 172P Skyhawk in FlightGear using yoke and pedal controls with a projector. That setup is great for immersion, but it makes the keyboard awkward to use. In a dark room, the keys are hard to see, and even when you can find them, you still have to remember which specific keyboard shortcut controls a given simulator function. The goal of this project is to replace that experience with clearly labeled, backlit buttons and a simple menu structure that is easy to navigate while flying.

## Project History

This project started on Windows with an Elgato Stream Deck. The first version used an Elgato profile built in the Stream Deck software and depended on the API Request plugin. Each button sent an HTTP request to FlightGear's built-in HTTP server.

That worked well for direct simulator properties, but some actions needed more than a simple property write. To handle those cases, I wrote [Nasal/http-control.nas](Nasal/http-control.nas), a Nasal script that adds custom triggerable properties for more complex behavior such as autostart.

While developing that first version, I added some VS Code tasks with hard-coded local copy paths so I could quickly move the profile assets and Nasal script into my own Windows and Linux installations. Those tasks are still useful for my setup, but anyone else using this repository should expect to modify the paths for their own machines.

Later, while moving the simulator from Windows to Linux, I continued using the Elgato profile on a Windows machine to control FlightGear running on Linux. That kept the project usable while I rebuilt the profile for a Linux-friendly workflow.

Because Elgato does not provide its Stream Deck software for Linux, I eventually switched to Open Deck. The Open Deck profiles in this repository are the result of that transition. They do not require plugins, and Open Deck can be used on Linux, macOS, and Windows.

One design change during that migration was especially important. The original Elgato profile used multi-action key presses for some controls so that pressing a button twice quickly would make larger adjustments to frequencies, headings, and similar values. That approach did not work especially well in practice. In the Open Deck version, I replaced it with a keyboard-repeat style interaction: when a button is held down, the command fires once, pauses briefly, and then repeats rapidly until the button is released. That feels much closer to how cockpit controls should behave.

To support that behavior, I wrote [fgcmd/src/main.rs](fgcmd/src/main.rs), a small Rust command-line tool named `fgcmd`. It reads commands from [fgcmd/commands.csv](fgcmd/commands.csv), supports key-down and key-up actions, and uses a lock file to coordinate the repeat behavior. This also cleanly separates the HTTP URLs and request bodies from profile design, which makes the Open Deck configuration easier to maintain.

## What Is In This Repository

- [Nasal/http-control.nas](Nasal/http-control.nas): FlightGear Nasal script that exposes custom triggerable properties and simulator actions.
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
6. On the FlightGear side, [Nasal/http-control.nas](Nasal/http-control.nas) handles the incoming requests and applies the requested simulator changes.

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

- FlightGear with its built-in HTTP server enabled.
- Access to your FlightGear Nasal directory so [Nasal/http-control.nas](Nasal/http-control.nas) can be installed.
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

## Installing The Nasal Script

Copy [Nasal/http-control.nas](Nasal/http-control.nas) into your FlightGear Nasal directory.

This repository includes a VS Code task named `Windows - Copy http-control.nas to FlightGear Nasal directory` that I used during development on Windows. The path is hard coded for my machine, so you will need to update it for your own installation before using it.

## VS Code Copy Tasks

The VS Code project includes tasks for copying profiles and images between this repository and local application directories on Windows, Linux, and macOS.

- Windows task for copying the Elgato profile into the repository.
- Windows task for copying `http-control.nas` into my local FlightGear installation.
- Linux and macOS tasks for syncing Open Deck profiles and images.

These tasks are configured with machine-specific paths from my development environment. Review and modify them before running them on your own system.

## Repository Layout

- [fgcmd](fgcmd): Rust command dispatcher and CSV command catalog.
- [Nasal](Nasal): FlightGear Nasal support script.
- [Elgato/StreamDeck](Elgato/StreamDeck): Original Stream Deck profiles and icon assets.
- [opendeck](opendeck): Current Open Deck profiles and images.

## Future Improvements

- Change certain button images or labels according to simulator state, such as pause versus play and parking brake on versus off.
- Expand the profiles and command sets for additional aircraft, including the Cessna 172S and Cessna 182 Skylane.

## Notes

- `fgcmd` embeds [fgcmd/commands.csv](fgcmd/commands.csv) at compile time, so rebuild after changing that file.
- If you are setting this up on a different machine, expect to adjust copy paths, application locations, and possibly FlightGear HTTP server configuration.

## License

This repository is licensed under the MIT License. See [LICENSE](LICENSE).
