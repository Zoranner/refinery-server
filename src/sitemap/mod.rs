pub mod client;
pub mod outcome;
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
pub use outcome::{SitemapSourceStatus, SitemapSources, SitemapStatus, SitemapWarning};

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
    pub status: SitemapStatus,
    pub sources: SitemapSources,
    pub urls: Vec<SitemapUrl>,
    pub truncated: bool,
    pub warnings: Vec<SitemapWarning>,
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
    let mut warnings = Vec::new();
    let mut robots_source = SourceTracker::default();
    let robots_sitemaps = match fetcher.get(&robots_url).await {
        Ok(robots) => {
            let sitemaps = parser::robots_sitemaps(&robots);
            robots_source.success(!sitemaps.is_empty());
            sitemaps
        }
        Err(error) => {
            robots_source.failure(&error, "robots_txt", &mut warnings);
            Vec::new()
        }
    };
    let mut documents: VecDeque<Url> = robots_sitemaps
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
    let mut sitemap_source = SourceTracker::default();

    while let Some(document_url) = documents.pop_front() {
        if visited.len() >= MAX_SITEMAP_DOCUMENTS {
            truncated = true;
            break;
        }

        if !visited.insert(document_url.clone()) {
            continue;
        }

        sitemap_source.attempted = true;
        let document = match fetcher.get(&document_url).await {
            Ok(document) => document,
            Err(error) => {
                sitemap_source.failure(&error, "sitemap", &mut warnings);
                continue;
            }
        };

        match parser::parse(&document) {
            Ok(parser::Document::Index(children)) => {
                sitemap_source.success(false);
                for child in children {
                    if same_origin(&site_url, &child) {
                        documents.push_back(child);
                    }
                }
            }
            Ok(parser::Document::UrlSet(candidates)) => {
                let mut discovered = false;
                for candidate in candidates {
                    if same_origin(&site_url, &candidate) && found.insert(candidate.clone()) {
                        discovered = true;
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
                sitemap_source.success(discovered);
            }
            Err(error) => sitemap_source.failure(&error, "sitemap", &mut warnings),
        }

        if truncated {
            break;
        }
    }

    sitemap_source.finalize(urls.iter().any(|url| url.source == "sitemap"));

    let mut response = SitemapResponse {
        requested_url: request.url,
        site_url: site_url.to_string(),
        status: SitemapStatus::Empty,
        sources: SitemapSources {
            robots_txt: robots_source.status,
            sitemap: sitemap_source.status,
            page_links: SitemapSourceStatus::NotAttempted,
        },
        urls,
        truncated,
        warnings,
    };
    response.refresh_status();
    Ok(response)
}

#[derive(Debug)]
pub(crate) struct SourceTracker {
    status: SitemapSourceStatus,
    attempted: bool,
    failed: bool,
    timed_out: bool,
}

impl Default for SourceTracker {
    fn default() -> Self {
        Self {
            status: SitemapSourceStatus::NotAttempted,
            attempted: false,
            failed: false,
            timed_out: false,
        }
    }
}

impl SourceTracker {
    fn success(&mut self, discovered: bool) {
        self.attempted = true;
        if !self.failed && !self.timed_out {
            self.status = if discovered {
                SitemapSourceStatus::Discovered
            } else {
                SitemapSourceStatus::Empty
            };
        }
    }

    fn failure(
        &mut self,
        error: &ApiError,
        source: &'static str,
        warnings: &mut Vec<SitemapWarning>,
    ) {
        self.attempted = true;
        if error.code == "fetch_timeout" {
            self.timed_out = true;
            self.status = SitemapSourceStatus::TimedOut;
            push_warning(warnings, source, "source_timeout");
        } else {
            self.failed = true;
            if !self.timed_out {
                self.status = SitemapSourceStatus::Failed;
            }
            push_warning(warnings, source, "source_failed");
        }
    }

    fn finalize(&mut self, discovered: bool) {
        if self.failed || self.timed_out {
            return;
        }
        if self.attempted {
            self.status = if discovered {
                SitemapSourceStatus::Discovered
            } else {
                SitemapSourceStatus::Empty
            };
        }
    }
}

impl SitemapResponse {
    pub(crate) fn refresh_status(&mut self) {
        self.status = aggregate_status_from_sources(
            !self.urls.is_empty(),
            [
                &self.sources.robots_txt,
                &self.sources.sitemap,
                &self.sources.page_links,
            ],
        );
    }
}

fn aggregate_status_from_sources(
    has_urls: bool,
    sources: [&SitemapSourceStatus; 3],
) -> SitemapStatus {
    let timed_out = sources
        .iter()
        .any(|source| matches!(source, SitemapSourceStatus::TimedOut));
    let failed = sources
        .iter()
        .any(|source| matches!(source, SitemapSourceStatus::Failed));

    if has_urls {
        if timed_out || failed {
            SitemapStatus::Partial
        } else {
            SitemapStatus::Discovered
        }
    } else if timed_out {
        SitemapStatus::TimedOut
    } else if failed {
        SitemapStatus::Failed
    } else {
        SitemapStatus::Empty
    }
}

pub(crate) fn push_warning(
    warnings: &mut Vec<SitemapWarning>,
    source: &'static str,
    code: &'static str,
) {
    if !warnings
        .iter()
        .any(|warning| warning.source == source && warning.code == code)
    {
        warnings.push(SitemapWarning { source, code });
    }
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
