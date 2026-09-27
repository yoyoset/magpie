mod events;
mod health;
mod reminders;

use axum::middleware;
use axum::routing::{delete, get, post};
use axum::Router;

use crate::{auth, AppState};

pub fn router(state: AppState) -> Router {
    let protected = Router::new()
        .route("/v1/events", post(events::create).get(events::list))
        .route("/v1/events/{id}", get(events::detail))
        .route("/v1/events/{id}/ack", post(events::ack))
        .route(
            "/v1/reminders",
            post(reminders::create).get(reminders::list),
        )
        .route("/v1/reminders/{id}", delete(reminders::remove))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_token,
        ));

    Router::new()
        .route("/v1/health", get(health::health))
        .merge(protected)
        .with_state(state)
}
