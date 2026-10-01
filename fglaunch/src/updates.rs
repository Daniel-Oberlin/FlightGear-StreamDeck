//! Finds FlightGear builds that can be installed: stable releases from the
//! download mirror and nightly builds of `next` from GitLab CI.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::installs::{Install, Kind, version_key};

const MIRROR: &str = "https://mirrors.ibiblio.org/flightgear/ftp/";
/// flightgear/fgmeta, whose scheduled pipelines build the nightlies.
const FGMETA_API: &str = "https://gitlab.com/api/v4/projects/179838";
/// Re-check at most this often unless asked to.
pub const MAX_AGE: Duration = Duration::from_secs(6 * 3600);

/// Something that can be installed. `name` is the folder it installs to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Offer {
    pub name: String,
    pub kind: OfferKind,
    pub appimage_url: String,
    /// Total download size, when the server reported it.
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OfferKind {
    Stable {
        version: String,
        data_url: String,
    },
    Nightly {
        /// When the pipeline started; the matching fgdata is `next` as of this time.
        built_at: String,
        /// GitLab deletes the AppImage after this.
        expires_at: Option<String>,
    },
}

/// Everything the last check found, before filtering against what is installed.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Found {
    /// Unix seconds.
    pub checked_at: u64,
    pub stable: Vec<Offer>,
    pub nightly: Option<Offer>,
    /// Problems reaching either source; results from the other are still used.
    pub errors: Vec<String>,
}

