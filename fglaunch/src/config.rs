use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The repository this binary was built from; holds the add-on and joystick bindings.
const BUILD_REPO: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/..");

/// Settings from ~/.config/fglaunch/config.toml. Every field is optional there.
#[derive(Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Holds installs/<name>/ and shared/TerraSync/.
    pub fg_base: PathBuf,
    /// Repository checkout with addon/ and config/Input/Joysticks/.
    pub repo: PathBuf,
    /// OpenDeck AppImage; defaults to the newest opendeck_*.AppImage in ~/flightgear.
    pub opendeck: Option<PathBuf>,
    /// OpenDeck profile to show when OpenDeck is started for a flight, e.g. "FlightGear/Main".
    pub opendeck_profile: Option<String>,
    /// OpenDeck's settings folder, holding profiles/<device>.json.
    pub opendeck_config: PathBuf,
    /// OpenXR runtime manifest exported as XR_RUNTIME_JSON in VR mode.
    pub vr_runtime_json: Option<PathBuf>,
    pub httpd_port: u16,
    /// Warn when fgfs.log grows past this many MB (crash loops can fill the disk).
    pub log_limit_mb: u64,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            fg_base: PathBuf::from("/mnt/nocow/doberlin/flightgear"),
            repo: PathBuf::from(BUILD_REPO),
            opendeck: None,
            opendeck_profile: Some("FlightGear/Main".into()),
            opendeck_config: xdg_dir("XDG_CONFIG_HOME", ".config").join("opendeck"),
            vr_runtime_json: Some(PathBuf::from(
                "/var/lib/flatpak/app/io.github.wivrn.wivrn/current/active/files/share/openxr/1/openxr_wivrn.json",
            )),
            httpd_port: 8080,
            log_limit_mb: 1024,
        }
    }
}

impl Config {
    pub fn load() -> Result<Config, String> {
        let path = config_dir().join("config.toml");
        let mut cfg: Config = match fs::read_to_string(&path) {
            Ok(text) => {
                toml::from_str(&text).map_err(|err| format!("{}: {err}", path.display()))?
            }
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Config::default(),
            Err(err) => return Err(format!("{}: {err}", path.display())),
        };
        cfg.repo = cfg.repo.canonicalize().unwrap_or(cfg.repo);
        if cfg.opendeck.is_none() {
            cfg.opendeck = newest_opendeck(&home_dir().join("flightgear"));
        }
        Ok(cfg)
    }

    pub fn installs_dir(&self) -> PathBuf {
        self.fg_base.join("installs")
    }

    pub fn terrasync_dir(&self) -> PathBuf {
        self.fg_base.join("shared").join("TerraSync")
    }

    pub fn addon_dir(&self) -> PathBuf {
        self.repo.join("addon")
    }

    pub fn joysticks_dir(&self) -> PathBuf {
        self.repo.join("config").join("Input").join("Joysticks")
    }
}

/// What the menu remembers between runs.
#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct State {
    pub last_flown: Option<String>,
    pub vr: bool,
}

impl State {
    pub fn load() -> State {
        fs::read_to_string(state_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = state_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }
}

fn home_dir() -> PathBuf {
    env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg_dir(var: &str, fallback: &str) -> PathBuf {
    env::var_os(var)
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home_dir().join(fallback))
}

fn config_dir() -> PathBuf {
    xdg_dir("XDG_CONFIG_HOME", ".config").join("fglaunch")
}

fn state_path() -> PathBuf {
    xdg_dir("XDG_STATE_HOME", ".local/state")
        .join("fglaunch")
        .join("state.json")
}

/// opendeck_2.12.0_amd64.AppImage sorts after opendeck_2.11.0_amd64.AppImage.
fn newest_opendeck(dir: &Path) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = fs::read_dir(dir)
        .ok()?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("opendeck_") && n.ends_with(".AppImage"))
        })
        .collect();
    found.sort_by_key(|p| crate::installs::version_key(&p.to_string_lossy()));
    found.pop()
}
