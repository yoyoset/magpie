//! Throwaway API server for trying the CLI before the desktop app exists.
//!
//! ```bash
//! MAGPIE_DEV_TOKEN=dev cargo run -p magpie-server --example dev_server -- 47615
//! MAGPIE_URL=http://127.0.0.1:47615 MAGPIE_TOKEN=dev magpie list
//! ```
//! In-memory store: everything is gone when it exits.

use std::sync::{Arc, Mutex};

use magpie_server::{bind_loopback, serve, AppState, SystemClock};
use magpie_store::Store;
use tokio::sync::Notify;

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let port = std::env::args()
        .nth(1)
        .and_then(|p| p.parse().ok())
        .unwrap_or(47_615);
    let token = std::env::var("MAGPIE_DEV_TOKEN").unwrap_or_else(|_| "dev".into());
    let store = Store::open_in_memory().expect("in-memory store");
    let state = AppState::new(
        Arc::new(Mutex::new(store)),
        token,
        Arc::new(SystemClock),
        Arc::new(Notify::new()),
    );
    let listener = bind_loopback(port).await?;
    eprintln!("dev server on http://{}", listener.local_addr()?);
    serve(listener, state).await
}
