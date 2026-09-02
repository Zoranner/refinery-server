pub mod client;
pub mod parser;

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::{content::validate_public_url, error::ApiError};

pub use client::HttpSitemapFetcher;

const MAX_SITEMAP_DOCUMENTS: usize = 20;

#[async_trait]
pub trait SitemapFetcher: Send + Sync {
    async fn get(&self, url: &Url) -> Result<String, ApiError>;
}

#[derive(Clone, Debug, Deserialize)]
pub struct SitemapRequest {
    pub url: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Debug, Serialize)]
pub struct SitemapResponse {
    pub requested_url: String,
    pub site_url: String,
    pub urls: Vec<SitemapUrl>,
    pub truncated: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct SitemapUrl {
    pub url: String,
    pub source: &'static str,
}

impl SitemapRequest {
    fn validate(&self) -> Result<Url, ApiError> {
        if !(1..=500).contains(&self.limit) {
            return Err(ApiError::invalid_request("limit must be between 1 and 500"));
        }

        validate_public_url(&self.url)
    }
}

pub async fn discover(
    fetcher: Arc<dyn SitemapFetcher>,
    request: SitemapRequest,
) -> Result<SitemapResponse, ApiError> {
    let requested_url = request.validate()?;
    let site_url = site_root(&requested_url)?;
    let robots_url = site_url
        .join("robots.txt")
        .map_err(|_| ApiError::fetch_failed())?;
    let robots = fetcher.get(&robots_url).await.unwrap_or_default();
    let mut documents: VecDeque<Url> = parser::robots_sitemaps(&robots)
        .into_iter()
        .filter(|url| same_origin(&site_url, url))
        .collect();

    if documents.is_empty() {
        documents.push_back(
            site_url
                .join("sitemap.xml")
                .map_err(|_| ApiError::fetch_failed())?,
        );
    }

    let mut visited = HashSet::new();
    let mut urls = Vec::new();
    let mut found = HashSet::new();
    let mut truncated = false;

    while let Some(document_url) = documents.pop_front() {
        if visited.len() >= MAX_SITEMAP_DOCUMENTS {
            truncated = true;
            break;
        }

        if !visited.insert(document_url.clone()) {
            continue;
        }

        let document = match fetcher.get(&document_url).await {
            Ok(document) => document,
            Err(_) => continue,
        };

        match parser::parse(&document)? {
            parser::Document::Index(children) => {
                for child in children {
                    if same_origin(&site_url, &child) {
                        documents.push_back(child);
                    }
                }
            }
            parser::Document::UrlSet(candidates) => {
                for candidate in candidates {
                    if same_origin(&site_url, &candidate) && found.insert(candidate.clone()) {
                        urls.push(SitemapUrl {
                            url: candidate.to_string(),
                            source: "sitemap",
                        });
                    }

                    if urls.len() == request.limit {
                        truncated = true;
                        break;
                    }
                }
            }
        }

        if truncated {
            break;
        }
    }

    Ok(SitemapResponse {
        requested_url: request.url,
        site_url: site_url.to_string(),
        urls,
        truncated,
        warnings: Vec::new(),
    })
}

fn default_limit() -> usize {
    100
}

fn site_root(url: &Url) -> Result<Url, ApiError> {
    let mut root = url.clone();
    root.set_path("/");
    root.set_query(None);
    root.set_fragment(None);
    Ok(root)
}

pub fn same_origin(site_url: &Url, candidate: &Url) -> bool {
    site_url.scheme() == candidate.scheme()
        && site_url.host() == candidate.host()
        && site_url.port_or_known_default() == candidate.port_or_known_default()
}
