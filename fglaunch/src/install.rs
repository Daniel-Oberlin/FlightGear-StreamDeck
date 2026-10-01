//! Installing offers into installs/<name>/ and removing installs.
//!
//! An install is built in its final folder and only becomes visible to the menu when
//! the `fgfs` link is created as the very last step, so a failed or cancelled install
//! never shows up half-finished; its folder is removed instead.

use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

use reqwest::blocking::Client;

use crate::config::Config;
use crate::installs::{Install, Kind, version_key};
use crate::updates::{Offer, OfferKind};

const FGDATA_REPO: &str = "https://gitlab.com/flightgear/fgdata.git";
const C172P_REPO: &str = "https://github.com/c172p-team/c172p.git";

/// Progress reports from a background task to the menu.
pub enum Progress {
    Step(String),
    /// Bytes downloaded so far and the total, for the current step.
    Bytes(u64, u64),
    Done(Result<String, String>),
}

/// Shared with the menu so it can cancel a running task.
#[derive(Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);

impl Cancel {
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    fn check(&self) -> Result<(), String> {
        if self.0.load(Ordering::Relaxed) {
            Err("cancelled".into())
        } else {
            Ok(())
        }
    }
}

/// Everything an install needs from the menu, captured when it starts.
pub struct Job {
    pub fg_base: PathBuf,
    pub installs: Vec<Install>,
    pub last_flown: Option<String>,
    pub cancel: Cancel,
    pub progress: Sender<Progress>,
}

impl Job {
    pub fn new(
        cfg: &Config,
        installs: &[Install],
        last_flown: Option<String>,
        progress: Sender<Progress>,
    ) -> Job {
        Job {
            fg_base: cfg.fg_base.clone(),
            installs: installs.to_vec(),
            last_flown,
            cancel: Cancel::default(),
            progress,
        }
    }

    fn step(&self, text: impl Into<String>) {
        let _ = self.progress.send(Progress::Step(text.into()));
    }

    fn shared(&self) -> PathBuf {
        self.fg_base.join("shared")
    }
}

/// Install an offer. Reports Progress::Done when finished, successfully or not.
pub fn install(job: Job, offer: Offer) {
    let dir = job.fg_base.join("installs").join(&offer.name);
    let result = if dir.join("fgfs").symlink_metadata().is_ok() {
        Err(format!("{} is already installed", offer.name))
    } else {
        // A folder without fgfs is left over from an install that didn't finish.
        let _ = remove_install_dir(&job.shared(), &dir);
        let result = fs::create_dir_all(&dir)
            .map_err(|err| format!("{}: {err}", dir.display()))
            .and_then(|()| match &offer.kind {
                OfferKind::Stable { version, data_url } => {
                    install_stable(&job, &offer, &dir, version, data_url)
                }
                OfferKind::Nightly { built_at, .. } => {
                    install_nightly(&job, &offer, &dir, built_at)
                }
            });
        if result.is_err() {
            job.step("cleaning up");
            let _ = remove_install_dir(&job.shared(), &dir);
        }
        result.map(|()| format!("Installed {}.", offer.name))
    };
    let _ = job.progress.send(Progress::Done(result));
}

fn install_stable(
    job: &Job,
    offer: &Offer,
    dir: &Path,
    version: &str,
    data_url: &str,
) -> Result<(), String> {
    let appimage = dir.join(file_name(&offer.appimage_url));
    download(
        job,
        &offer.appimage_url,
        &appimage,
        "downloading FlightGear",
    )?;
    make_executable(&appimage)?;

    let downloads = job.fg_base.join("downloads");
    fs::create_dir_all(&downloads).map_err(|err| format!("{}: {err}", downloads.display()))?;
    let archive = downloads.join(file_name(data_url));
    download(job, data_url, &archive, "downloading data")?;

    job.step("extracting data");
    let fgdata = dir.join("fgdata");
    fs::create_dir_all(&fgdata).map_err(|err| err.to_string())?;
    let extracted = run(
        job,
        Command::new("tar")
            .arg("-xJf")
            .arg(&archive)
            .arg("-C")
            .arg(&fgdata)
            .arg("--strip-components=1"),
    );
    let _ = fs::remove_file(&archive);
    extracted?;
    let found = data_version(&fgdata);
    if found.as_deref() != Some(version) {
        return Err(format!(
            "data reports version {found:?}, expected {version}"
        ));
    }

    finish(job, dir, &appimage, &series(version))
}

