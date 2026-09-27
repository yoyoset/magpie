//! The one place that knows where Magpie keeps things.
//!
//! - [`paths`]: data directory layout.
//! - [`endpoint`]: how clients find the running API (written by the app, read by the CLI).
//! - [`secrets`]: API token in the OS credential store, never in a file.
//! - [`connection`]: resolves URL + token for a client, with env overrides.

pub mod connection;
pub mod endpoint;
pub mod paths;
pub mod secrets;

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("cannot locate the user data directory")]
    NoDataDir,
    #[error("{what}: {source}")]
    Io {
        what: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{what} is malformed: {detail}")]
    Malformed { what: String, detail: String },
    #[error("credential store: {0}")]
    Secret(String),
    #[error(
        "Magpie is not set up on this machine yet (no {0}). Start the Magpie app once, \
         or set MAGPIE_URL and MAGPIE_TOKEN."
    )]
    NotSetUp(&'static str),
}

pub type Result<T> = std::result::Result<T, ConfigError>;
