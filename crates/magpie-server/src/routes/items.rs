//! HTTP adapter for todos and ideas. Logic lives in `crate::ops`.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use magpie_core::{Item, ItemChange, ItemDraft, ItemId};
use uuid::Uuid;

use crate::ops::{self, ItemListQuery};
use crate::{ApiError, AppState};

pub async fn create(
    State(state): State<AppState>,
    Json(draft): Json<ItemDraft>,
) -> Result<(StatusCode, Json<Item>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(ops::create_item(&state, draft).await?),
    ))
}

pub async fn list(
    State(state): State<AppState>,
    Query(q): Query<ItemListQuery>,
) -> Result<Json<Vec<Item>>, ApiError> {
    Ok(Json(ops::list_items(&state, q).await?))
}

pub async fn detail(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Item>, ApiError> {
    Ok(Json(ops::get_item(&state, ItemId(id)).await?))
}

pub async fn change(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(change): Json<ItemChange>,
) -> Result<Json<Item>, ApiError> {
    Ok(Json(ops::change_item(&state, ItemId(id), change).await?))
}