fn install_nightly(job: &Job, offer: &Offer, dir: &Path, built_at: &str) -> Result<(), String> {
    let appimage = dir.join(file_name(&offer.appimage_url));
    download(
        job,
        &offer.appimage_url,
        &appimage,
        "downloading FlightGear",
    )?;
    make_executable(&appimage)?;

    // One shallow repository for all nightlies; each gets a worktree at the fgdata
    // commit that was current when its build started (CI builds against next's tip).
    let repo = job.shared().join("fgdata.git");
    // Nightlies are only downloadable for 30 days, so this history always covers the commit.
    let since = "--shallow-since=45 days ago";
    if repo.join("HEAD").exists() {
        job.step("updating the shared fgdata repository");
        run(
            job,
            Command::new("git").arg("-C").arg(&repo).args([
                "fetch",
                "--quiet",
                since,
                "origin",
                "+next:next",
            ]),
        )?;
    } else {
        job.step("cloning fgdata (first nightly only, a few GB)");
        let _ = fs::remove_dir_all(&repo);
        run(
            job,
            Command::new("git")
                .args([
                    "clone",
                    "--quiet",
                    "--bare",
                    "--single-branch",
                    "--branch",
                    "next",
                    since,
                ])
                .arg(FGDATA_REPO)
                .arg(&repo),
        )?;
    }
    let commit = output(Command::new("git").arg("-C").arg(&repo).args([
        "rev-list",
        "-1",
        &format!("--before={built_at}"),
        "next",
    ]))?;
    if commit.is_empty() {
        return Err(format!("no fgdata commit found before {built_at}"));
    }
    job.step(format!(
        "checking out fgdata {}",
        &commit[..commit.len().min(10)]
    ));
    run(
        job,
        Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "add", "--quiet", "--detach"])
            .arg(dir.join("fgdata"))
            .arg(&commit),
    )?;

    // The C172P isn't in next's fgdata or aircraft catalog; share one clone.
    let aircraft = job.shared().join("aircraft");
    let c172p = aircraft.join("c172p");
    if c172p.join(".git").exists() {
        job.step("updating the C172P");
        run(
            job,
            Command::new("git")
                .arg("-C")
                .arg(&c172p)
                .args(["pull", "--quiet", "--ff-only"]),
        )?;
    } else {
        job.step("cloning the C172P");
        fs::create_dir_all(&aircraft).map_err(|err| err.to_string())?;
        run(
            job,
            Command::new("git")
                .args(["clone", "--quiet", "--depth", "1", C172P_REPO])
                .arg(&c172p),
        )?;
    }
    symlink(Path::new("../../shared/aircraft"), dir.join("aircraft"))
        .map_err(|err| err.to_string())?;

    let version = data_version(&dir.join("fgdata")).ok_or("fgdata has no version file")?;
    finish(job, dir, &appimage, &series(&version))
}

/// Seed the home folder, then create the fgfs link that makes the install visible.
fn finish(job: &Job, dir: &Path, appimage: &Path, series: &str) -> Result<(), String> {
    job.step("setting up the home folder");
    let home = dir.join("home");
    fs::create_dir_all(&home).map_err(|err| err.to_string())?;
    seed_home(job, &home, series);
    let name = appimage.file_name().ok_or("bad AppImage name")?;
    symlink(name, dir.join("fgfs")).map_err(|err| err.to_string())
}

/// Copy launcher settings from the most recent install of the same release series
/// (without paths into that install) and aircraft state from the install flown last.
fn seed_home(job: &Job, home: &Path, series: &str) {
    let last = job
        .last_flown
        .as_ref()
        .and_then(|name| job.installs.iter().find(|i| &i.name == name));
    let same_series =
        |i: &&Install| i.data_version.as_deref().map(self::series).as_deref() == Some(series);
    let settings_from = last.filter(same_series).or_else(|| {
        job.installs
            .iter()
            .filter(same_series)
            .max_by_key(|i| version_key(&i.name))
    });
    if let Some(from) = settings_from {
        let ini = Path::new("FlightGear").join(format!("FlightGear_{series}.ini"));
        if let Ok(text) = fs::read_to_string(from.home().join(&ini)) {
            let kept: String = text
                .lines()
                .filter(|line| {
                    !["aircraft-cache=", "selected-aircraft=", "recent-aircraft"]
                        .iter()
                        .any(|key| line.starts_with(key))
                })
                .map(|line| format!("{line}\n"))
                .collect();
            let _ = fs::create_dir_all(home.join("FlightGear"));
            let _ = fs::write(home.join(&ini), kept);
        }
    }
    let aircraft_from = last.or_else(|| {
        job.installs
            .iter()
            .filter(|i| i.kind == Kind::Stable)
            .max_by_key(|i| version_key(&i.name))
    });
    if let Some(from) = aircraft_from
        && let Ok(entries) = fs::read_dir(from.home().join("aircraft-data"))
    {
        let to = home.join("aircraft-data");
        let _ = fs::create_dir_all(&to);
        for entry in entries.flatten() {
            let _ = fs::copy(entry.path(), to.join(entry.file_name()));
        }
    }
}

