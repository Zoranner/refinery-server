use std::sync::Arc;

use crate::{
    config::Config,
    http,
    resource::{HttpResourceFetcher, ResourceFetcher},
    sitemap::{HttpSitemapFetcher, SitemapFetcher},
};

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub http_client: reqwest::Client,
    pub resource_fetcher: Arc<dyn ResourceFetcher>,
    pub sitemap_fetcher: Arc<dyn SitemapFetcher>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config,
            http_client: http::new_client(),
            resource_fetcher: Arc::new(HttpResourceFetcher::new()),
            sitemap_fetcher: Arc::new(HttpSitemapFetcher::new()),
        }
    }

    pub fn for_test() -> Self {
        Self::new(Config::for_test())
    }

    pub fn with_sitemap_fetcher(config: Config, sitemap_fetcher: Arc<dyn SitemapFetcher>) -> Self {
        Self {
            config,
            http_client: http::new_client(),
            resource_fetcher: Arc::new(HttpResourceFetcher::new()),
            sitemap_fetcher,
        }
    }

    pub fn with_resource_fetcher(
        config: Config,
        resource_fetcher: Arc<dyn ResourceFetcher>,
    ) -> Self {
        Self {
            config,
            http_client: http::new_client(),
            resource_fetcher,
            sitemap_fetcher: Arc::new(HttpSitemapFetcher::new()),
        }
    }
}
