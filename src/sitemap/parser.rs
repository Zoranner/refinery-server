use serde::Deserialize;
use url::Url;

use crate::error::ApiError;

pub enum Document {
    UrlSet(Vec<Url>),
    Index(Vec<Url>),
}

pub fn robots_sitemaps(robots: &str) -> Vec<Url> {
    robots
        .lines()
        .filter_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("sitemap").then_some(value.trim())
        })
        .filter_map(|value| Url::parse(value).ok())
        .collect()
}

pub fn parse(xml: &str) -> Result<Document, ApiError> {
    if xml.contains("<sitemapindex") {
        let index: SitemapIndex =
            quick_xml::de::from_str(xml).map_err(|_| ApiError::fetch_failed())?;
        Ok(Document::Index(
            index.entries.into_iter().filter_map(parse_loc).collect(),
        ))
    } else {
        let set: UrlSet = quick_xml::de::from_str(xml).map_err(|_| ApiError::fetch_failed())?;
        Ok(Document::UrlSet(
            set.entries.into_iter().filter_map(parse_loc).collect(),
        ))
    }
}

fn parse_loc(entry: LocationEntry) -> Option<Url> {
    Url::parse(entry.loc.trim()).ok()
}

#[derive(Deserialize)]
struct SitemapIndex {
    #[serde(rename = "sitemap", default)]
    entries: Vec<LocationEntry>,
}

#[derive(Deserialize)]
struct UrlSet {
    #[serde(rename = "url", default)]
    entries: Vec<LocationEntry>,
}

#[derive(Deserialize)]
struct LocationEntry {
    loc: String,
}
