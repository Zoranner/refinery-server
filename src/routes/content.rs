use axum::{Json, extract::State};

use crate::{
    content::{ContentRequest, ResourceKind, kind_for_url},
    error::ApiError,
    material::{TargetFacts, download_only_response, normalize_reader_result_with_options},
    reader::client,
    state::AppState,
};

pub async fn content(
    State(state): State<AppState>,
    Json(request): Json<ContentRequest>,
) -> Result<Json<crate::material::MaterialContentResponse>, ApiError> {
    let url = request.validate()?;
    let requested_kind = kind_for_url(&url);
    if matches!(requested_kind, ResourceKind::Image | ResourceKind::Unknown) {
        let target = TargetFacts::new(
            url.clone(),
            url.clone(),
            requested_kind,
            "application/octet-stream".to_owned(),
        );
        return Ok(Json(download_only_response(target)));
    }

    let document = client::read(
        &state.http_client,
        &state.config.reader_base_url,
        &url,
        state.config.reader_timeout,
    )
    .await?;

    let target = TargetFacts::new(
        url,
        document.final_url,
        requested_kind,
        document.content_type.clone(),
    );
    Ok(Json(normalize_reader_result_with_options(
        target,
        document.markdown,
        document.content_type,
        request.offset,
        request.max_chars,
    )))
}
