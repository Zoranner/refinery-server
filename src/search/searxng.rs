use serde::Deserialize;

use crate::{
    error::ApiError,
    search::{SearchRequest, SearchResponse, SearchResult},
};

pub async fn search(
    client: &reqwest::Client,
    base_url: &str,
    request: &SearchRequest,
) -> Result<SearchResponse, ApiError> {
    let endpoint = format!("{}/search", base_url.trim_end_matches('/'));
    let page = request.page.to_string();
    let mut query = vec![
        ("q", request.query.as_str()),
        ("format", "json"),
        ("pageno", page.as_str()),
    ];

    if let Some(language) = &request.language {
        query.push(("language", language));
    }

    let upstream = client
        .get(endpoint)
        .query(&query)
        .send()
        .await
        .map_err(|_| ApiError::search_upstream_failed())?
        .error_for_status()
        .map_err(|_| ApiError::search_upstream_failed())?
        .json::<SearxngResponse>()
        .await
        .map_err(|_| ApiError::search_upstream_failed())?;

    Ok(SearchResponse {
        query: request.query.clone(),
        page: request.page,
        results: upstream
            .results
            .into_iter()
            .take(request.limit.into())
            .map(|result| SearchResult {
                title: result.title,
                url: result.url,
                snippet: result.content,
                published_at: result.published_date,
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct SearxngResponse {
    results: Vec<SearxngResult>,
}

#[derive(Deserialize)]
struct SearxngResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    #[serde(rename = "publishedDate")]
    published_date: Option<String>,
}
