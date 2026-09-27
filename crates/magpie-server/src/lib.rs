//! Magpie's local HTTP API.
//!
//! The only way in for agents and scripts. Binds to loopback only and requires
//! a bearer token on everything except `/v1/health`.

mod auth;
mod clock;
mod error;
pub mod ops;
mod routes;

use std::net::{Ipv4Addr, SocketAddr};
use std::sync::{Arc, Mutex};

use axum::extract::DefaultBodyLimit;
use axum::Router;
use magpie_store::Store;
use tokio::net::TcpListener;
use tokio::sync::Notify;

pub use clock::{Clock, SystemClock};
pub use error::ApiError;

/// Largest accepted request body. An event body is capped at 20k chars
/// (up to ~80 KB UTF-8); this leaves room for JSON overhead and nothing more.
pub const MAX_BODY_BYTES: usize = 128 * 1024;

#[derive(Clone)]
pub struct AppState {
    store: Arc<Mutex<Store>>,
    token: Arc<str>,
    clock: Arc<dyn Clock>,
    /// Pinged whenever an event, ack or reminder changes, so the runtime can
    /// react immediately instead of waiting for its next tick.
    changed: Arc<Notify>,
}

impl std::fmt::Debug for AppState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Never print the token.
        f.debug_struct("AppState").finish_non_exhaustive()
    }
}

impl AppState {
    pub fn new(
        store: Arc<Mutex<Store>>,
        token: impl Into<Arc<str>>,
        clock: Arc<dyn Clock>,
        changed: Arc<Notify>,
    ) -> Self {
        Self {
            store,
            token: token.into(),
            clock,
            changed,
        }
    }

    /// Runs a store operation on the blocking pool; `SQLite` calls are synchronous.
    async fn with_store<T, F>(&self, f: F) -> Result<T, ApiError>
    where
        T: Send + 'static,
        F: FnOnce(&mut Store) -> Result<T, ApiError> + Send + 'static,
    {
        let store = Arc::clone(&self.store);
        tokio::task::spawn_blocking(move || {
            let mut guard = store
                .lock()
                .map_err(|_| ApiError::internal("store lock poisoned"))?;
            f(&mut guard)
        })
        .await
        .map_err(|e| ApiError::internal(format!("store task failed: {e}")))?
    }
}

pub fn router(state: AppState) -> Router {
    routes::router(state).layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

/// Binds to `127.0.0.1:port`. There is deliberately no way to bind elsewhere.
pub async fn bind_loopback(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).await
}

pub async fn serve(listener: TcpListener, state: AppState) -> std::io::Result<()> {
    tracing::info!(addr = ?listener.local_addr().ok(), "magpie API listening");
    axum::serve(listener, router(state)).await
}
