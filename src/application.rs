use std::sync::Arc;

use crate::{
    content::{
        ContentRequest, ResourceKind, kind_for_response, kind_for_url, links, target_content_type,
        validate_public_url,
    },
    error::ApiError,
    material::{
        Diagnostics, DownloadCapability, ExtractionResult, MaterialContentResponse, Pagination,
        TargetFacts,
    },
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
    let requested_kind = kind_for_url(&url);
    if matches!(requested_kind, ResourceKind::Image | ResourceKind::Unknown) {
        return Ok(MaterialContentResponse {
            target: TargetFacts::new(
                url.clone(),
                url.clone(),
                requested_kind,
                target_content_type(&url, &kind_for_url(&url)),
            ),
            extraction: ExtractionResult::download_only(
                "none",
                "该资源不支持文本抽取，请通过 web_download 获取原文件",
            ),
            download: DownloadCapability { available: true },
            pagination: Pagination {
                offset: 0,
                next_offset: None,
                truncated: false,
            },
            diagnostics: Diagnostics {
                upstream_status: None,
                duration_ms: None,
                timeout_seconds: None,
                warnings: Vec::new(),
            },
        });
    }
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
