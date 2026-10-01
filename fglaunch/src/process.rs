use std::fs::{self, File};
use std::os::unix::fs::symlink;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;
use crate::installs::Install;

/// A running FlightGear session started from the menu.
pub struct Flight {
    pub child: Child,
    pub install: String,
    pub vr: bool,
    pub started: Instant,
    /// FlightGear's own log, watched for runaway growth.
    pub log: PathBuf,
    /// Where FlightGear's stdout/stderr go, so they don't draw over the menu.
    pub console_log: PathBuf,
    /// Set only when this session started OpenDeck, so it is stopped afterwards.
    pub opendeck: Option<Child>,
    /// Stopped from the menu, so a signal exit is expected rather than a crash.
    pub stop_requested: bool,
}

pub fn start_flightgear(cfg: &Config, install: &Install, vr: bool) -> Result<Child, String> {
    let home = install.home();
    fs::create_dir_all(home.join("Input")).map_err(|err| format!("{}: {err}", home.display()))?;
    link_joysticks(cfg, &home)?;
    fs::create_dir_all(cfg.terrasync_dir())
        .map_err(|err| format!("{}: {err}", cfg.terrasync_dir().display()))?;

    let console_log = home.join("launch.log");
    let out =
        File::create(&console_log).map_err(|err| format!("{}: {err}", console_log.display()))?;
    let err_out = out.try_clone().map_err(|err| err.to_string())?;

    let mut cmd = Command::new(install.fgfs());
    cmd.arg("--launcher")
        .arg(format!("--fg-root={}", install.fgdata().display()))
        .arg(format!("--terrasync-dir={}", cfg.terrasync_dir().display()))
        .arg(format!("--httpd={}", cfg.httpd_port))
        .arg(format!("--addon={}", cfg.addon_dir().display()));
    if install.aircraft().is_dir() {
        cmd.arg(format!("--fg-aircraft={}", install.aircraft().display()));
    }
    if vr {
        cmd.args(["--enable-vr", "--prop:/sim/current-view/y-offset-m=0.2"]);
        if let Some(runtime) = &cfg.vr_runtime_json {
            cmd.env("XR_RUNTIME_JSON", runtime);
        }
    } else {
        cmd.args(["--disable-vr", "--prop:/sim/current-view/y-offset-m=0.0"]);
    }
    cmd.env("FG_HOME", &home)
        .current_dir(&install.dir)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err_out)
        // Own process group, so stopping it also stops the AppImage's child processes.
        .process_group(0);
    cmd.spawn()
        .map_err(|err| format!("could not start {}: {err}", install.fgfs().display()))
}

/// Share the repo's joystick bindings with every install. A real directory is left alone.
fn link_joysticks(cfg: &Config, home: &Path) -> Result<(), String> {
    let link = home.join("Input").join("Joysticks");
    if link.symlink_metadata().is_err() {
        symlink(cfg.joysticks_dir(), &link).map_err(|err| format!("{}: {err}", link.display()))?;
    }
    Ok(())
}

/// Make OpenDeck open on `profile` for every device that has it, by setting the device's
/// selected profile before OpenDeck starts (it reads this only at startup).
/// Returns how many devices were set.
pub fn select_opendeck_profile(opendeck_config: &Path, profile: &str) -> Result<usize, String> {
    let profiles = opendeck_config.join("profiles");
    let entries =
        fs::read_dir(&profiles).map_err(|err| format!("{}: {err}", profiles.display()))?;
    let mut set = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let (Some(device), Some("json")) = (
            path.file_stem().and_then(|s| s.to_str()),
            path.extension().and_then(|e| e.to_str()),
        ) else {
            continue;
        };
        if !profiles
            .join(device)
            .join(format!("{profile}.json"))
            .is_file()
        {
            continue;
        }
        let mut config: serde_json::Value = fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(|| serde_json::json!({}));
        let Some(fields) = config.as_object_mut() else {
            continue;
        };
        fields.insert("selected_profile".into(), profile.into());
        let text = serde_json::to_string_pretty(&config).map_err(|err| err.to_string())?;
        fs::write(&path, text).map_err(|err| format!("{}: {err}", path.display()))?;
        set += 1;
    }
    if set == 0 {
        return Err(format!("no OpenDeck device has a profile named {profile}"));
    }
    Ok(set)
}

pub fn start_opendeck(appimage: &Path) -> Result<Child, String> {
    let mut cmd = Command::new(appimage);
    cmd.arg("--hide")
        .current_dir(appimage.parent().unwrap_or(Path::new("/")))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // OpenDeck stalls during startup when it is attached to the menu's terminal, so
    // give it its own session. That also makes it its own process group for stopping.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    cmd.spawn()
        .map_err(|err| format!("could not start OpenDeck: {err}"))
}

/// Ask a process group to exit, escalating to SIGKILL if it hasn't after a few seconds.
/// Runs in the background so the menu stays responsive.
pub fn stop_group_in_background(mut child: Child) {
    thread::spawn(move || {
        let pgid = child.id() as libc::pid_t;
        unsafe { libc::kill(-pgid, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if let Ok(Some(_)) = child.try_wait() {
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        unsafe { libc::kill(-pgid, libc::SIGKILL) };
        let _ = child.wait();
    });
}

pub fn signal_group(child: &Child, signal: libc::c_int) {
    unsafe { libc::kill(-(child.id() as libc::pid_t), signal) };
}

/// Any FlightGear running, whether started from here or not.
pub fn flightgear_running() -> bool {
    processes().any(|p| p.comm == "fgfs")
}

/// OpenDeck runs as its AppImage plus the real binary; either one counts.
pub fn opendeck_running() -> bool {
    processes().any(|p| {
        p.exe
            .as_deref()
            .and_then(|exe| exe.file_name())
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.to_ascii_lowercase().starts_with("opendeck"))
    })
}

struct Proc {
    comm: String,
    exe: Option<PathBuf>,
}

fn processes() -> impl Iterator<Item = Proc> {
    let me = std::process::id().to_string();
    fs::read_dir("/proc")
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.ok())
        .filter(move |entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.chars().all(|c| c.is_ascii_digit()) && name != me
        })
        .filter_map(|entry| {
            let dir = entry.path();
            let comm = fs::read_to_string(dir.join("comm"))
                .ok()?
                .trim()
                .to_string();
            let exe = fs::read_link(dir.join("exe")).ok();
            Some(Proc { comm, exe })
        })
}
