use axum::{Json, extract::State};

use crate::{
    content::{ContentKind, ContentRequest, ContentResponse, kind_for_url, response},
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
        ContentKind::Image | ContentKind::Unknown
    ) {
        return Err(ApiError::resource_download_required(&url));
    }

    let document = client::read(&state.http_client, &state.config.reader_base_url, &url).await?;

    Ok(Json(response(
        &request,
        document.final_url,
        document.content_type,
        document.markdown,
    )))
}
