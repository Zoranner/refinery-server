use axum::{Json, extract::State};

use crate::{
    content::{links, validate_public_url},
    error::ApiError,
    reader::client,
    routes::rejection::ApiJson,
    sitemap::{
        SitemapRequest, SitemapResponse, SitemapSourceStatus, SitemapUrl, discover, push_warning,
        same_origin,
    },
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
                let mut discovered = false;

                for link in links::collect(&document.markdown, &document.final_url) {
                    let Ok(candidate) = validate_public_url(&link.url) else {
                        continue;
                    };

                    if same_origin(&site_url, &candidate) {
                        discovered = true;
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
                response.sources.page_links = if discovered {
                    SitemapSourceStatus::Discovered
                } else {
                    SitemapSourceStatus::Empty
                };
            }
            Err(error) => {
                response.sources.page_links = if error.code == "fetch_timeout" {
                    push_warning(&mut response.warnings, "page_links", "source_timeout");
                    SitemapSourceStatus::TimedOut
                } else {
                    push_warning(&mut response.warnings, "page_links", "fallback_failed");
                    SitemapSourceStatus::Failed
                };
            }
        }
    }

    response.refresh_status();
    Ok(Json(response))
}
