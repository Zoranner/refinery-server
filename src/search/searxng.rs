use serde::Deserialize;

use crate::{
    error::ApiError,
    search::{SearchRequest, SearchResponse, SearchResult, UnresponsiveEngine, assemble},
};

pub async fn search(
    client: &reqwest::Client,
    base_url: &str,
    request: &SearchRequest,
) -> Result<SearchResponse, ApiError> {
    search_inner(client, base_url, request)
        .await
        .map_err(|error| error.with_stage("search"))
}

async fn search_inner(
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
        .map_err(|_| ApiError::upstream_unavailable())?
        .error_for_status()
        .map_err(|_| ApiError::upstream_unavailable())?
        .json::<SearxngResponse>()
        .await
        .map_err(|_| ApiError::upstream_unavailable())?;

    Ok(assemble_parsed(request, upstream))
}

#[derive(Deserialize)]
struct SearxngResponse {
    #[serde(default)]
    results: Vec<SearxngResult>,
    #[serde(default)]
    unresponsive_engines: Vec<Vec<String>>,
    number_of_results: Option<u64>,
}

#[derive(Deserialize)]
struct SearxngResult {
    title: String,
    url: String,
    #[serde(default)]
    content: String,
    #[serde(rename = "publishedDate")]
    published_date: Option<String>,
    #[serde(default)]
    engines: Vec<String>,
}

fn assemble_parsed(request: &SearchRequest, upstream: SearxngResponse) -> SearchResponse {
    let unresponsive_engines = upstream
        .unresponsive_engines
        .into_iter()
        .map(|entry| UnresponsiveEngine {
            name: entry.first().cloned().unwrap_or_default(),
            reason: entry.get(1).cloned().unwrap_or_default(),
        })
        .collect();
    let results = upstream
        .results
        .into_iter()
        .map(|result| SearchResult {
            title: result.title,
            url: result.url,
            snippet: result.content,
            published_at: result.published_date,
            engines: result.engines,
        })
        .collect();

    assemble(
        request,
        results,
        unresponsive_engines,
        upstream.number_of_results,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchSourceStatus;

    fn request() -> SearchRequest {
        SearchRequest {
            query: "trafilatura".to_owned(),
            page: 1,
            limit: 5,
            language: None,
        }
    }

    #[test]
    fn parses_engine_facts_from_upstream_payload() {
        let payload = r#"{
            "results": [
                {"title":"a","url":"https://a.example/1","content":"x","engines":["bing"]},
                {"title":"b","url":"https://b.example/2","content":"y","engines":["wikipedia"]}
            ],
            "unresponsive_engines": [["brave","暂停服务: 请求过于频繁"],["google","暂停服务: 拒绝访问"]],
            "number_of_results": 123
        }"#;
        let parsed: SearxngResponse = serde_json::from_str(payload).unwrap();
        let response = assemble_parsed(&request(), parsed);

        assert_eq!(response.results.len(), 2);
        assert_eq!(response.results[0].engines, ["bing"]);
        assert_eq!(response.diagnostics.unresponsive_engines.len(), 2);
        assert_eq!(response.diagnostics.unresponsive_engines[0].name, "brave");
        assert_eq!(
            response.diagnostics.source_status,
            SearchSourceStatus::Partial
        );
        assert_eq!(response.diagnostics.upstream_results, Some(123));
    }

    #[test]
    fn missing_optional_facts_do_not_break_parsing() {
        let payload = r#"{"results":[{"title":"a","url":"https://a.example/1"}]}"#;
        let parsed: SearxngResponse = serde_json::from_str(payload).unwrap();
        let response = assemble_parsed(&request(), parsed);

        assert_eq!(response.diagnostics.source_status, SearchSourceStatus::Ok);
        assert!(response.diagnostics.unresponsive_engines.is_empty());
        assert_eq!(response.diagnostics.upstream_results, None);
    }
}
