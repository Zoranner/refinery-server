use axum::{
    body::Body,
    extract::{Query, State},
    http::{HeaderValue, StatusCode, header},
    response::Response,
};
use serde::Deserialize;

use crate::{content::validate_public_url, error::ApiError, state::AppState};

#[derive(Deserialize)]
pub struct ResourceQuery {
    pub url: String,
}

pub async fn resource(
    State(state): State<AppState>,
    Query(query): Query<ResourceQuery>,
) -> Result<Response, ApiError> {
    let url = validate_public_url(&query.url)?;
    let resource = state.resource_fetcher.get(&url).await?;

    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            HeaderValue::from_str(&resource.content_type).map_err(|_| ApiError::fetch_failed())?,
        )
        .header(
            header::CONTENT_DISPOSITION,
            HeaderValue::from_static("attachment"),
        )
        .header(
            header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        )
        .body(Body::from(resource.bytes))
        .map_err(|_| ApiError::fetch_failed())
}
