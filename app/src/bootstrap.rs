//! Loads everything the services need from disk and the credential store.

use std::sync::{Arc, Mutex};

use magpie_config::paths::Paths;
use magpie_config::secrets::{api_token_or_create, OsKeyring};
use magpie_config::settings::Settings;
use magpie_store::Store;

#[derive(Debug)]
pub struct Boot {
    pub paths: Paths,
    pub settings: Settings,
    pub api_token: String,
    pub store: Arc<Mutex<Store>>,
}

/// Fails on an invalid config instead of starting with half of it.
pub fn boot(paths: Paths) -> anyhow::Result<Boot> {
    paths.ensure_root()?;
    if Settings::write_template_if_missing(&paths)? {
        tracing::info!(path = %paths.config_file().display(), "wrote config template");
    }
    let settings = Settings::load(&paths)?;
    let api_token = api_token_or_create(&OsKeyring)?;
    let store = Store::open(paths.database())?;
    Ok(Boot {
        paths,
        settings,
        api_token,
        store: Arc::new(Mutex::new(store)),
    })
}
