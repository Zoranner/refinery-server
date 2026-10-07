pub mod searxng;

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::error::ApiError;

#[derive(Debug, Deserialize)]
pub struct SearchRequest {
    pub query: String,
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_limit")]
    pub limit: u8,
    pub language: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub query: String,
    pub page: u32,
    pub results: Vec<SearchResult>,
    pub pagination: SearchPagination,
    pub diagnostics: SearchDiagnostics,
}

#[derive(Debug, Serialize)]
pub struct SearchPagination {
    pub requested_page: u32,
    pub has_more: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct SearchDiagnostics {
    pub source_status: SearchSourceStatus,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub engines: Vec<EngineUsage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresponsive_engines: Vec<UnresponsiveEngine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_results: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<&'static str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SearchSourceStatus {
    Ok,
    Partial,
    Empty,
    Failed,
}

#[derive(Debug, Serialize)]
pub struct EngineUsage {
    pub name: String,
    pub results: usize,
}

#[derive(Debug, Serialize)]
pub struct UnresponsiveEngine {
    pub name: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_at: Option<String>,
    pub engines: Vec<String>,
}

pub fn derive_status(results: usize, unresponsive: usize) -> SearchSourceStatus {
    match (results, unresponsive) {
        (0, 0) => SearchSourceStatus::Empty,
        (0, _) => SearchSourceStatus::Failed,
        (_, 0) => SearchSourceStatus::Ok,
        (_, _) => SearchSourceStatus::Partial,
    }
}

pub fn assemble(
    request: &SearchRequest,
    results: Vec<SearchResult>,
    unresponsive_engines: Vec<UnresponsiveEngine>,
    upstream_results: Option<u64>,
) -> SearchResponse {
    let mut seen = HashSet::new();
    let results = results
        .into_iter()
        .filter(|item| seen.insert(item.url.clone()))
        .take(request.limit.into())
        .collect::<Vec<_>>();

    let mut counts: Vec<(String, usize)> = Vec::new();
    for item in &results {
        for engine in &item.engines {
            match counts.iter_mut().find(|(name, _)| name == engine) {
                Some((_, count)) => *count += 1,
                None => counts.push((engine.clone(), 1)),
            }
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    let status = derive_status(results.len(), unresponsive_engines.len());

    SearchResponse {
        query: request.query.clone(),
        page: request.page,
        results,
        pagination: SearchPagination {
            requested_page: request.page,
            has_more: None,
        },
        diagnostics: SearchDiagnostics {
            source_status: status,
            engines: counts
                .into_iter()
                .map(|(name, results)| EngineUsage { name, results })
                .collect(),
            unresponsive_engines,
            upstream_results,
            warnings: Vec::new(),
        },
    }
}

impl SearchRequest {
    pub fn validate(&self) -> Result<(), ApiError> {
        if self.query.trim().is_empty() {
            return Err(ApiError::invalid_request("query must not be blank"));
        }

        if self.page == 0 {
            return Err(ApiError::invalid_request("page must be at least 1"));
        }

        if self.limit == 0 || self.limit > 20 {
            return Err(ApiError::invalid_request("limit must be between 1 and 20"));
        }

        Ok(())
    }
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u8 {
    10
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(url: &str, engine: &str) -> SearchResult {
        SearchResult {
            title: "t".to_owned(),
            url: url.to_owned(),
            snippet: "s".to_owned(),
            published_at: None,
            engines: vec![engine.to_owned()],
        }
    }

    fn request() -> SearchRequest {
        SearchRequest {
            query: "q".to_owned(),
            page: 1,
            limit: 10,
            language: None,
        }
    }

    fn unresponsive(name: &str) -> UnresponsiveEngine {
        UnresponsiveEngine {
            name: name.to_owned(),
            reason: "验证码".to_owned(),
        }
    }

    #[test]
    fn status_follows_results_and_unresponsive_engines() {
        assert_eq!(derive_status(3, 0), SearchSourceStatus::Ok);
        assert_eq!(derive_status(3, 2), SearchSourceStatus::Partial);
        assert_eq!(derive_status(0, 0), SearchSourceStatus::Empty);
        assert_eq!(derive_status(0, 2), SearchSourceStatus::Failed);
    }

    #[test]
    fn assemble_deduplicates_urls_and_counts_engines() {
        let response = assemble(
            &request(),
            vec![
                result("https://a.example/1", "bing"),
                result("https://a.example/1", "bing"),
                result("https://b.example/2", "wikipedia"),
            ],
            vec![unresponsive("brave")],
            Some(42),
        );

        assert_eq!(response.results.len(), 2);
        assert_eq!(
            response.diagnostics.source_status,
            SearchSourceStatus::Partial
        );
        let names = response
            .diagnostics
            .engines
            .iter()
            .map(|usage| usage.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["bing", "wikipedia"]);
        assert_eq!(response.diagnostics.upstream_results, Some(42));
        assert_eq!(response.diagnostics.unresponsive_engines[0].name, "brave");
    }
}
