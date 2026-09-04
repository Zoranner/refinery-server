use axum::{Json, extract::State};

use crate::{
    content::{links, validate_public_url},
    error::ApiError,
    reader::client,
    routes::rejection::ApiJson,
    sitemap::{SitemapRequest, SitemapResponse, SitemapUrl, discover, same_origin},
    state::AppState,
};

pub async fn sitemap(
    State(state): State<AppState>,
    ApiJson(request): ApiJson<SitemapRequest>,
) -> Result<Json<SitemapResponse>, ApiError> {
    let requested_url = validate_public_url(&request.url)?;
    let limit = request.limit;
    let mut response = discover(state.sitemap_fetcher, request).await?;

    if response.urls.is_empty() {
        match client::read(
            &state.http_client,
            &state.config.reader_base_url,
            &requested_url,
            state.config.reader_timeout,
        )
        .await
        {
            Ok(document) => {
                let site_url =
                    url::Url::parse(&response.site_url).map_err(|_| ApiError::fetch_failed())?;

                for link in links::collect(&document.markdown, &document.final_url) {
                    let Ok(candidate) = validate_public_url(&link.url) else {
                        continue;
                    };

                    if same_origin(&site_url, &candidate) {
                        response.urls.push(SitemapUrl {
                            url: candidate.to_string(),
                            source: "page_link",
                        });
                    }

                    if response.urls.len() == limit {
                        response.truncated = true;
                        break;
                    }
                }
            }
            Err(_) => response
                .warnings
                .push("page_link_fallback_failed".to_owned()),
        }
    }

    Ok(Json(response))
}