/// Remove an install. Reports Progress::Done when finished.
pub fn remove(job: Job, install: Install) {
    job.step(format!("removing {}", install.name));
    let result = remove_install_dir(&job.shared(), &install.dir)
        .map(|()| format!("Removed {}.", install.name));
    let _ = job.progress.send(Progress::Done(result));
}

/// Delete an install folder, unregistering a nightly's fgdata worktree first.
/// Shared scenery, fgdata and aircraft are never touched: the aircraft link is removed, not followed.
fn remove_install_dir(shared: &Path, dir: &Path) -> Result<(), String> {
    let repo = shared.join("fgdata.git");
    if dir.join("fgdata").join(".git").is_file() {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "remove", "--force"])
            .arg(dir.join("fgdata"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
    match fs::remove_dir_all(dir) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(format!("{}: {err}", dir.display())),
    }
    if repo.exists() {
        let _ = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(["worktree", "prune"])
            .status();
    }
    Ok(())
}

/// Download to `dest`, verifying the size against Content-Length.
fn download(job: &Job, url: &str, dest: &Path, what: &str) -> Result<(), String> {
    job.step(what);
    let client = Client::builder()
        .user_agent(concat!("fglaunch/", env!("CARGO_PKG_VERSION")))
        .connect_timeout(Duration::from_secs(20))
        .build()
        .map_err(|err| err.to_string())?;
    let mut response = client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .map_err(|err| format!("{what}: {err}"))?;
    let total = response.content_length().unwrap_or(0);
    let partial = dest.with_extension("part");
    let mut file = File::create(&partial).map_err(|err| format!("{}: {err}", partial.display()))?;
    let mut buf = vec![0u8; 1 << 18];
    let mut done = 0u64;
    let mut reported = Instant::now();
    let result = loop {
        if let Err(err) = job.cancel.check() {
            break Err(err);
        }
        match response.read(&mut buf) {
            Ok(0) => break Ok(()),
            Ok(n) => {
                if let Err(err) = file.write_all(&buf[..n]) {
                    break Err(format!("{}: {err}", partial.display()));
                }
                done += n as u64;
                if reported.elapsed() > Duration::from_millis(200) {
                    let _ = job.progress.send(Progress::Bytes(done, total));
                    reported = Instant::now();
                }
            }
            Err(err) => break Err(format!("{what}: {err}")),
        }
    };
    let result = result.and_then(|()| {
        if total > 0 && done != total {
            Err(format!("{what}: got {done} of {total} bytes"))
        } else {
            file.sync_all().map_err(|err| err.to_string())
        }
    });
    match result {
        Ok(()) => fs::rename(&partial, dest).map_err(|err| err.to_string()),
        Err(err) => {
            let _ = fs::remove_file(&partial);
            Err(err)
        }
    }
}

/// Run a command to completion, stopping it if the task is cancelled.
fn run(job: &Job, cmd: &mut Command) -> Result<(), String> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("{program}: {err}"))?;
    let mut stderr = child.stderr.take();
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    });
    let status = loop {
        if job.cancel.check().is_err() {
            let _ = child.kill();
            let _ = child.wait();
            return Err("cancelled".into());
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(err) => return Err(format!("{program}: {err}")),
        }
    };
    let errors = reader.join().unwrap_or_default();
    if status.success() {
        Ok(())
    } else {
        let last = errors
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("failed");
        Err(format!("{program}: {last}"))
    }
}

fn output(cmd: &mut Command) -> Result<String, String> {
    let out = cmd
        .stderr(Stdio::null())
        .output()
        .map_err(|err| err.to_string())?;
    if !out.status.success() {
        return Err(format!("{:?} failed", cmd.get_program()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn make_executable(path: &Path) -> Result<(), String> {
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|err| err.to_string())
}

fn data_version(fgdata: &Path) -> Option<String> {
    fs::read_to_string(fgdata.join("version"))
        .ok()
        .map(|v| v.trim().to_string())
}

/// "2024.1.7" -> "2024.1", the release series that shares launcher settings.
fn series(version: &str) -> String {
    version.split('.').take(2).collect::<Vec<_>>().join(".")
}

fn file_name(url: &str) -> String {
    url.rsplit('/').next().unwrap_or("download").to_string()
}
