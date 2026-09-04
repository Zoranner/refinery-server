use axum::{Json, extract::State};

use crate::{
    content::{ContentRequest, ContentResponse, ResourceKind, kind_for_url, response},
    error::ApiError,
    reader::client,
    state::AppState,
};

pub async fn content(
    State(state): State<AppState>,
    Json(request): Json<ContentRequest>,
) -> Result<Json<ContentResponse>, ApiError> {
    let url = request.validate()?;
    if matches!(
        kind_for_url(&url),
        ResourceKind::Image | ResourceKind::Unknown
    ) {
        return Err(ApiError::resource_download_required(&url));
    }

    let document = client::read(
        &state.http_client,
        &state.config.reader_base_url,
        &url,
        state.config.reader_timeout,
    )
    .await?;

    Ok(Json(response(
        &request,
        document.final_url,
        document.content_type,
        document.markdown,
    )))
}
