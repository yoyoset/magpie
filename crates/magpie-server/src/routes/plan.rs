//! HTTP adapter for day planning. Logic lives in `crate::ops`.

use axum::extract::{Query, State};
use axum::Json;
use chrono::NaiveDate;
use magpie_core::Plan;
use serde::Deserialize;

use crate::{ops, ApiError, AppState};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DayParam {
    day: NaiveDate,
}

pub async fn draft(
    State(state): State<AppState>,
    Query(p): Query<DayParam>,
) -> Result<Json<Plan>, ApiError> {
    Ok(Json(ops::plan(&state, p.day).await?))
}

pub async fn apply(
    State(state): State<AppState>,
    Json(p): Json<DayParam>,
) -> Result<Json<Plan>, ApiError> {
    Ok(Json(ops::apply_plan(&state, p.day).await?))
}
