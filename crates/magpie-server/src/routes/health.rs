use axum::Json;
use serde_json::{json, Value};

/// Unauthenticated liveness probe. Reveals nothing but the version.
pub async fn health() -> Json<Value> {
    Json(json!({ "status": "ok", "version": env!("CARGO_PKG_VERSION") }))
}
