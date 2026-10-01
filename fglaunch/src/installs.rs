use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Stable,
    Nightly,
}

impl Kind {
    pub fn label(self) -> &'static str {
        match self {
            Kind::Stable => "stable",
            Kind::Nightly => "nightly",
        }
    }
}

/// One side-by-side install: installs/<name>/{fgfs, fgdata/, home/, aircraft/}.
#[derive(Debug, Clone)]
pub struct Install {
    pub name: String,
    pub dir: PathBuf,
    pub kind: Kind,
    /// Contents of fgdata/version, e.g. "2024.1.7".
    pub data_version: Option<String>,
}

impl Install {
    pub fn fgfs(&self) -> PathBuf {
        self.dir.join("fgfs")
    }

    pub fn fgdata(&self) -> PathBuf {
        self.dir.join("fgdata")
    }

    pub fn home(&self) -> PathBuf {
        self.dir.join("home")
    }

    pub fn aircraft(&self) -> PathBuf {
        self.dir.join("aircraft")
    }
}

/// Complete installs, stable releases first (newest first), then nightlies (newest first).
pub fn scan(installs_dir: &Path) -> Vec<Install> {
    let Ok(entries) = fs::read_dir(installs_dir) else {
        return Vec::new();
    };
    let mut installs: Vec<Install> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let dir = entry.path();
            let name = entry.file_name().to_str()?.to_string();
            if !dir.join("fgfs").exists() || !dir.join("fgdata").is_dir() {
                return None;
            }
            let kind = if name.starts_with("next") {
                Kind::Nightly
            } else {
                Kind::Stable
            };
            let data_version = fs::read_to_string(dir.join("fgdata").join("version"))
                .ok()
                .map(|v| v.trim().to_string());
            Some(Install {
                name,
                dir,
                kind,
                data_version,
            })
        })
        .collect();
    installs.sort_by(|a, b| {
        (a.kind == Kind::Nightly)
            .cmp(&(b.kind == Kind::Nightly))
            .then_with(|| version_key(&b.name).cmp(&version_key(&a.name)))
    });
    installs
}

/// Numeric parts of a name, for ordering "2024.1.10" after "2024.1.9" and dated nightlies by date.
pub fn version_key(name: &str) -> Vec<u64> {
    name.split(|c: char| !c.is_ascii_digit())
        .filter_map(|part| part.parse().ok())
        .collect()
}
