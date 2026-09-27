//! `endpoint.json`: the running app announces its port here so clients can find it.
//!
//! The port is chosen by the app (a fixed default, or an OS-assigned one if that
//! is taken), so clients must not hard-code it.

use serde::{Deserialize, Serialize};

use crate::paths::Paths;
use crate::{ConfigError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Endpoint {
    pub port: u16,
}

impl Endpoint {
    pub fn base_url(self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }

    pub fn write(self, paths: &Paths) -> Result<()> {
        paths.ensure_root()?;
        let file = paths.endpoint_file();
        let json = serde_json::to_string_pretty(&self).expect("endpoint serializes");
        // Write-then-rename so a client never reads a half-written file.
        let tmp = file.with_extension("json.tmp");
        std::fs::write(&tmp, json)
            .and_then(|()| std::fs::rename(&tmp, &file))
            .map_err(|source| ConfigError::Io {
                what: format!("write {}", file.display()),
                source,
            })
    }

    /// `Ok(None)` when the app has never run here.
    pub fn read(paths: &Paths) -> Result<Option<Self>> {
        let file = paths.endpoint_file();
        match std::fs::read_to_string(&file) {
            Ok(s) => serde_json::from_str(&s)
                .map(Some)
                .map_err(|e| ConfigError::Malformed {
                    what: file.display().to_string(),
                    detail: e.to_string(),
                }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(source) => Err(ConfigError::Io {
                what: format!("read {}", file.display()),
                source,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_read_roundtrip_and_missing_is_none() {
        let dir = std::env::temp_dir().join(format!("magpie-cfg-{}", uuid::Uuid::new_v4()));
        let paths = Paths::at(&dir);
        assert_eq!(Endpoint::read(&paths).unwrap(), None);

        Endpoint { port: 47_615 }.write(&paths).unwrap();
        let got = Endpoint::read(&paths).unwrap().unwrap();
        assert_eq!(got.base_url(), "http://127.0.0.1:47615");

        // Rewrite must replace the existing file (rename-over on Windows).
        Endpoint { port: 47_616 }.write(&paths).unwrap();
        assert_eq!(Endpoint::read(&paths).unwrap().unwrap().port, 47_616);

        std::fs::write(paths.endpoint_file(), "{not json").unwrap();
        assert!(matches!(
            Endpoint::read(&paths),
            Err(ConfigError::Malformed { .. })
        ));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
