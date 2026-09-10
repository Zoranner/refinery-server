use std::sync::Arc;

use crate::{
    content::{ContentRequest, kind_for_response, links, validate_public_url},
    error::ApiError,
    material::{MaterialContentResponse, TargetFacts},
    reader::client,
    sitemap::{
        SitemapRequest, SitemapResponse, SitemapSourceStatus, SitemapUrl, discover, same_origin,
    },
    state::AppState,
};

pub async fn search(
    state: &AppState,
    request: crate::search::SearchRequest,
) -> Result<crate::search::SearchResponse, ApiError> {
    request.validate()?;
    crate::search::searxng::search(&state.http_client, &state.config.searxng_base_url, &request)
        .await
}

pub async fn read(
    state: &AppState,
    request: ContentRequest,
) -> Result<MaterialContentResponse, ApiError> {
    let url = request.validate()?;
    let document = client::read(
        &state.http_client,
        &state.config.reader_base_url,
        &url,
        state.config.reader_timeout,
    )
    .await?;
    let kind = kind_for_response(&document.final_url, &document.content_type);
    let target = TargetFacts::new(url, document.final_url, kind, document.content_type.clone());
    Ok(crate::material::normalize_reader_result_with_options(
        target,
        document.markdown,
        document.content_type,
        request.offset,
        request.max_chars,
    ))
}

pub async fn explore(
    state: &AppState,
    request: SitemapRequest,
) -> Result<SitemapResponse, ApiError> {
    let requested_url = validate_public_url(&request.url)?;
    let limit = request.limit;
    let mut response = discover(Arc::clone(&state.sitemap_fetcher), request).await?;
    if response.urls.is_empty() {
        let document = client::read(
            &state.http_client,
            &state.config.reader_base_url,
            &requested_url,
            state.config.reader_timeout,
        )
        .await;
        match document {
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
                    SitemapSourceStatus::TimedOut
                } else {
                    SitemapSourceStatus::Failed
                };
            }
        }
    }
    response.refresh_status();
    Ok(response)
}

pub fn validate_download(url: &str) -> Result<url::Url, ApiError> {
    validate_public_url(url)
}
