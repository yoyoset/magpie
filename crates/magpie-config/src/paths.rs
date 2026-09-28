//! Data directory layout: `%APPDATA%\magpie\` on Windows.

use std::path::{Path, PathBuf};

use crate::{ConfigError, Result};

pub const HOME_ENV: &str = "MAGPIE_HOME";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    /// `MAGPIE_HOME` if set (portable installs, tests that must not touch real
    /// data), otherwise the per-user default.
    pub fn default_location() -> Result<Self> {
        if let Some(home) = std::env::var_os(HOME_ENV).filter(|v| !v.is_empty()) {
            return Ok(Self::at(home));
        }
        dirs::data_dir()
            .map(|d| Self::at(d.join("magpie")))
            .ok_or(ConfigError::NoDataDir)
    }

    /// A custom root, for tests and portable installs.
    pub fn at(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("magpie.db")
    }

    pub fn config_file(&self) -> PathBuf {
        self.root.join("config.toml")
    }

    pub fn endpoint_file(&self) -> PathBuf {
        self.root.join("endpoint.json")
    }

    pub fn ensure_root(&self) -> Result<()> {
        std::fs::create_dir_all(&self.root).map_err(|source| ConfigError::Io {
            what: format!("create {}", self.root.display()),
            source,
        })
    }
}
