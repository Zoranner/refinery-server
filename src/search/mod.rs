pub mod searxng;

use std::collections::HashSet;

use rmcp::schemars::{self, JsonSchema};
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
    pub categories: Option<String>,
    pub safesearch: Option<u8>,
    pub scope: Option<SearchScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SearchScope {
    /// 默认：不限定类别，走实例默认的 general。
    Web,
    /// 技术资料：代码托管、包仓库与问答站。
    Code,
    /// 排障问答：问答站与官方文档。
    Qa,
    /// 包查询：包注册表。
    Package,
    /// 学术资料：论文与预印本。
    Paper,
}

pub fn effective_categories(request: &SearchRequest) -> Option<&str> {
    if let Some(categories) = request.categories.as_deref() {
        return Some(categories);
    }

    match request.scope.unwrap_or(SearchScope::Web) {
        SearchScope::Web => None,
        SearchScope::Code => Some("it"),
        SearchScope::Qa => Some("q&a"),
        SearchScope::Package => Some("packages"),
        SearchScope::Paper => Some("science"),
    }
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
    pub routing: SearchRouting,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub engines: Vec<EngineUsage>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub unresponsive_engines: Vec<UnresponsiveEngine>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub upstream_results: Option<u64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
pub struct SearchRouting {
    pub scope: SearchScope,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categories: Option<String>,
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
    let uses_default_category = effective_categories(request)
        .map(|value| value.split(',').any(|part| part.trim() == "general"))
        .unwrap_or(true);
    let mut warnings = Vec::new();
    if uses_default_category && !unresponsive_engines.is_empty() {
        warnings.push("default_category_degraded");
    }

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
            routing: SearchRouting {
                scope: request.scope.unwrap_or(SearchScope::Web),
                categories: effective_categories(request).map(str::to_owned),
            },
            engines: counts
                .into_iter()
                .map(|(name, results)| EngineUsage { name, results })
                .collect(),
            unresponsive_engines,
            upstream_results,
            warnings,
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

        if self.safesearch.is_some_and(|value| value > 2) {
            return Err(ApiError::invalid_request("safesearch must be 0, 1 or 2"));
        }

        if self
            .categories
            .as_deref()
            .is_some_and(|value| value.trim().is_empty())
        {
            return Err(ApiError::invalid_request("categories must not be blank"));
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

    fn request(categories: Option<&str>) -> SearchRequest {
        request_with_scope(categories, SearchScope::Web)
    }

    fn request_with_scope(categories: Option<&str>, scope: SearchScope) -> SearchRequest {
        SearchRequest {
            query: "q".to_owned(),
            page: 1,
            limit: 10,
            language: None,
            categories: categories.map(str::to_owned),
            safesearch: None,
            scope: Some(scope),
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
            &request(None),
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

    #[test]
    fn degraded_default_category_emits_warning() {
        let default = assemble(
            &request(None),
            vec![result("https://a.example/1", "bing")],
            vec![unresponsive("google")],
            None,
        );
        assert_eq!(default.diagnostics.warnings, ["default_category_degraded"]);

        let general = assemble(
            &request(Some("general,it")),
            vec![result("https://a.example/1", "bing")],
            vec![unresponsive("google")],
            None,
        );
        assert_eq!(general.diagnostics.warnings, ["default_category_degraded"]);

        let explicit = assemble(
            &request(Some("it")),
            vec![result("https://a.example/1", "docker hub")],
            vec![unresponsive("google")],
            None,
        );
        assert!(explicit.diagnostics.warnings.is_empty());
    }

    #[test]
    fn scope_maps_to_categories_unless_explicitly_overridden() {
        assert_eq!(effective_categories(&request(None)), None);
        assert_eq!(
            effective_categories(&request_with_scope(None, SearchScope::Web)),
            None
        );
        assert_eq!(
            effective_categories(&request_with_scope(None, SearchScope::Code)),
            Some("it")
        );
        assert_eq!(
            effective_categories(&request_with_scope(None, SearchScope::Qa)),
            Some("q&a")
        );
        assert_eq!(
            effective_categories(&request_with_scope(None, SearchScope::Package)),
            Some("packages")
        );
        assert_eq!(
            effective_categories(&request_with_scope(None, SearchScope::Paper)),
            Some("science")
        );
        assert_eq!(
            effective_categories(&request_with_scope(Some("science"), SearchScope::Code)),
            Some("science")
        );
    }

    #[test]
    fn response_reports_the_routing_that_was_used() {
        let response = assemble(
            &request_with_scope(None, SearchScope::Qa),
            vec![result("https://a.example/1", "superuser")],
            Vec::new(),
            None,
        );

        assert_eq!(response.diagnostics.routing.scope, SearchScope::Qa);
        assert_eq!(
            response.diagnostics.routing.categories.as_deref(),
            Some("q&a")
        );
    }
}
