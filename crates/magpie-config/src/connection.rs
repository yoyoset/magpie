//! How a client (CLI, hook, script) finds the API.
//!
//! Precedence: `MAGPIE_URL` / `MAGPIE_TOKEN` env vars, then `endpoint.json` and the
//! credential store. Env first so tests and remote setups can point anywhere.

use crate::endpoint::Endpoint;
use crate::paths::Paths;
use crate::secrets::{SecretStore, API_TOKEN};
use crate::{ConfigError, Result};

pub const ENV_URL: &str = "MAGPIE_URL";
pub const ENV_TOKEN: &str = "MAGPIE_TOKEN";

#[derive(Clone, PartialEq, Eq)]
pub struct Connection {
    pub base_url: String,
    pub token: String,
}

impl std::fmt::Debug for Connection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connection")
            .field("base_url", &self.base_url)
            .field("token", &"<redacted>")
            .finish()
    }
}

/// `env` is injected (normally `|k| std::env::var(k).ok()`) so this is testable.
pub fn resolve(
    env: &dyn Fn(&str) -> Option<String>,
    paths: &Paths,
    secrets: &dyn SecretStore,
) -> Result<Connection> {
    let non_empty = |k: &str| {
        env(k)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };

    let base_url = match non_empty(ENV_URL) {
        Some(url) => url.trim_end_matches('/').to_owned(),
        None => Endpoint::read(paths)?
            .ok_or(ConfigError::NotSetUp("endpoint.json"))?
            .base_url(),
    };
    let token = match non_empty(ENV_TOKEN) {
        Some(t) => t,
        None => secrets
            .get(API_TOKEN)?
            .filter(|t| !t.is_empty())
            .ok_or(ConfigError::NotSetUp("API token in the credential store"))?,
    };
    Ok(Connection { base_url, token })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecrets;
    use std::collections::HashMap;

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k| map.get(k).cloned()
    }

    fn tmp_paths() -> Paths {
        Paths::at(std::env::temp_dir().join(format!("magpie-conn-{}", uuid::Uuid::new_v4())))
    }

    #[test]
    fn env_wins_and_trailing_slash_is_trimmed() {
        let c = resolve(
            &env(&[(ENV_URL, "http://127.0.0.1:9000/"), (ENV_TOKEN, "t")]),
            &tmp_paths(),
            &MemorySecrets::default(),
        )
        .unwrap();
        assert_eq!(c.base_url, "http://127.0.0.1:9000");
        assert_eq!(c.token, "t");
    }

    #[test]
    fn falls_back_to_endpoint_file_and_keyring() {
        let paths = tmp_paths();
        Endpoint { port: 4000 }.write(&paths).unwrap();
        let secrets = MemorySecrets::default();
        secrets.set(API_TOKEN, "from-keyring").unwrap();

        let c = resolve(&env(&[(ENV_URL, "  ")]), &paths, &secrets).unwrap();
        assert_eq!(
            c.base_url, "http://127.0.0.1:4000",
            "blank env var is ignored"
        );
        assert_eq!(c.token, "from-keyring");
        std::fs::remove_dir_all(paths.root()).unwrap();
    }

    #[test]
    fn not_set_up_says_what_is_missing() {
        let err = resolve(&env(&[]), &tmp_paths(), &MemorySecrets::default()).unwrap_err();
        assert!(matches!(err, ConfigError::NotSetUp("endpoint.json")));
        assert!(err.to_string().contains("MAGPIE_URL"));
    }

    #[test]
    fn debug_never_prints_token() {
        let c = Connection {
            base_url: "u".into(),
            token: "secret".into(),
        };
        assert!(!format!("{c:?}").contains("secret"));
    }
}
