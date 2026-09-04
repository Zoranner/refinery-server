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
        let resource_timeout = config.resource_timeout;
        Self {
            config,
            http_client: http::new_client(),
            resource_fetcher: Arc::new(HttpResourceFetcher::with_timeout(resource_timeout)),
            sitemap_fetcher: Arc::new(HttpSitemapFetcher::new()),
        }
    }

    pub fn for_test() -> Self {
        Self::new(Config::for_test())
    }

    pub fn with_sitemap_fetcher(config: Config, sitemap_fetcher: Arc<dyn SitemapFetcher>) -> Self {
        let resource_timeout = config.resource_timeout;
        Self {
            config,
            http_client: http::new_client(),
            resource_fetcher: Arc::new(HttpResourceFetcher::with_timeout(resource_timeout)),
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