impl Found {
    pub fn load_cached() -> Option<Found> {
        let text = fs::read_to_string(cache_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn save(&self) {
        let path = cache_path();
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = fs::write(path, text);
        }
    }

    pub fn is_stale(&self) -> bool {
        now().saturating_sub(self.checked_at) > MAX_AGE.as_secs()
    }

    /// Stable releases newer than the newest installed one (or just the newest release
    /// when none is installed), and the latest nightly if it is newer than any installed.
    pub fn offers_for(&self, installed: &[Install]) -> Vec<Offer> {
        let mut offers = Vec::new();
        let newest_stable = installed
            .iter()
            .filter(|i| i.kind == Kind::Stable)
            .map(|i| version_key(&i.name))
            .max();
        let mut stable: Vec<&Offer> = self.stable.iter().collect();
        stable.sort_by_key(|o| std::cmp::Reverse(version_key(&o.name)));
        match newest_stable {
            Some(newest) => offers.extend(
                stable
                    .into_iter()
                    .filter(|o| version_key(&o.name) > newest)
                    .cloned(),
            ),
            None => offers.extend(stable.first().map(|o| (*o).clone())),
        }
        if let Some(nightly) = &self.nightly {
            let newest_nightly = installed
                .iter()
                .filter(|i| i.kind == Kind::Nightly)
                .map(|i| version_key(&i.name))
                .max();
            let installed_already = installed.iter().any(|i| i.name == nightly.name);
            if !installed_already && newest_nightly.is_none_or(|n| version_key(&nightly.name) > n) {
                offers.push(nightly.clone());
            }
        }
        offers
    }
}

/// Query both sources. Network failures are recorded, not fatal: a source that can't be
/// reached keeps its results from the previous check, so going offline doesn't hide offers.
pub fn check(previous: Option<&Found>) -> Found {
    let mut found = Found {
        checked_at: now(),
        stable: previous.map(|p| p.stable.clone()).unwrap_or_default(),
        nightly: previous.and_then(|p| p.nightly.clone()),
        errors: Vec::new(),
    };
    let client = match Client::builder()
        .user_agent(concat!("fglaunch/", env!("CARGO_PKG_VERSION")))
        .timeout(Duration::from_secs(20))
        .build()
    {
        Ok(client) => client,
        Err(err) => {
            found.errors.push(err.to_string());
            return found;
        }
    };
    match stable_releases(&client) {
        Ok(stable) => found.stable = stable,
        Err(err) => found.errors.push(format!("stable releases: {err}")),
    }
    match latest_nightly(&client) {
        Ok(nightly) => found.nightly = nightly,
        Err(err) => found.errors.push(format!("nightlies: {err}")),
    }
    // Nothing was reached: keep the time of the last check that did get through.
    if found.errors.len() == 2
        && let Some(previous) = previous
    {
        found.checked_at = previous.checked_at;
    }
    found
}

/// Releases in the two newest release-YYYY.N series that have both an AppImage and data.
fn stable_releases(client: &Client) -> Result<Vec<Offer>, String> {
    let index = get_text(client, MIRROR)?;
    let mut series: Vec<String> = hrefs(&index)
        .filter_map(|h| {
            h.strip_prefix("release-")?
                .strip_suffix('/')
                .map(str::to_string)
        })
        .collect();
    series.sort_by_key(|s| version_key(s));
    series.dedup();

    let mut offers = Vec::new();
    for s in series.iter().rev().take(2) {
        let dir = format!("{MIRROR}release-{s}/");
        let listing = get_text(client, &dir)?;
        let files: Vec<&str> = hrefs(&listing).collect();
        for file in &files {
            let Some(version) = file
                .strip_prefix("flightgear-")
                .and_then(|f| f.strip_suffix("-linux-amd64.AppImage"))
            else {
                continue;
            };
            let data = format!("FlightGear-{version}-data.txz");
            if !files.contains(&data.as_str()) {
                continue;
            }
            offers.push(Offer {
                name: version.to_string(),
                kind: OfferKind::Stable {
                    version: version.to_string(),
                    data_url: format!("{dir}{data}"),
                },
                appimage_url: format!("{dir}{file}"),
                size: None,
            });
        }
    }
    // Sizes cost a request per file, so only fetch them for the newest few, which are
    // the only ones that can be offered unless installs are several releases behind.
    offers.sort_by_key(|o| std::cmp::Reverse(version_key(&o.name)));
    for offer in offers.iter_mut().take(3) {
        if let OfferKind::Stable { data_url, .. } = &offer.kind {
            offer.size = content_length(client, &offer.appimage_url)
                .zip(content_length(client, data_url))
                .map(|(a, b)| a + b);
        }
    }
    Ok(offers)
}

#[derive(Deserialize)]
struct Pipeline {
    id: u64,
    created_at: String,
}

#[derive(Deserialize)]
struct Job {
    id: u64,
    name: String,
    artifacts_expire_at: Option<String>,
}

/// The newest successful scheduled build of `next` whose AppImage can still be downloaded.
fn latest_nightly(client: &Client) -> Result<Option<Offer>, String> {
    let pipelines: Vec<Pipeline> = get_json(
        client,
        &format!("{FGMETA_API}/pipelines?ref=next&status=success&source=schedule&per_page=5"),
    )?;
    for pipeline in pipelines {
        let jobs: Vec<Job> = get_json(
            client,
            &format!("{FGMETA_API}/pipelines/{}/jobs?per_page=100", pipeline.id),
        )?;
        let Some(job) = jobs.into_iter().find(|j| j.name == "linux-appimage") else {
            continue;
        };
        // The AppImage is named after the UTC date the pipeline ran.
        let date: String = pipeline
            .created_at
            .chars()
            .take(10)
            .filter(|c| c.is_ascii_digit())
            .collect();
        let appimage_url = format!(
            "{FGMETA_API}/jobs/{}/artifacts/flightgear-{date}-linux-amd64.AppImage",
            job.id
        );
        let Some(size) = content_length(client, &appimage_url) else {
            continue;
        };
        return Ok(Some(Offer {
            name: format!("next-{date}"),
            kind: OfferKind::Nightly {
                built_at: pipeline.created_at,
                expires_at: job.artifacts_expire_at,
            },
            appimage_url,
            size: Some(size),
        }));
    }
    Ok(None)
}

fn get_text(client: &Client, url: &str) -> Result<String, String> {
    client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.text())
        .map_err(|err| short_error(&err))
}

fn get_json<T: serde::de::DeserializeOwned>(client: &Client, url: &str) -> Result<T, String> {
    client
        .get(url)
        .send()
        .and_then(|r| r.error_for_status())
        .and_then(|r| r.json())
        .map_err(|err| short_error(&err))
}

/// Size from a HEAD request. Read from the header: reqwest's content_length()
/// describes the (empty) body of a HEAD response.
fn content_length(client: &Client, url: &str) -> Option<u64> {
    let response = client.head(url).send().ok()?.error_for_status().ok()?;
    response
        .headers()
        .get(reqwest::header::CONTENT_LENGTH)?
        .to_str()
        .ok()?
        .parse()
        .ok()
}

fn short_error(err: &reqwest::Error) -> String {
    if err.is_timeout() {
        "timed out".into()
    } else if err.is_connect() {
        "offline or server unreachable".into()
    } else if let Some(status) = err.status() {
        format!("HTTP {status}")
    } else {
        err.to_string()
    }
}

/// href="..." targets in a directory listing.
fn hrefs(html: &str) -> impl Iterator<Item = &str> {
    html.split("href=\"")
        .skip(1)
        .filter_map(|rest| rest.split('"').next())
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn cache_path() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache")
        });
    base.join("fglaunch").join("updates.json")
}
