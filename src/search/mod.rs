pub mod searxng;

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
    pub source_status: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SearchResult {
    pub title: String,
    pub url: String,
    pub snippet: String,
    pub published_at: Option<String>,
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
