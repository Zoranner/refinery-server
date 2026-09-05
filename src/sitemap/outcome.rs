use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SitemapStatus {
    Discovered,
    Partial,
    Empty,
    Failed,
    TimedOut,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SitemapSourceStatus {
    NotAttempted,
    Discovered,
    Empty,
    Failed,
    TimedOut,
}

#[derive(Debug, Serialize)]
pub struct SitemapSources {
    pub robots_txt: SitemapSourceStatus,
    pub sitemap: SitemapSourceStatus,
    pub page_links: SitemapSourceStatus,
}

#[derive(Debug, Serialize)]
pub struct SitemapWarning {
    pub source: &'static str,
    pub code: &'static str,
}
