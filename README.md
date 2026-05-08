# FlightGear-StreamDeck

This repository contains a control-stack for FlightGear using Stream Deck / OpenDeck profiles and a small Rust command dispatcher (`fgcmd`) that translates button actions into HTTP requests consumed by a FlightGear Nasal script.

## What This Project Includes

- **FlightGear remote-control Nasal script** for custom property-based actions.
- **Command catalog** (`fgcmd/commands.csv`) mapping symbolic command names to URL path + optional JSON body.
- **Rust CLI (`fgcmd`)** that executes commands and supports hold-to-repeat behavior.
- **OpenDeck and Elgato profile assets** for practical cockpit layouts and key bindings.
- **VS Code tasks** to sync profile assets between this repo and local platform profile directories.

## Repository Layout

- `fgcmd/`
  - `src/main.rs`: command parsing, lock-file repeat behavior, HTTP POST logic.
  - `commands.csv`: command name -> URL -> JSON body map.
- `Nasal/http-control.nas`
  - Listener endpoints under `/sim/remote/c172/...` used by `commands.csv` URLs.
- `opendeck/`
  - OpenDeck profiles/images that invoke `fgcmd` with `down`/`up` actions.
- `Elgato/StreamDeck/`
  - Stream Deck profile and icon assets.
- `.vscode/tasks.json`
  - Copy/sync tasks for macOS, Linux, and Windows.

## How It Works

1. A button profile action runs `fgcmd <action> <COMMAND>` where action is `down` or `up`.
2. `fgcmd` resolves `COMMAND` in `fgcmd/commands.csv`.
3. For `down`:
   - lock file is created/overwritten in a temp lock directory,
   - one HTTP POST is sent,
   - waits initial delay,
   - repeats POST while lock file exists and until max duration is reached.
4. For `up`:
   - lock file is deleted (missing lock is ignored),
   - program exits immediately.
5. In FlightGear, `Nasal/http-control.nas` listeners consume those updates and apply sim changes.

## Requirements

- FlightGear with access to the Nasal data directory.
- Rust toolchain (for building `fgcmd`).
- HTTP endpoint available at FlightGear side (default base URL is `http://localhost:8080`).
- Optional:
  - OpenDeck,
  - Elgato Stream Deck software,
  - VS Code tasks integration.

## Build fgcmd

From `fgcmd/`:

```bash
cargo build --release
```

Binary path:

- `fgcmd/target/release/fgcmd`

## fgcmd CLI Usage

```text
fgcmd [--pretend] <down|up> <COMMAND>
```

- `down` / `up` are case-insensitive.
- `--pretend` prints the final POST URL/body instead of making HTTP calls.

Examples:

```bash
# Pressed event (starts repeat flow)
fgcmd down DME_NAV1

# Released event (stops repeat flow)
fgcmd up DME_NAV1

# Dry-run for debugging
fgcmd --pretend down DME_NAV1
```

## fgcmd Environment Variables

### URL override

- `FGCMD_BASE_URL`
  - Overrides request base URL (default: `http://localhost:8080`).

### Lock and timing overrides

- `FGCMD_LOCK_DIR`
  - Directory for command lock files.
- `FGCMD_INITIAL_DELAY_MS`
  - Delay after first POST before repeat loop (default: `500`).
- `FGCMD_REPEAT_DELAY_MS`
  - Delay between repeated POST calls (default: `30`).
- `FGCMD_MAX_DURATION_MS`
  - Maximum repeat-loop runtime (default: `5000`).

Example:

```bash
FGCMD_BASE_URL=http://192.168.1.10:8080 \
FGCMD_INITIAL_DELAY_MS=300 \
FGCMD_REPEAT_DELAY_MS=40 \
FGCMD_MAX_DURATION_MS=7000 \
fgcmd down NAV1_FRQ_.1
```

## FlightGear Nasal Setup

Copy this repository file into your FlightGear Nasal directory:

- `Nasal/http-control.nas`

A VS Code task is provided for Windows:

- `Windows - Copy http-control.nas to FlightGear Nasal directory`

Adjust task paths if your FlightGear installation differs.

## Profile Sync Workflows

VS Code tasks exist for syncing profile/image data:

- macOS:
  - `Mac - Copy OpenDeck profiles and images to project directory`
  - `Mac - Copy OpenDeck profiles and images to Application Support`
- Linux:
  - `Linux - Copy OpenDeck profiles and images to project directory`
  - `Linux - Copy OpenDeck profiles and images to profile directory`
- Windows:
  - `Windows - Copy StreamDeck ProfilesV3 to project directory`

These use `rsync`/`robocopy` with delete/mirror semantics, so review paths before running.

## Development Notes

- `fgcmd` command definitions are embedded at compile time via `include_str!("../commands.csv")`.
  - Rebuild after changing `commands.csv`.
- `--pretend` is useful for validating command names, URL composition, and request bodies without a running server.

## Troubleshooting

- **`usage: fgcmd [--pretend] <down|up> <COMMAND>`**
  - Required args were missing or malformed.
- **`unknown command: ...`**
  - Command is not present in `fgcmd/commands.csv`.
- **HTTP failures**
  - Verify FlightGear endpoint is running and `FGCMD_BASE_URL` is correct.
- **No effect in sim despite successful POST**
  - Confirm `http-control.nas` is installed and loaded by FlightGear.

## License

No license file is currently included in this repository.
