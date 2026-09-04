use axum::{Json, extract::State};

use crate::{
    error::ApiError,
    routes::rejection::ApiJson,
    search::{SearchRequest, SearchResponse, searxng},
    state::AppState,
};

pub async fn search(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<SearchRequest>,
) -> Result<Json<SearchResponse>, ApiError> {
    request.validate()?;

    let response =
        searxng::search(&state.http_client, &state.config.searxng_base_url, &request).await?;

    Ok(Json(response))
}
